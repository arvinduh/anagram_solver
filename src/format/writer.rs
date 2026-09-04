//! Compiles and serializes an arena-based [`Trie`] into the binary `.agrm` v1 format.

use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;

use rayon::prelude::*;
use zerocopy::IntoBytes;

use crate::constants::{FOOTER_MAGIC, HEADER_MAGIC, MAX_WORD_LEN};
use crate::core::trie::Trie;
use crate::core::word::Word;
use crate::error::Result;
use crate::format::layout::{Footer, Header, Node, VERSION};
use crate::format::mask::LetterMask;

/// Compiles and serializes word lists or pre-built [`Trie`]s into the binary `.agrm` format.
pub struct Writer<W = ()> {
  writer: W,
}

impl<W: Write> Writer<W> {
  /// Wraps an output stream (such as a [`std::fs::File`] or [`std::io::Cursor`]).
  pub fn new(writer: W) -> Self {
    Self { writer }
  }

  /// Writes a compiled [`Trie`] sequentially to the underlying stream.
  pub fn write_trie(&mut self, trie: &Trie) -> Result<()> {
    let (nodes, word_pool) = flatten_core_trie(trie);
    let total_words = trie.total_words() as u32;

    let nodes_offset = size_of::<Header>() as u32;
    let word_pool_offset =
      nodes_offset + (nodes.len() * size_of::<Node>()) as u32;

    let mut hasher = crc32fast::Hasher::new();
    hasher.update(nodes.as_bytes());
    hasher.update(&word_pool);
    let checksum = hasher.finalize();

    let header = Header {
      magic: HEADER_MAGIC,
      version: VERSION.into(),
      max_word_len: (MAX_WORD_LEN as u16).into(),
      node_count: (nodes.len() as u32).into(),
      total_words: total_words.into(),
      nodes_offset: nodes_offset.into(),
      word_pool_offset: word_pool_offset.into(),
      reserved: 0.into(),
      checksum: checksum.into(),
    };

    let footer = Footer {
      checksum: checksum.into(),
      magic: FOOTER_MAGIC,
    };

    self.writer.write_all(header.as_bytes())?;
    self.writer.write_all(nodes.as_bytes())?;
    self.writer.write_all(&word_pool)?;
    self.writer.write_all(footer.as_bytes())?;
    self.writer.flush()?;
    Ok(())
  }

  /// Ingests words from an iterator, compiles them into a [`Trie`], and writes to the stream.
  pub fn write_words<I, S>(&mut self, words: I) -> Result<()>
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    self.write_trie(&trie_from_iter(words))
  }

  /// Ingests words in parallel via Rayon, compiles them into a [`Trie`], and writes to the stream.
  pub fn write_words_par<I, S>(&mut self, words: I) -> Result<()>
  where
    I: IntoParallelIterator<Item = S>,
    S: AsRef<str> + Send,
  {
    self.write_trie(&trie_from_par_iter(words))
  }
}

impl Writer<()> {
  /// Compiles an iterator of words into an in-memory binary `.agrm` image.
  pub fn compile_words<I, S>(words: I) -> Vec<u8>
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    Self::encode_trie(&trie_from_iter(words))
  }

  /// Compiles words in parallel using [`rayon`] into an in-memory binary `.agrm` image.
  pub fn compile_words_par<I, S>(words: I) -> Vec<u8>
  where
    I: IntoParallelIterator<Item = S>,
    S: AsRef<str> + Send,
  {
    Self::encode_trie(&trie_from_par_iter(words))
  }

  /// Builds a complete `.agrm` database from an iterator of words atomically at `path`.
  pub fn build_file<P, I, S>(path: P, words: I) -> Result<()>
  where
    P: AsRef<Path>,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    Self::build_from_trie(path, &trie_from_iter(words))
  }

  /// Builds a complete `.agrm` database from a parallel iterator of words atomically at `path`.
  pub fn build_file_par<P, I, S>(path: P, words: I) -> Result<()>
  where
    P: AsRef<Path>,
    I: IntoParallelIterator<Item = S>,
    S: AsRef<str> + Send,
  {
    Self::build_from_trie(path, &trie_from_par_iter(words))
  }

  /// Builds an `.agrm` database directly from a pre-constructed [`Trie`] atomically at `path`.
  pub fn build_from_trie<P: AsRef<Path>>(path: P, trie: &Trie) -> Result<()> {
    let path = path.as_ref();
    let tmp = path.with_extension(format!("agrm.tmp.{}", std::process::id()));

    struct TempCleanup<'a>(&'a Path, bool);
    impl Drop for TempCleanup<'_> {
      fn drop(&mut self) {
        if !self.1 {
          let _ = std::fs::remove_file(self.0);
        }
      }
    }

    let mut guard = TempCleanup(&tmp, false);
    {
      let file = std::fs::File::create(&tmp)?;
      let mut writer = Writer::new(std::io::BufWriter::new(file));
      writer.write_trie(trie)?;
    }
    std::fs::rename(&tmp, path)?;
    guard.1 = true;
    Ok(())
  }

  /// Encodes a pre-constructed [`Trie`] directly into a binary `.agrm` byte vector.
  pub fn encode_trie(trie: &Trie) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = Writer::new(&mut buf).write_trie(trie);
    buf
  }
}

