//! Import files as they arrive in the params, turned into text (`design/api/import.md`, "`import.preview`").
//!
//! A file comes as text, already Unicode, or as bytes with the encoding the client names (a WHATWG label, decoded
//! with `encoding_rs`). GnuCash guesses a CSV file's encoding and drops byte sequences that are not valid in it
//! (`5.17:gnucash/import-export/csv-imp/gnc-tokenizer.cpp:64-114`); red-ale takes the client's encoding and refuses
//! the file at the first invalid sequence, naming its line, so no text is lost without the user knowing.

use encoding_rs::{DecoderResult, Encoding};

use crate::ServiceError;
use crate::config::invalid;

/// What an import file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    /// Text, already Unicode.
    Text(String),
    /// Bytes in the file's encoding.
    Bytes(Vec<u8>),
}

/// One import file (`ImportFile`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFile {
    /// Its name, shown in messages.
    pub name: Option<String>,
    /// Its content.
    pub content: FileContent,
    /// The encoding of [`FileContent::Bytes`], a WHATWG label; refused with text.
    pub encoding: Option<String>,
}

/// The file as text. `default_encoding` applies to bytes when the file names no encoding (for CSV, the layout's).
/// A byte-order mark is removed: for bytes, one that names UTF-8 or UTF-16 decides the encoding, as the WHATWG
/// decode algorithm has it.
pub fn decode(index: usize, file: &ImportFile, default_encoding: &str) -> Result<String, ServiceError> {
    match (&file.content, &file.encoding) {
        (FileContent::Text(_), Some(_)) => Err(invalid(format!(
            "import file {index}: encoding is for base64 content; text is already Unicode"
        ))),
        (FileContent::Text(text), None) => Ok(text.strip_prefix('\u{feff}').unwrap_or(text).to_owned()),
        (FileContent::Bytes(bytes), encoding) => {
            let label = encoding.as_deref().unwrap_or(default_encoding);
            let encoding = Encoding::for_label(label.trim().as_bytes()).ok_or_else(|| {
                invalid(format!(
                    "import file {index}: {label:?} is not an encoding label (https://encoding.spec.whatwg.org/#names-and-labels)"
                ))
            })?;
            decode_bytes(index, bytes, encoding)
        }
    }
}

/// Decode without replacement, stopping at the first malformed sequence.
fn decode_bytes(index: usize, bytes: &[u8], encoding: &'static Encoding) -> Result<String, ServiceError> {
    let mut decoder = encoding.new_decoder();
    let mut out = String::with_capacity(
        decoder
            .max_utf8_buffer_length_without_replacement(bytes.len())
            .unwrap_or(bytes.len()),
    );
    let mut read = 0;
    loop {
        let (result, n) = decoder.decode_to_string_without_replacement(&bytes[read..], &mut out, true);
        read += n;
        match result {
            DecoderResult::InputEmpty => return Ok(out),
            DecoderResult::OutputFull => out.reserve(bytes.len().saturating_sub(read).saturating_mul(3).max(16)),
            DecoderResult::Malformed(..) => {
                let line = out.matches('\n').count() + 1;
                let message = format!(
                    "bytes not valid in {} at line {line}; is the file in another encoding?",
                    encoding.name()
                );
                return Err(ServiceError::ImportFile {
                    file: index,
                    line: Some(line),
                    reason: "encoding",
                    message,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(b: &[u8], encoding: Option<&str>) -> ImportFile {
        ImportFile {
            name: None,
            content: FileContent::Bytes(b.to_vec()),
            encoding: encoding.map(str::to_owned),
        }
    }

    #[test]
    fn encodings_and_byte_order_marks() {
        assert_eq!(decode(0, &bytes(b"caf\xe9\n", Some("iso-8859-1")), "utf-8").unwrap(), "café\n");
        assert_eq!(decode(0, &bytes(b"caf\xe9\n", None), "windows-1252").unwrap(), "café\n");
        assert_eq!(decode(0, &bytes(b"\xef\xbb\xbfDate\n", None), "utf-8").unwrap(), "Date\n");
        let utf16: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain("Dé\n".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode(0, &bytes(&utf16, Some("utf-16")), "utf-8").unwrap(), "Dé\n");
        let text = ImportFile {
            name: None,
            content: FileContent::Text("\u{feff}a,b".to_owned()),
            encoding: None,
        };
        assert_eq!(decode(0, &text, "utf-8").unwrap(), "a,b");
    }

    #[test]
    fn invalid_bytes_name_their_line_and_labels_are_checked() {
        let err = decode(2, &bytes(b"a\nb\n\xff\n", None), "utf-8").unwrap_err();
        assert!(
            matches!(
                err,
                ServiceError::ImportFile {
                    file: 2,
                    line: Some(3),
                    reason: "encoding",
                    ..
                }
            ),
            "{err}"
        );
        assert!(decode(0, &bytes(b"a", Some("klingon")), "utf-8").is_err());
        let text = ImportFile {
            name: None,
            content: FileContent::Text("a".to_owned()),
            encoding: Some("utf-8".to_owned()),
        };
        assert!(decode(0, &text, "utf-8").is_err());
    }
}
