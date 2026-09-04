//! Crate-level error types, result aliases, and classification helpers.

use thiserror::Error;

use crate::format::error::FormatError;

/// A specialized [`Result`](std::result::Result) type for anagram database operations.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Umbrella error type for all failures that can occur when interacting with this crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
  /// An underlying standard I/O operation failed.
  #[error("i/o error: {0}")]
  Io(#[from] std::io::Error),

  /// A format or validation error occurred while decoding the `.agrm` binary layout.
  #[error("format error: {0}")]
  Format(#[from] FormatError),
}