pub(crate) fn trie_from_iter<I, S>(words: I) -> Trie
where
  I: IntoIterator<Item = S>,
  S: AsRef<str>,
{
  let mut trie = Trie::new();
  for entry in words {
    if let Ok(word) = Word::<MAX_WORD_LEN>::from_str(entry.as_ref())
      && !word.as_bytes().is_empty()
    {
      trie.insert_word(&word);
    }
  }
  trie
}

pub(crate) fn trie_from_par_iter<I, S>(words: I) -> Trie
where
  I: IntoParallelIterator<Item = S>,
  S: AsRef<str> + Send,
{
  let entries: Vec<Word> = words
    .into_par_iter()
    .filter_map(|entry| Word::from_str(entry.as_ref()).ok())
    .filter(|w| !w.as_bytes().is_empty())
    .collect();

  build_trie_from_words(entries)
}

pub(crate) fn build_trie_from_words(mut words: Vec<Word>) -> Trie {
  words.par_sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
  words.dedup_by(|a, b| a.as_bytes() == b.as_bytes());

  // Precompute keys ONCE per unique word: eliminates ~15 million insertion sorts during sort!
  let mut entries: Vec<(Word, Word)> =
    words.into_par_iter().map(|w| (w.key(), w)).collect();
  entries.par_sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

  let mut trie = Trie::with_capacity(entries.len(), entries.len() * 6);
  for (key, word) in entries {
    trie.insert(key.as_bytes(), word.as_bytes());
  }
  trie
}

