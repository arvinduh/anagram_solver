//! Reads, validates, and queries `.agrm` v1 database files.

use std::fs::File;
use std::ops::Deref;
use std::path::Path;

use zerocopy::FromBytes;

use crate::constants::{FOOTER_MAGIC, HEADER_MAGIC, MAX_WORD_LEN};
use crate::core::word::Word;
use crate::error::Result;
use crate::format::error::FormatError;
use crate::format::layout::{Footer, Header, Node, VERSION};

/// Backing storage for a [`Reader`], either a memory-mapped file or an owned byte buffer.
enum Storage {
  /// Memory-mapped file via [`memmap2::Mmap`].
  Mmap(memmap2::Mmap),
  /// Owned heap-allocated byte buffer.
  Owned(Vec<u8>),
}

impl Deref for Storage {
  type Target = [u8];

  #[inline(always)]
  fn deref(&self) -> &[u8] {
    match self {
      Self::Mmap(m) => m,
      Self::Owned(b) => b,
    }
  }
}

/// A validated `.agrm` v1 database reader.
///
/// Backed either by a memory-mapped file ([`memmap2::Mmap`]) for instant $O(1)$ zero-copy
/// queries or an in-memory byte buffer. Backing byte buffers are retained in memory for
/// the lifetime of this struct, and queries return zero-copy string slices referencing
/// the word pool without any deserialization or intermediate heap allocations.
pub struct Reader {
  storage: Storage,
  header: Header,
}

