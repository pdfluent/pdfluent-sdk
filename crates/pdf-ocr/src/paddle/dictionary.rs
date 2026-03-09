//! Character dictionary for CTC decoding.

use std::path::Path;

/// Character dictionary for CTC decoding.
///
/// Index 0 is always the CTC blank token.
/// Characters are loaded from a dict.txt file with one character per line.
pub struct Dictionary {
    chars: Vec<String>,
}

impl Dictionary {
    /// Load from a dict.txt file (one character per line).
    pub fn from_file(path: &Path) -> Result<Self, DictionaryError> {
        let data = std::fs::read(path)
            .map_err(|e| DictionaryError::Io(format!("{}: {e}", path.display())))?;
        Self::from_bytes(&data)
    }

    /// Load from embedded bytes (one character per line, UTF-8).
    pub fn from_bytes(data: &[u8]) -> Result<Self, DictionaryError> {
        let text = std::str::from_utf8(data)
            .map_err(|e| DictionaryError::Parse(format!("invalid UTF-8: {e}")))?;

        // Blank token at index 0, then each line is a character
        let mut chars = vec![String::new()]; // index 0 = blank
        for line in text.lines() {
            if !line.is_empty() {
                chars.push(line.to_string());
            }
        }

        if chars.len() < 2 {
            return Err(DictionaryError::Parse(
                "dictionary must contain at least one character".into(),
            ));
        }

        Ok(Self { chars })
    }

    /// Number of entries including the blank token at index 0.
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// Whether the dictionary is empty (only blank token).
    pub fn is_empty(&self) -> bool {
        self.chars.len() <= 1
    }

    /// Get the character at `index`. Index 0 is the CTC blank token (returns `None`).
    pub fn get(&self, index: usize) -> Option<&str> {
        if index == 0 {
            return None; // blank token
        }
        self.chars.get(index).map(|s| s.as_str())
    }
}

/// Errors from dictionary operations.
#[derive(Debug)]
pub enum DictionaryError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for DictionaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "dictionary I/O error: {msg}"),
            Self::Parse(msg) => write!(f, "dictionary parse error: {msg}"),
        }
    }
}

impl std::error::Error for DictionaryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_and_lookup() {
        let dict = Dictionary::from_bytes(b"a\nb\nc\nd\n").unwrap();
        assert_eq!(dict.len(), 5); // 4 chars + blank token
        assert_eq!(dict.get(0), None); // blank
        assert_eq!(dict.get(1), Some("a"));
        assert_eq!(dict.get(2), Some("b"));
        assert_eq!(dict.get(3), Some("c"));
        assert_eq!(dict.get(4), Some("d"));
        assert_eq!(dict.get(5), None); // out of range
    }

    #[test]
    fn empty_dict_is_error() {
        let result = Dictionary::from_bytes(b"");
        assert!(result.is_err());
    }

    #[test]
    fn dict_with_unicode() {
        let dict = Dictionary::from_bytes("日\n本\n語\n".as_bytes()).unwrap();
        assert_eq!(dict.len(), 4);
        assert_eq!(dict.get(1), Some("日"));
        assert_eq!(dict.get(2), Some("本"));
        assert_eq!(dict.get(3), Some("語"));
    }

    #[test]
    fn is_empty() {
        let dict = Dictionary::from_bytes(b"a\n").unwrap();
        assert!(!dict.is_empty());
    }

    #[test]
    fn skips_empty_lines() {
        let dict = Dictionary::from_bytes(b"a\n\nb\n").unwrap();
        assert_eq!(dict.len(), 3); // blank + a + b
        assert_eq!(dict.get(1), Some("a"));
        assert_eq!(dict.get(2), Some("b"));
    }
}
