//! Errors encountered while reading, validating, or parsing the `.agrm` binary format.

use thiserror::Error;

/// Specific errors that can occur during binary `.agrm` database validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum FormatError {
  /// The file or buffer does not start and end with the expected `.agrm` magic bytes.
  #[error("not an .agrm database (bad magic bytes)")]
  InvalidMagic,

  /// The database uses a format version this build does not support.
  #[error("unsupported .agrm format version: {0}")]
  UnsupportedVersion(u16),

  /// The database is truncated, misaligned, or its section offsets are inconsistent.
  #[error("malformed or truncated .agrm database")]
  Malformed,

  /// The computed CRC32 payload checksum does not match the stored checksum.
  #[error("checksum mismatch: .agrm database is corrupt")]
  ChecksumMismatch,

  /// A word exceeded the maximum allowed fixed-length buffer.
  #[error("word length {len} exceeds maximum buffer capacity of {max}")]
  WordTooLong {
    /// Number of characters encountered.
    len: usize,
    /// Maximum allowed buffer capacity.
    max: usize,
  },
}
