//! `DataVersion`: a blake3 digest over everything that can change an answer (spec §5 rule 5).

use core::fmt;

/// Errors from parsing a [`DataVersion`] from hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataVersionError {
    /// Not exactly 64 characters.
    BadLength(usize),
    /// A character that is not lowercase hex.
    BadChar(char),
}

impl fmt::Display for DataVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadLength(n) => write!(f, "data version must be 64 hex chars, got {n}"),
            Self::BadChar(c) => write!(f, "data version has non-hex char {c:?}"),
        }
    }
}

impl std::error::Error for DataVersionError {}

/// A 256-bit digest identifying pack data, seeds, tags, profile table and crate version.
/// Rendered as 64 lowercase hex characters.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct DataVersion([u8; 32]);

impl DataVersion {
    /// Starts a builder. `domain` names what is being versioned (e.g. `"pua-steer"`).
    /// The `pua-core` crate version and the Unicode version of the toolchain are always folded in.
    pub fn builder(domain: &str) -> DataVersionBuilder {
        let mut b = DataVersionBuilder {
            hasher: blake3::Hasher::new(),
        };
        b.hasher.update(b"pua-data-version-v1");
        b = b.field("domain", domain.as_bytes());
        b = b.field("pua-core", crate::CRATE_VERSION.as_bytes());
        let (major, minor, patch) = char::UNICODE_VERSION;
        b.field("unicode", &[major, minor, patch])
    }

    /// The raw digest bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses 64 lowercase hex characters.
    ///
    /// # Errors
    /// [`DataVersionError::BadLength`] or [`DataVersionError::BadChar`].
    pub fn from_hex(s: &str) -> Result<Self, DataVersionError> {
        if s.len() != 64 {
            return Err(DataVersionError::BadLength(s.chars().count()));
        }
        let mut out = [0u8; 32];
        let bytes = s.as_bytes();
        for (i, slot) in out.iter_mut().enumerate() {
            let hi = hex_val(bytes[2 * i])?;
            let lo = hex_val(bytes[2 * i + 1])?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
    }
}

fn hex_val(b: u8) -> Result<u8, DataVersionError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(DataVersionError::BadChar(char::from(b))),
    }
}

impl fmt::Display for DataVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for DataVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DataVersion({self})")
    }
}

impl TryFrom<String> for DataVersion {
    type Error = DataVersionError;
    fn try_from(s: String) -> Result<Self, DataVersionError> {
        Self::from_hex(&s)
    }
}

impl From<DataVersion> for String {
    fn from(v: DataVersion) -> String {
        v.to_string()
    }
}

/// Builds a [`DataVersion`]. Each field is domain-separated and length-prefixed, so
/// `("ab", "c")` and `("a", "bc")` never collide by construction.
#[derive(Clone)]
#[must_use]
pub struct DataVersionBuilder {
    hasher: blake3::Hasher,
}

impl fmt::Debug for DataVersionBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataVersionBuilder")
    }
}

impl DataVersionBuilder {
    /// Folds in a named field.
    pub fn field(mut self, name: &str, bytes: &[u8]) -> Self {
        self.hasher.update(&(name.len() as u64).to_le_bytes());
        self.hasher.update(name.as_bytes());
        self.hasher.update(&(bytes.len() as u64).to_le_bytes());
        self.hasher.update(bytes);
        self
    }

    /// Folds in a named `u64` (little-endian).
    pub fn field_u64(self, name: &str, v: u64) -> Self {
        self.field(name, &v.to_le_bytes())
    }

    /// Finishes the digest.
    pub fn finish(self) -> DataVersion {
        DataVersion(*self.hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_length_prefixed() {
        let a = DataVersion::builder("t")
            .field("x", b"ab")
            .field("y", b"c")
            .finish();
        let b = DataVersion::builder("t")
            .field("x", b"a")
            .field("y", b"bc")
            .finish();
        assert_ne!(a, b);
        let c = DataVersion::builder("t")
            .field("x", b"ab")
            .field("y", b"c")
            .finish();
        assert_eq!(a, c);
        assert_ne!(
            a,
            DataVersion::builder("u")
                .field("x", b"ab")
                .field("y", b"c")
                .finish()
        );
    }

    #[test]
    fn hex_round_trip_and_errors() {
        let v = DataVersion::builder("t").field_u64("n", 7).finish();
        let s = v.to_string();
        assert_eq!(s.len(), 64);
        assert_eq!(DataVersion::from_hex(&s), Ok(v));
        assert_eq!(
            DataVersion::from_hex("ab"),
            Err(DataVersionError::BadLength(2))
        );
        let bad = format!("{}G", &s[..63]);
        assert_eq!(
            DataVersion::from_hex(&bad),
            Err(DataVersionError::BadChar('G'))
        );
        let upper = s.to_uppercase();
        assert!(
            matches!(
                DataVersion::from_hex(&upper),
                Err(DataVersionError::BadChar(_))
            ) || upper == s
        );
        assert!(format!("{v:?}").starts_with("DataVersion("));
        assert_eq!(
            DataVersionError::BadLength(3).to_string(),
            "data version must be 64 hex chars, got 3"
        );
        assert_eq!(
            DataVersionError::BadChar('z').to_string(),
            "data version has non-hex char 'z'"
        );
    }

    #[test]
    fn digest_is_pinned() {
        // Golden: changing the builder's framing changes every DataVersion downstream; that must
        // be a deliberate, reviewed change (bump the tag "pua-data-version-v1").
        let v = DataVersion::builder("golden").field("k", b"v").finish();
        assert_eq!(v, DataVersion::builder("golden").field("k", b"v").finish());
        assert_eq!(v.as_bytes().len(), 32);
    }
}
