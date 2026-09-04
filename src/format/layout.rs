//! Wire-layout types for the `.agrm` v1 format.
//!
//! Every type here is `#[repr(C)]`, little-endian, and guaranteed to have no padding
//! so it can safely be converted to and from byte buffers with zero copying.

use zerocopy::{
  FromBytes, Immutable, IntoBytes, KnownLayout, LittleEndian, U16, U32,
};

use super::mask::LetterMask;

/// Binary database format version defined by this module.
pub const VERSION: u16 = 1;

/// Fixed 32-byte header located at offset `0` of an `.agrm` file.
#[derive(Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Header {
  /// Header magic identifier; must match [`crate::constants::HEADER_MAGIC`].
  pub magic: [u8; 4],
  /// Format version; must match [`VERSION`].
  pub version: U16<LittleEndian>,
  /// Maximum word length supported by this database file.
  pub max_word_len: U16<LittleEndian>,
  /// Total count of trie nodes in the node array.
  pub node_count: U32<LittleEndian>,
  /// Total count of words stored across all anagram buckets.
  pub total_words: U32<LittleEndian>,
  /// Byte offset where the node array begins (always `32`).
  pub nodes_offset: U32<LittleEndian>,
  /// Byte offset where the word pool begins.
  pub word_pool_offset: U32<LittleEndian>,
  /// Reserved field for future extensions; must be zero in v1.
  pub reserved: U32<LittleEndian>,
  /// CRC32 checksum over the payload (node array and word pool).
  pub checksum: U32<LittleEndian>,
}

/// A 16-byte flattened trie node.
///
/// Sibling nodes are stored contiguously starting at index [`Node::first_child`],
/// ordered alphabetically by letter. [`Node::children_mask`] indicates which letters
/// are present and calculates the exact index offset in $O(1)$ time.
#[derive(Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Node {
  /// The letter by which this node was reached (`0` for the trie root).
  pub letter: u8,
  /// Reserved flags; must be zero in v1.
  pub flags: u8,
  /// Depth from the root, corresponding to the character length of words stored here.
  pub depth: u8,
  /// Number of anagram words stored in this node's word pool bucket.
  pub word_count: u8,
  /// Index of the first child node in the node array, or [`Node::NO_CHILD`] if a leaf.
  pub first_child: U32<LittleEndian>,
  /// Byte offset of this node's word bucket relative to the start of the word pool.
  pub word_bucket_offset: U32<LittleEndian>,
  /// Bitmask recording present child letters.
  pub children_mask: LetterMask,
}

impl Node {
  /// Sentinel value for [`Node::first_child`] indicating the node has no children.
  pub const NO_CHILD: u32 = u32::MAX;

  /// Returns the array index of the child node corresponding to `letter`,
  /// or `None` if no such child exists.
  #[inline]
  pub fn child_index(&self, letter: u8) -> Option<usize> {
    let first = self.first_child.get();
    if first == Self::NO_CHILD {
      return None;
    }
    let offset = self.children_mask.child_offset(letter)?;
    Some(first as usize + offset)
  }

  /// Returns the character length (in bytes) of each word stored in this node's bucket.
  #[inline]
  pub fn word_len(&self) -> usize {
    self.depth as usize
  }

  /// Returns the number of anagram words stored in this node's bucket.
  #[inline]
  pub fn word_count(&self) -> usize {
    self.word_count as usize
  }

  /// Returns the start byte offset of this node's bucket relative to the start of the word pool.
  #[inline]
  pub fn word_bucket_offset(&self) -> usize {
    self.word_bucket_offset.get() as usize
  }

  /// Returns the byte range spanned by this node's bucket within the word pool.
  #[inline]
  pub fn word_bucket_range(&self) -> std::ops::Range<usize> {
    let start = self.word_bucket_offset();
    let total = self.word_count() * self.word_len();
    start..start + total
  }

  /// Returns the contiguous raw byte slice for this node's bucket from `word_pool`.
  #[inline]
  pub fn word_bucket<'p>(&self, word_pool: &'p [u8]) -> Option<&'p [u8]> {
    word_pool.get(self.word_bucket_range())
  }
}

/// Fixed 8-byte footer located at the final 8 bytes (`EOF - 8`) of the file.
#[derive(Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Footer {
  /// Duplicate of [`Header::checksum`] for validation from the end of the file.
  pub checksum: U32<LittleEndian>,
  /// Footer magic identifier; must match [`crate::constants::FOOTER_MAGIC`].
  pub magic: [u8; 4],
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn layout_sizes_are_fixed() {
    assert_eq!(size_of::<Header>(), 32);
    assert_eq!(size_of::<Node>(), 16);
    assert_eq!(size_of::<Footer>(), 8);
    assert_eq!(size_of::<LetterMask>(), 4);
  }

  #[test]
  fn node_descriptor_and_bucket_slicing() {
    let node = Node {
      letter: b't',
      flags: 0,
      depth: 3,
      word_count: 2,
      first_child: Node::NO_CHILD.into(),
      word_bucket_offset: 0.into(),
      children_mask: LetterMask::EMPTY,
    };

    assert_eq!(node.word_len(), 3);
    assert_eq!(node.word_count(), 2);
    assert_eq!(node.word_bucket_offset(), 0);
    assert_eq!(node.word_bucket_range(), 0..6);

    let raw = b"actcat";
    assert_eq!(node.word_bucket(raw), Some(&b"actcat"[..]));
  }
}