impl Reader {
  /// Memory-maps and validates an `.agrm` database file from disk.
  ///
  /// Uses [`memmap2::Mmap`] to establish a zero-copy mapping into the process's
  /// virtual address space, validating format integrity and CRC32 payload checksum.
  ///
  /// # Arguments
  ///
  /// * `path` - Path to the `.agrm` file on disk.
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::Io`] if the file cannot be opened or mapped, or
  /// [`crate::Error::Format`] if the file is invalid or corrupt.
  pub fn open(path: impl AsRef<Path>) -> Result<Self> {
    let file = File::open(path)?;
    let meta = file.metadata()?;
    let min_size = size_of::<Header>() + size_of::<Footer>();
    if (meta.len() as usize) < min_size {
      return Err(FormatError::Malformed.into());
    }

    // SAFETY: We map the file as read-only. We assume the underlying file is not
    // concurrently truncated or modified while mapped.
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    let header = validate(&mmap)?;
    Ok(Self {
      storage: Storage::Mmap(mmap),
      header,
    })
  }

  /// Memory-maps an `.agrm` database file with fast $O(1)$ structural validation.
  ///
  /// Validates magic identifiers, format version, and section boundaries without
  /// scanning the entire payload to compute CRC32. This provides sub-microsecond
  /// startup time on large dictionaries, touching only the first and last pages.
  ///
  /// You can verify the payload integrity at any time via [`Reader::validate_checksum`].
  pub fn open_fast(path: impl AsRef<Path>) -> Result<Self> {
    let file = File::open(path)?;
    let meta = file.metadata()?;
    let min_size = size_of::<Header>() + size_of::<Footer>();
    if (meta.len() as usize) < min_size {
      return Err(FormatError::Malformed.into());
    }

    // SAFETY: We map the file as read-only. We assume the underlying file is not
    // concurrently truncated or modified while mapped.
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    Self::from_mmap_fast(mmap)
  }

  /// Constructs a [`Reader`] from an existing memory mapping with full CRC32 validation.
  pub fn from_mmap(mmap: memmap2::Mmap) -> Result<Self> {
    let header = validate(&mmap)?;
    Ok(Self {
      storage: Storage::Mmap(mmap),
      header,
    })
  }

  /// Constructs a [`Reader`] from an existing memory mapping with fast $O(1)$ structural validation.
  pub fn from_mmap_fast(mmap: memmap2::Mmap) -> Result<Self> {
    let header = validate_structure(&mmap)?;
    Ok(Self {
      storage: Storage::Mmap(mmap),
      header,
    })
  }

  /// Validates an in-memory byte buffer containing an `.agrm` database image.
  ///
  /// Performs full structural and CRC32 checksum verification.
  ///
  /// # Arguments
  ///
  /// * `bytes` - The complete database image as owned bytes.
  ///
  /// # Errors
  ///
  /// Returns [`crate::Error::Format`] with one of the [`FormatError`] variants
  /// if the buffer fails validation.
  pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self> {
    let bytes = bytes.into();
    let header = validate(&bytes)?;
    Ok(Self {
      storage: Storage::Owned(bytes),
      header,
    })
  }

  /// Validates an in-memory byte buffer with $O(1)$ structural validation, skipping CRC32.
  pub fn from_bytes_fast(bytes: impl Into<Vec<u8>>) -> Result<Self> {
    let bytes = bytes.into();
    let header = validate_structure(&bytes)?;
    Ok(Self {
      storage: Storage::Owned(bytes),
      header,
    })
  }

  /// Verifies the CRC32 payload checksum against the stored header and footer checksums.
  pub fn validate_checksum(&self) -> Result<()> {
    validate_checksum_for(&self.storage, &self.header)?;
    Ok(())
  }

  /// Advises the OS kernel about expected page access patterns for memory-mapped databases.
  ///
  /// Has no effect if the database is backed by an in-memory owned buffer.
  #[cfg(unix)]
  pub fn advise(&self, advice: memmap2::Advice) -> std::io::Result<()> {
    if let Storage::Mmap(ref mmap) = self.storage {
      mmap.advise(advice)?;
    }
    Ok(())
  }

  /// Returns a reference to the parsed file [`Header`].
  #[inline]
  pub fn header(&self) -> &Header {
    &self.header
  }

  /// Returns the entire trie node array as a slice.
  #[inline]
  pub fn nodes(&self) -> &[Node] {
    let start = self.header.nodes_offset.get() as usize;
    let count = self.header.node_count.get() as usize;
    <[Node]>::ref_from_prefix_with_elems(&self.storage[start..], count)
      .expect("validated during load")
      .0
  }

  /// Returns a byte slice over the contiguous word pool.
  #[inline]
  pub fn word_pool(&self) -> &[u8] {
    let start = self.header.word_pool_offset.get() as usize;
    let end = self.storage.len() - size_of::<Footer>();
    &self.storage[start..end]
  }

  /// Traverses the flattened trie to locate the [`Node`] matching a sorted letter key.
  ///
  /// # Arguments
  ///
  /// * `key` - An ASCII byte slice containing lowercase letters sorted in ascending order.
  pub fn node_for_key(&self, key: &[u8]) -> Option<&Node> {
    let nodes = self.nodes();
    let mut node = nodes.first()?;
    for &letter in key {
      node = nodes.get(node.child_index(letter)?)?;
    }
    Some(node)
  }

  /// Returns the contiguous raw byte slice for the words stored at `node`.
  #[inline]
  pub fn word_bucket<'a>(&'a self, node: &Node) -> Option<&'a [u8]> {
    node.word_bucket(self.word_pool())
  }

  /// Returns an iterator yielding all word string slices stored at `node`.
  pub fn words<'a>(
    &'a self,
    node: &Node,
  ) -> impl Iterator<Item = &'a str> + 'a {
    let len = node.word_len();
    let bucket = if len > 0 {
      node.word_bucket(self.word_pool()).unwrap_or(&[])
    } else {
      &[]
    };

    bucket.chunks_exact(len).map(|chunk| {
      // SAFETY: Words stored in .agrm are guaranteed valid lowercase ASCII.
      unsafe { std::str::from_utf8_unchecked(chunk) }
    })
  }

  /// Returns an iterator yielding all words stored in the database that are exact
  /// anagrams of `letters`.
  ///
  /// Non-ASCII-alphabetic characters are ignored and all letters are normalized to lowercase.
  ///
  /// # Arguments
  ///
  /// * `letters` - The input string whose anagrams will be searched.
  pub fn anagrams<'a>(
    &'a self,
    letters: &str,
  ) -> impl Iterator<Item = &'a str> + 'a {
    let node = Word::<MAX_WORD_LEN>::from_str(letters)
      .ok()
      .and_then(|w| self.node_for_key(w.key().as_bytes()));

    node.into_iter().flat_map(|n| self.words(n))
  }

  /// Returns an iterator yielding all words stored in the database.
  pub fn all_words(&self) -> impl Iterator<Item = &str> {
    self
      .nodes()
      .iter()
      .filter(|node| node.word_count() > 0)
      .flat_map(|node| self.words(node))
  }

  /// Returns all words in the database that can be formed using any subset of `letters`
  /// with a length of at least `min_len`.
  ///
  /// Traverses the sorted-letter trie via depth-first search, using bitmask child pruning
  /// to eliminate non-existent branches in $O(1)$ CPU cycles.
  /// Traverses the sorted-letter trie via depth-first search, invoking `callback`
  /// on each discovered word as soon as it is matched.
  ///
  /// Prunes non-existent branches using bitmask operations in $O(1)$ CPU cycles.
  pub fn sub_anagrams_streaming<'a, F>(
    &'a self,
    letters: &str,
    min_len: usize,
    mut callback: F,
  ) where
    F: FnMut(&'a str),
  {
    let mut counts = [0u8; 26];
    let mut total = 0usize;
    for &b in letters.as_bytes() {
      let lower = b | 0x20;
      if lower.is_ascii_lowercase() {
        counts[(lower - b'a') as usize] += 1;
        total += 1;
      }
    }

    if total < min_len {
      return;
    }

    let nodes = self.nodes();
    let root = match nodes.first() {
      Some(r) => r,
      None => return,
    };

    self.dfs_sub_anagrams_streaming(
      nodes, root, 0, &mut counts, min_len, &mut callback,
    );
  }

  fn dfs_sub_anagrams_streaming<'a, F>(
    &'a self,
    nodes: &'a [Node],
    node: &Node,
    start_letter_idx: usize,
    counts: &mut [u8; 26],
    min_len: usize,
    callback: &mut F,
  ) where
    F: FnMut(&'a str),
  {
    if node.word_count() > 0 && (node.depth as usize) >= min_len {
      for word in self.words(node) {
        callback(word);
      }
    }

    let first = node.first_child.get();
    if first == Node::NO_CHILD {
      return;
    }

    let num_children = node.children_mask.count_ones() as usize;
    let first_idx = first as usize;
    if let Some(children) = nodes.get(first_idx..first_idx + num_children) {
      for child in children {
        if child.letter < b'a' + (start_letter_idx as u8) {
          continue;
        }
        let letter_idx = (child.letter - b'a') as usize;
        if counts[letter_idx] > 0 {
          counts[letter_idx] -= 1;
          self.dfs_sub_anagrams_streaming(
            nodes, child, letter_idx, counts, min_len, callback,
          );
          counts[letter_idx] += 1;
        }
      }
    }
  }

  /// Returns all words in the database that can be formed using any subset of `letters`
  /// with a length of at least `min_len`.
  pub fn sub_anagrams<'a>(
    &'a self,
    letters: &str,
    min_len: usize,
  ) -> Vec<&'a str> {
    let mut results = Vec::new();
    self.sub_anagrams_streaming(letters, min_len, |w| results.push(w));
    results
  }

  /// Returns all sub-anagrams grouped by word length descending, sorted alphabetically within each length.
  pub fn sub_anagrams_grouped<'a>(
    &'a self,
    letters: &str,
    min_len: usize,
  ) -> Vec<(usize, Vec<&'a str>)> {
    let mut buckets: [Vec<&'a str>; MAX_WORD_LEN + 1] =
      std::array::from_fn(|_| Vec::new());
    self.sub_anagrams_streaming(letters, min_len, |word| {
      if word.len() <= MAX_WORD_LEN {
        buckets[word.len()].push(word);
      }
    });

    let mut grouped = Vec::new();
    for len in (min_len..=MAX_WORD_LEN).rev() {
      if !buckets[len].is_empty() {
        buckets[len].sort_unstable();
        grouped.push((len, std::mem::take(&mut buckets[len])));
      }
    }
    grouped
  }
}

/// Verifies header magic, footer magic, offset bounds, and node alignment.
fn validate_structure(
  bytes: &[u8],
) -> std::result::Result<Header, FormatError> {
  let (header, _) =
    Header::ref_from_prefix(bytes).map_err(|_| FormatError::Malformed)?;

  if header.magic != HEADER_MAGIC {
    return Err(FormatError::InvalidMagic);
  }
  if header.version.get() != VERSION {
    return Err(FormatError::UnsupportedVersion(header.version.get()));
  }
  let header = *header;

  let (before_footer, footer) =
    Footer::ref_from_suffix(bytes).map_err(|_| FormatError::Malformed)?;
  if footer.magic != FOOTER_MAGIC {
    return Err(FormatError::InvalidMagic);
  }
  let footer_start = before_footer.len();

  let nodes_offset = header.nodes_offset.get() as usize;
  let pool_offset = header.word_pool_offset.get() as usize;
  let node_count = header.node_count.get() as usize;

  if nodes_offset != size_of::<Header>() || pool_offset > footer_start {
    return Err(FormatError::Malformed);
  }

  let node_region = bytes
    .get(nodes_offset..pool_offset)
    .ok_or(FormatError::Malformed)?;
  let (_, rest) = <[Node]>::ref_from_prefix_with_elems(node_region, node_count)
    .map_err(|_| FormatError::Malformed)?;
  if !rest.is_empty() {
    return Err(FormatError::Malformed);
  }

  Ok(header)
}

/// Verifies CRC32 payload checksum against the stored header and footer checksums.
fn validate_checksum_for(
  bytes: &[u8],
  header: &Header,
) -> std::result::Result<(), FormatError> {
  let nodes_offset = header.nodes_offset.get() as usize;
  let footer_start = bytes.len() - size_of::<Footer>();

  let crc = crc32fast::hash(&bytes[nodes_offset..footer_start]);
  if crc != header.checksum.get() {
    return Err(FormatError::ChecksumMismatch);
  }

  let (_, footer) =
    Footer::ref_from_suffix(bytes).map_err(|_| FormatError::Malformed)?;
  if crc != footer.checksum.get() {
    return Err(FormatError::ChecksumMismatch);
  }

  Ok(())
}

/// Full validation: structural checks followed by CRC32 checksum verification.
fn validate(bytes: &[u8]) -> std::result::Result<Header, FormatError> {
  let header = validate_structure(bytes)?;
  validate_checksum_for(bytes, &header)?;
  Ok(header)
}

#[cfg(test)]
mod tests {
  use super::*;

  use std::io::Cursor;

  use super::super::writer::Writer;
  use crate::error::Error;

  fn sample() -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    Writer::new(&mut buf)
      .write_words(["act", "cat", "dog"])
      .unwrap();
    buf.into_inner()
  }

  #[test]
  fn rejects_bad_magic() {
    let mut bytes = sample();
    bytes[0] = b'X';
    assert!(matches!(
      Reader::from_bytes(bytes),
      Err(Error::Format(FormatError::InvalidMagic))
    ));
  }

  #[test]
  fn rejects_bad_version() {
    let mut bytes = sample();
    bytes[4] = 99;
    assert!(matches!(
      Reader::from_bytes(bytes),
      Err(Error::Format(FormatError::UnsupportedVersion(_)))
        | Err(Error::Format(FormatError::ChecksumMismatch))
    ));
  }

  #[test]
  fn rejects_corrupt_payload() {
    let mut bytes = sample();
    let last = bytes.len() - size_of::<Footer>() - 1;
    bytes[last] ^= 0xFF;
    assert!(matches!(
      Reader::from_bytes(bytes),
      Err(Error::Format(FormatError::ChecksumMismatch))
    ));
  }

  #[test]
  fn rejects_truncation() {
    let mut bytes = sample();
    bytes.truncate(bytes.len() - 4);
    assert!(matches!(
      Reader::from_bytes(bytes),
      Err(Error::Format(FormatError::Malformed))
        | Err(Error::Format(FormatError::InvalidMagic))
    ));
  }

  #[test]
  fn mmap_reader_open_and_open_fast() {
    let temp_dir = std::env::temp_dir();
    let file_path =
      temp_dir.join(format!("test_mmap_reader_{}.agrm", std::process::id()));

    Writer::build_file(&file_path, ["silent", "listen", "enlist"]).unwrap();

    // Standard open (with checksum verification)
    let reader = Reader::open(&file_path).unwrap();
    assert_eq!(reader.header().total_words.get(), 3);
    let mut hits = reader.anagrams("listen").collect::<Vec<_>>();
    hits.sort_unstable();
    assert_eq!(hits, ["enlist", "listen", "silent"]);

    // Fast open (skips checksum scan)
    let reader_fast = Reader::open_fast(&file_path).unwrap();
    assert_eq!(reader_fast.header().total_words.get(), 3);
    assert!(reader_fast.validate_checksum().is_ok());

    // Advising kernel
    #[cfg(unix)]
    assert!(reader.advise(memmap2::Advice::Random).is_ok());

    let _ = std::fs::remove_file(file_path);
  }

  #[test]
  fn open_rejects_empty_file() {
    let temp_dir = std::env::temp_dir();
    let file_path =
      temp_dir.join(format!("test_empty_{}.agrm", std::process::id()));
    std::fs::write(&file_path, b"").unwrap();

    assert!(matches!(
      Reader::open(&file_path),
      Err(Error::Format(FormatError::Malformed))
    ));

    let _ = std::fs::remove_file(file_path);
  }

  #[test]
  fn test_sub_anagrams_finds_subsets() {
    let mut buf = Cursor::new(Vec::new());
    // "listen", "silent", "enlist", "in", "it", "sit", "line", "tie", "cat"
    Writer::new(&mut buf)
      .write_words([
        "listen", "silent", "enlist", "in", "it", "sit", "line", "tie", "cat",
      ])
      .unwrap();

    let reader = Reader::from_bytes(buf.into_inner()).unwrap();

    // Min len 2: matches all subsets from "listen", but excludes "cat"
    let mut subs = reader.sub_anagrams("listen", 2);
    subs.sort_unstable();
    assert_eq!(
      subs,
      vec![
        "enlist", "in", "it", "line", "listen", "silent", "sit", "tie"
      ]
    );

    // Min len 4: should exclude 2-letter and 3-letter words
    let mut subs4 = reader.sub_anagrams("listen", 4);
    subs4.sort_unstable();
    assert_eq!(subs4, vec!["enlist", "line", "listen", "silent"]);

    // Test streaming
    let mut streamed = Vec::new();
    reader.sub_anagrams_streaming("listen", 4, |w| streamed.push(w));
    streamed.sort_unstable();
    assert_eq!(streamed, vec!["enlist", "line", "listen", "silent"]);

    // Test grouped
    let grouped = reader.sub_anagrams_grouped("listen", 4);
    assert_eq!(grouped.len(), 2); // Length 6 and Length 4
    assert_eq!(grouped[0].0, 6);
    assert_eq!(grouped[0].1, vec!["enlist", "listen", "silent"]);
    assert_eq!(grouped[1].0, 4);
    assert_eq!(grouped[1].1, vec!["line"]);
  }
}