/// Flattens an arena [`Trie`] into the on-disk level-order representation.
///
/// Sibling nodes are arranged contiguously in alphabetical order to allow
/// $O(1)$ child indexing via bitmask popcount on disk.
fn flatten_core_trie(trie: &Trie) -> (Vec<Node>, Vec<u8>) {
  let mut on_disk_nodes: Vec<Node> = Vec::with_capacity(trie.nodes.len());
  let mut compact_word_pool: Vec<u8> = Vec::with_capacity(trie.word_pool.len());

  // Root node is always at index 0
  on_disk_nodes.push(Node {
    letter: 0,
    flags: 0,
    depth: 0,
    word_count: 0,
    first_child: Node::NO_CHILD.into(),
    word_bucket_offset: 0.into(),
    children_mask: LetterMask::EMPTY,
  });

  let mut queue = VecDeque::with_capacity(trie.nodes.len().min(4096));
  queue.push_back((0usize, 0usize)); // (arena_idx, on_disk_idx)

  while let Some((arena_idx, on_disk_idx)) = queue.pop_front() {
    let parent = &trie.nodes[arena_idx];

    let mut active_children = [(0u8, 0usize); 26];
    let mut child_count = 0;
    let mut mask = LetterMask::EMPTY;

    // Scan all 26 edges in alphabetical order
    for (i, child_opt) in parent.children.iter().enumerate() {
      if let Some(child_arena_idx) = child_opt {
        let letter = b'a' + (i as u8);
        mask.insert(letter);
        active_children[child_count] = (letter, child_arena_idx.get() as usize);
        child_count += 1;
      }
    }

    if child_count == 0 {
      continue;
    }

    let first_child_idx = on_disk_nodes.len() as u32;
    on_disk_nodes[on_disk_idx].first_child = first_child_idx.into();
    on_disk_nodes[on_disk_idx].children_mask = mask;

    // Allocate all siblings contiguously in alphabetical order
    for &(letter, child_arena_idx) in &active_children[..child_count] {
      let child_node = &trie.nodes[child_arena_idx];
      let depth = child_node.depth;
      let count = child_node.word_count;
      let word_bucket_offset = compact_word_pool.len() as u32;

      if count > 0 {
        let start = child_node.word_start as usize;
        let total = (count as usize) * (depth as usize);
        compact_word_pool
          .extend_from_slice(&trie.word_pool[start..start + total]);
      }

      let child_on_disk_idx = on_disk_nodes.len();
      on_disk_nodes.push(Node {
        letter,
        flags: 0,
        depth,
        word_count: count,
        first_child: Node::NO_CHILD.into(),
        word_bucket_offset: word_bucket_offset.into(),
        children_mask: LetterMask::EMPTY,
      });

      queue.push_back((child_arena_idx, child_on_disk_idx));
    }
  }

  (on_disk_nodes, compact_word_pool)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::format::reader::Reader;
  use std::io::Cursor;

  #[test]
  fn round_trips_through_a_database() {
    let mut buf = Cursor::new(Vec::new());
    Writer::new(&mut buf)
      .write_words(["act", "cat", "at", "dog", "god", "!@#", ""])
      .unwrap();

    let db = Reader::from_bytes(buf.into_inner()).unwrap();
    assert_eq!(db.header().total_words.get(), 5); // "!@#" and "" are empty and skipped

    let mut act = db.anagrams("cat").collect::<Vec<_>>();
    act.sort_unstable();
    assert_eq!(act, ["act", "cat"]);

    let mut dog = db.anagrams("God").collect::<Vec<_>>();
    dog.sort_unstable();
    assert_eq!(dog, ["dog", "god"]);

    assert!(db.anagrams("xyz").next().is_none());
  }

  #[test]
  fn round_trips_parallel_database() {
    let words = vec!["act", "cat", "at", "dog", "god", "!@#", ""];
    let bytes = Writer::compile_words_par(words);

    let db = Reader::from_bytes(bytes).unwrap();
    assert_eq!(db.header().total_words.get(), 5);

    let mut act = db.anagrams("cat").collect::<Vec<_>>();
    act.sort_unstable();
    assert_eq!(act, ["act", "cat"]);
  }

  #[test]
  fn build_file_atomic_roundtrip() {
    let dir = std::env::temp_dir();
    let file_path =
      dir.join(format!("test_agrm_atomic_{}.agrm", std::process::id()));

    Writer::build_file(&file_path, ["star", "rats", "tars"]).unwrap();
    let db = Reader::open(&file_path).unwrap();
    assert_eq!(db.header().total_words.get(), 3);

    let mut hits = db.anagrams("star").collect::<Vec<_>>();
    hits.sort_unstable();
    assert_eq!(hits, ["rats", "star", "tars"]);

    let _ = std::fs::remove_file(file_path);
  }

  #[test]
  fn empty_input_produces_a_valid_database() {
    let bytes = Writer::compile_words(Vec::<String>::new());
    let db = Reader::from_bytes(bytes).unwrap();
    assert_eq!(db.header().total_words.get(), 0);
    assert_eq!(db.header().node_count.get(), 1);
  }

  #[test]
  fn write_trie_matches_encode_trie() {
    let mut trie = Trie::new();
    trie.insert_word(&Word::from_str("stone").unwrap());
    trie.insert_word(&Word::from_str("notes").unwrap());
    trie.insert_word(&Word::from_str("tones").unwrap());

    let encoded = Writer::encode_trie(&trie);

    let mut streamed = Vec::new();
    Writer::new(&mut streamed).write_trie(&trie).unwrap();

    assert_eq!(encoded, streamed);

    let reader = Reader::from_bytes(streamed).unwrap();
    assert_eq!(reader.header().total_words.get(), 3);
    let mut hits = reader.anagrams("onset").collect::<Vec<_>>();
    hits.sort_unstable();
    assert_eq!(hits, ["notes", "stone", "tones"]);
  }

  #[test]
  fn build_from_trie_roundtrip() {
    let dir = std::env::temp_dir();
    let file_path =
      dir.join(format!("test_build_from_trie_{}.agrm", std::process::id()));

    let mut trie = Trie::new();
    trie.insert_word(&Word::from_str("rust").unwrap());
    trie.insert_word(&Word::from_str("tsur").unwrap());

    Writer::build_from_trie(&file_path, &trie).unwrap();

    let reader = Reader::open(&file_path).unwrap();
    assert_eq!(reader.header().total_words.get(), 2);
    let mut hits = reader.anagrams("ruts").collect::<Vec<_>>();
    hits.sort_unstable();
    assert_eq!(hits, ["rust", "tsur"]);

    let _ = std::fs::remove_file(file_path);
  }
}
