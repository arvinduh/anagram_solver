//! Engine-wide constants, default limits, and architectural bounds.

/// The maximum allowed word length (in bytes) across the anagram engine.
///
/// Set to 32 bytes (256 bits) to cleanly occupy a single AVX2/SIMD register
/// while accommodating virtually all natural language words in standard dictionaries.
pub const MAX_WORD_LEN: usize = 32;

/// Magic bytes at the start of an `.agrm` database file (`b"AGRM"`).
pub const HEADER_MAGIC: [u8; 4] = *b"AGRM";

/// Magic bytes at the end of an `.agrm` database file (`b"MRGA"`).
pub const FOOTER_MAGIC: [u8; 4] = *b"MRGA";
