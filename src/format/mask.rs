//! A 26-bit compact set of lowercase ASCII letters.
//!
//! Encoded within a little-endian `u32` to allow zero-copy embedding inside on-disk
//! trie nodes (see [`crate::format::layout::Node`]).
//!
//! Children in a trie node are arranged contiguously in alphabetical order. The mask
//! allows $O(1)$ calculation of a child's offset by counting the number of set bits
//! preceding that letter (popcount).

use zerocopy::{
  FromBytes, Immutable, IntoBytes, KnownLayout, LittleEndian, U32,
};

/// A bitset representing a subset of ASCII letters `a` through `z`.
///
/// Bit `i` (for `i` in `0..26`) corresponds to `b'a' + i`. Bits `26..32` are unused
/// and must remain zero.
#[derive(Clone, Copy, IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(transparent)]
pub struct LetterMask(U32<LittleEndian>);

impl LetterMask {
  /// The empty letter set containing no letters.
  pub const EMPTY: Self = Self(U32::from_bytes([0; 4]));

  /// Asserts that `letter` is a valid lowercase ASCII letter in debug builds.
  ///
  /// In release builds (`--release`), this is completely eliminated with zero runtime cost.
  #[inline(always)]
  fn assert_valid_letter(letter: u8) {
    debug_assert!(
      letter.is_ascii_lowercase(),
      "LetterMask only supports 'a'..='z', got '{}' (0x{:02x})",
      letter as char,
      letter
    );
  }

  /// Computes the single-bit mask corresponding to `letter`, or returns `None` if
  /// `letter` is not within `b'a'..=b'z'`.
  #[inline]
  pub const fn bit(letter: u8) -> Option<u32> {
    match letter {
      b'a'..=b'z' => Some(1 << (letter - b'a')),
      _ => None,
    }
  }

  /// Returns the raw 32-bit integer representation of the mask.
  #[inline]
  #[must_use]
  pub fn bits(self) -> u32 {
    self.0.get()
  }

  /// Returns the number of set bits (count of children present).
  #[inline]
  #[must_use]
  pub fn count_ones(self) -> u32 {
    self.bits().count_ones()
  }

  /// Inserts an ASCII letter into the set.
  ///
  /// Returns `true` if `letter` was valid and inserted, or `false` if `letter`
  /// was not a lowercase ASCII letter.
  #[inline]
  pub fn insert(&mut self, letter: u8) -> bool {
    Self::assert_valid_letter(letter);
    if let Some(bit) = Self::bit(letter) {
      self.0.set(self.bits() | bit);
      true
    } else {
      false
    }
  }

  /// Returns `true` if `letter` is present in the set. Always returns `false` for non-letters.
  #[inline]
  pub fn contains(self, letter: u8) -> bool {
    Self::assert_valid_letter(letter);
    Self::bit(letter).is_some_and(|bit| self.bits() & bit != 0)
  }

  /// Computes the sibling child offset for `letter`, or `None` if `letter` is not in the set.
  ///
  /// Because children are stored contiguously in ascending alphabetical order,
  /// the offset corresponds to the number of set bits below `letter`.
  #[inline]
  pub fn child_offset(self, letter: u8) -> Option<usize> {
    Self::assert_valid_letter(letter);
    let bit = Self::bit(letter)?;
    if self.bits() & bit == 0 {
      return None;
    }
    Some((self.bits() & (bit - 1)).count_ones() as usize)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn insert_and_contains() {
    let mut mask = LetterMask::EMPTY;
    assert!(mask.insert(b'a'));
    assert!(mask.insert(b'c'));
    assert!(mask.insert(b't'));

    assert!(mask.contains(b'a'));
    assert!(mask.contains(b'c'));
    assert!(mask.contains(b't'));
    assert!(!mask.contains(b'b'));
  }

  #[test]
  fn bit_fallback_for_non_letters() {
    assert_eq!(LetterMask::bit(b'1'), None);
    assert_eq!(LetterMask::bit(b'A'), None);
    assert_eq!(LetterMask::bit(0xFF), None);
  }

  #[test]
  #[cfg(debug_assertions)]
  #[should_panic(expected = "LetterMask only supports 'a'..='z'")]
  fn insert_panics_on_invalid_byte_in_debug() {
    let mut mask = LetterMask::EMPTY;
    mask.insert(b'1');
  }

  #[test]
  fn child_offset_counts_preceding_bits() {
    let mut mask = LetterMask::EMPTY;
    for &letter in b"act" {
      mask.insert(letter);
    }

    assert_eq!(mask.child_offset(b'a'), Some(0));
    assert_eq!(mask.child_offset(b'c'), Some(1));
    assert_eq!(mask.child_offset(b't'), Some(2));
    assert_eq!(mask.child_offset(b'b'), None); // Not in the set
  }
}
