//! Entity identifiers.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// A 128-bit GnuCash GUID, written as 32 lowercase hex digits. `5.17:libgnucash/engine/guid.cpp`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Guid([u8; 16]);

/// A string that is not 32 hex digits.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("not a GUID: {0:?}")]
pub struct ParseGuidError(pub String);

impl Guid {
    /// The all-zero GUID GnuCash uses for "none" (`guid_null`).
    pub const NULL: Self = Self([0; 16]);

    /// A GUID from its 16 bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The 16 bytes.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// True for the all-zero GUID.
    pub fn is_null(&self) -> bool {
        self.0 == [0; 16]
    }

    /// Parse 32 hex digits, either case.
    pub fn parse(s: &str) -> Result<Self, ParseGuidError> {
        s.parse()
    }
}

impl FromStr for Guid {
    type Err = ParseGuidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseGuidError(s.to_owned());
        let s = s.trim();
        if s.len() != 32 || !s.is_ascii() {
            return Err(err());
        }
        let mut bytes = [0_u8; 16];
        bytes.iter_mut().enumerate().try_for_each(|(i, b)| {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| err())?;
            Ok(())
        })?;
        Ok(Self(bytes))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_lowercase() {
        let s = "3e8c5d7f0a1b2c3d4e5f60718293a4b5";
        assert_eq!(Guid::parse(s).unwrap().to_string(), s);
        assert_eq!(Guid::parse(&s.to_uppercase()).unwrap().to_string(), s);
        assert!(Guid::parse("xyz").is_err());
        assert!(Guid::NULL.is_null());
    }
}
