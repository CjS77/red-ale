//! A book file's text as a stream: gzip undone, UTF-8 checked and line ends normalised as it is read, so the whole
//! text is never held at once.

use std::io::{self, BufRead, Read};

use flate2::bufread::GzDecoder;

use crate::XmlError;

/// How much decoded text is read at a time.
const CHUNK: usize = 64 * 1024;

/// The file's bytes, decompressed if they are gzipped.
enum Source<R: BufRead> {
    Plain(R),
    Gzip(GzDecoder<R>),
}

impl<R: BufRead> Read for Source<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(r) => r.read(buf),
            Self::Gzip(r) => r.read(buf),
        }
    }
}

/// The decoded text of a book file. Invalid UTF-8 anywhere is an error (`InvalidData`, read as
/// [`XmlError::Encoding`]), and a raw CR LF or lone CR reads as LF, XML's end-of-line handling.
///
/// The first error from the file is kept, so the caller can report it rather than whatever the XML tokeniser made of
/// it.
pub(crate) struct TextStream<R: BufRead> {
    source: Source<io::Chain<io::Cursor<Vec<u8>>, R>>,
    compressed: bool,
    /// Bytes read but not yet decoded: at most the start of one incomplete UTF-8 sequence between reads.
    pending: Vec<u8>,
    raw: Vec<u8>,
    out: Vec<u8>,
    pos: usize,
    after_cr: bool,
    eof: bool,
    error: Option<io::Error>,
}

impl<R: BufRead> TextStream<R> {
    /// Start reading a file, decompressing it if it starts with the gzip magic number.
    pub(crate) fn new(mut reader: R) -> Result<Self, XmlError> {
        let magic = read_magic(&mut reader).map_err(from_io)?;
        let compressed = crate::book::is_gzip(&magic);
        let chained = io::Cursor::new(magic).chain(reader);
        let source = if compressed { Source::Gzip(GzDecoder::new(chained)) } else { Source::Plain(chained) };
        Ok(Self { source, compressed, pending: Vec::new(), raw: vec![0; CHUNK], out: Vec::new(), pos: 0, after_cr: false, eof: false, error: None })
    }

    /// The file was gzip-compressed.
    pub(crate) fn compressed(&self) -> bool {
        self.compressed
    }

    /// The first error reading the file, if any, as the reader reports it.
    pub(crate) fn take_error(&mut self) -> Option<XmlError> {
        self.error.take().map(from_io)
    }

    /// At least `len` bytes of decoded text from the current position, or all that is left if the file is shorter.
    pub(crate) fn peek(&mut self, len: usize) -> io::Result<&[u8]> {
        while self.out.len() - self.pos < len && !self.eof {
            self.out.drain(..self.pos);
            self.pos = 0;
            self.refill()?;
        }
        Ok(&self.out[self.pos..])
    }

    /// Read the rest of the file, decoding it, to find any error after the point the XML stopped being read.
    pub(crate) fn drain(&mut self) -> Option<XmlError> {
        match io::copy(self, &mut io::sink()) {
            Ok(_) => None,
            Err(e) => Some(self.take_error().unwrap_or_else(|| from_io(e))),
        }
    }

    /// Decode the next chunk of the file onto the end of `out`.
    fn refill(&mut self) -> io::Result<()> {
        if let Some(e) = &self.error {
            return Err(io::Error::new(e.kind(), e.to_string()));
        }
        let read = self.source.read(&mut self.raw).and_then(|n| self.decode(n));
        read.map_err(|e| {
            let copy = io::Error::new(e.kind(), e.to_string());
            self.error = Some(e);
            copy
        })
    }

    fn decode(&mut self, n: usize) -> io::Result<()> {
        if n == 0 {
            self.eof = true;
            return if self.pending.is_empty() { Ok(()) } else { Err(invalid_utf8()) };
        }
        self.pending.extend_from_slice(&self.raw[..n]);
        let valid = match std::str::from_utf8(&self.pending) {
            Ok(text) => text.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => return Err(invalid_utf8()),
        };
        let mut after_cr = self.after_cr;
        self.out.extend(self.pending.drain(..valid).filter_map(|b| {
            let skip = after_cr && b == b'\n';
            after_cr = b == b'\r';
            (!skip).then_some(if b == b'\r' { b'\n' } else { b })
        }));
        self.after_cr = after_cr;
        Ok(())
    }
}

impl<R: BufRead> Read for TextStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<R: BufRead> BufRead for TextStream<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.pos == self.out.len() && !self.eof {
            self.out.clear();
            self.pos = 0;
            self.refill()?;
        }
        Ok(&self.out[self.pos..])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = (self.pos + amt).min(self.out.len());
    }
}

/// The first two bytes, or fewer if the file is shorter.
fn read_magic(reader: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut magic = Vec::with_capacity(2);
    reader.by_ref().take(2).read_to_end(&mut magic)?;
    Ok(magic)
}

fn invalid_utf8() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "the book is not valid UTF-8")
}

/// An error reading a file: invalid data is a bad encoding, as when the whole file was decoded at once.
pub(crate) fn from_io(e: io::Error) -> XmlError {
    if e.kind() == io::ErrorKind::InvalidData { XmlError::Encoding } else { XmlError::Io(e) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(bytes: &[u8]) -> Result<Vec<u8>, XmlError> {
        let mut stream = TextStream::new(bytes)?;
        let mut out = Vec::new();
        match stream.read_to_end(&mut out) {
            Ok(_) => Ok(out),
            Err(e) => Err(stream.take_error().unwrap_or_else(|| from_io(e))),
        }
    }

    #[test]
    fn normalises_line_ends_across_reads() {
        let text = "a\r\nb\rc\r".repeat(CHUNK / 3);
        assert_eq!(decoded(text.as_bytes()).unwrap(), "a\nb\nc\n".repeat(CHUNK / 3).into_bytes());
    }

    #[test]
    fn utf8_split_between_reads_is_kept_and_invalid_utf8_refused() {
        let text = format!("{}é€", "x".repeat(CHUNK - 1));
        assert_eq!(decoded(text.as_bytes()).unwrap(), text.as_bytes());
        assert!(matches!(decoded(&[b'a', 0xff]), Err(XmlError::Encoding)));
        assert!(matches!(decoded(&[b'a', 0xc3]), Err(XmlError::Encoding)));
    }
}
