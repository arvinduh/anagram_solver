//! Arena-based, cache-friendly trie data structures for indexing anagrams.

use std::num::NonZeroU32;

use crate::constants::MAX_WORD_LEN;
use crate::core::word::Word;

/// An individual node within the arena-backed [`Trie`].
#[derive(Clone, Copy)]
pub struct TrieNode {
  /// 26 edges pointing to 1-based child node indices in [`Trie::nodes`] (`None` = no child).
  pub children: [Option<NonZeroU32>; 26],
  /// Byte offset where this node's words begin in [`Trie::word_pool`].
  pub word_start: u32,
  /// Number of anagram words stored in this node's bucket.
  pub word_count: u8,
  /// Depth from the root, corresponding to the character length of words stored at this node.
  pub depth: u8,
}

impl TrieNode {
  /// Creates a new empty node at the given depth.
  #[inline]
  pub const fn new(depth: u8) -> Self {
    Self {
      children: [None; 26],
      word_start: 0,
      word_count: 0,
      depth,
    }
  }

  /// Returns the character length of each word stored in this node's bucket.
  #[inline(always)]
  pub const fn word_len(&self) -> usize {
    self.depth as usize
  }

  /// Returns the number of anagram words stored in this node's bucket.
  #[inline(always)]
  pub const fn word_count(&self) -> usize {
    self.word_count as usize
  }
}

/// An arena-based, cache-friendly trie storing words grouped by sorted letter paths.
pub struct Trie {
  /// Contiguous array of all trie nodes, where index 0 is always the root.
  pub nodes: Vec<TrieNode>,
  /// Single shared byte arena for all words across all nodes.
  pub word_pool: Vec<u8>,
  /// Total count of words stored across all anagram buckets.
  total_words: usize,
}

impl Default for Trie {
  fn default() -> Self {
    Self::new()
  }
}

impl Trie {
  /// Creates a new empty `Trie` containing a single root node.
  pub fn new() -> Self {
    Self::with_capacity(128, 1024)
  }

  /// Creates a new empty `Trie` with pre-allocated node and word-pool capacities.
  pub fn with_capacity(node_cap: usize, pool_cap: usize) -> Self {
    let mut nodes = Vec::with_capacity(node_cap.max(1));
    nodes.push(TrieNode::new(0)); // Root node at index 0
    Self {
      nodes,
      word_pool: Vec::with_capacity(pool_cap),
      total_words: 0,
    }
  }

  /// Traverses or allocates nodes for the given sorted key, returning the leaf node index.
  #[inline]
  pub fn get_or_create_node(&mut self, key: &[u8]) -> usize {
    let mut current = 0;
    let mut depth = 0u8;

    for &b in key {
      depth += 1;
      let idx = (b - b'a') as usize;
      let next = match self.nodes[current].children[idx] {
        Some(child_idx) => child_idx.get() as usize,
        None => {
          let new_idx = self.nodes.len();
          self.nodes.push(TrieNode::new(depth));
          // Store 1-based NonZeroU32
          let non_zero = NonZeroU32::new(new_idx as u32)
            .expect("node index must not exceed u32 capacity");
          self.nodes[current].children[idx] = Some(non_zero);
          new_idx
        }
      };
      current = next;
    }

    current
  }

  /// Inserts a normalized word under its sorted key into the trie.
  /// Returns `true` if newly inserted, or `false` if already present.
  pub fn insert(&mut self, key: &[u8], word: &[u8]) -> bool {
    let node_idx = self.get_or_create_node(key);
    self.add_word_to_node(node_idx, word)
  }

  /// Inserts a [`Word`] into the trie.
  pub fn insert_word(&mut self, word: &Word) -> bool {
    let key = word.key();
    self.insert(key.as_bytes(), word.as_bytes())
  }

  /// Adds a word to the given node's bucket, handling in-place growth or relocation.
  fn add_word_to_node(&mut self, node_idx: usize, word: &[u8]) -> bool {
    let depth = self.nodes[node_idx].word_len();
    debug_assert_eq!(depth, word.len());

    let start = self.nodes[node_idx].word_start as usize;
    let count = self.nodes[node_idx].word_count as usize;
    let total_bytes = count * depth;

    // 1. Check if word is already present in this node's bucket (deduplication)
    if count > 0 {
      let existing = &self.word_pool[start..start + total_bytes];
      for chunk in existing.chunks_exact(depth) {
        if chunk == word {
          return false;
        }
      }
    }

    // 2. Insert or relocate word in word_pool
    if count > 0 && start + total_bytes == self.word_pool.len() {
      // Fast path: Bucket is already at the end of word_pool -> append in-place
      self.word_pool.extend_from_slice(word);
      self.nodes[node_idx].word_count += 1;
    } else if count == 0 {
      // First word for this node: append to the end of word_pool
      let new_start = self.word_pool.len() as u32;
      self.word_pool.extend_from_slice(word);
      self.nodes[node_idx].word_start = new_start;
      self.nodes[node_idx].word_count = 1;
    } else {
      // Middle of heap insertion: Relocate bucket to the end of the pool
      let new_start = self.word_pool.len() as u32;
      self
        .word_pool
        .extend_from_within(start..start + total_bytes);
      self.word_pool.extend_from_slice(word);
      self.nodes[node_idx].word_start = new_start;
      self.nodes[node_idx].word_count += 1;
    }

    self.total_words += 1;
    true
  }

  /// Returns an iterator yielding string slices for all anagram words at `node_idx`.
  pub fn words_at(&self, node_idx: usize) -> impl Iterator<Item = &str> {
    let depth = self.nodes[node_idx].word_len();
    let start = self.nodes[node_idx].word_start as usize;
    let count = self.nodes[node_idx].word_count as usize;
    let total = count * depth;

    let slice =
      if depth > 0 && count > 0 && start + total <= self.word_pool.len() {
        &self.word_pool[start..start + total]
      } else {
        &[]
      };

    let chunk_size = depth.max(1);
    slice.chunks_exact(chunk_size).map(|chunk| {
      // SAFETY: Ingested words are guaranteed valid lowercase ASCII.
      unsafe { std::str::from_utf8_unchecked(chunk) }
    })
  }

  /// Finds the node index corresponding to the given sorted key, if present.
  #[inline]
  #[must_use]
  pub fn find_node(&self, key: &[u8]) -> Option<usize> {
    let mut current = 0;
    for &b in key {
      if !b.is_ascii_lowercase() {
        return None;
      }
      let idx = (b - b'a') as usize;
      current = self.nodes[current].children[idx]?.get() as usize;
    }
    Some(current)
  }

  /// Returns all anagrams for a given query word.
  #[must_use]
  pub fn anagrams(&self, query: &str) -> Vec<&str> {
    let key = Word::<MAX_WORD_LEN>::from_str(query).ok().map(|w| w.key());

    match key.and_then(|k| self.find_node(k.as_bytes())) {
      Some(idx) => self.words_at(idx).collect(),
      None => Vec::new(),
    }
  }

  /// Returns total number of words stored in the trie across all buckets.
  #[inline]
  #[must_use]
  pub fn total_words(&self) -> usize {
    self.total_words
  }

  /// Returns total number of nodes in the arena.
  #[inline]
  #[must_use]
  pub fn node_count(&self) -> usize {
    self.nodes.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_empty_trie() {
    let trie = Trie::new();
    assert_eq!(trie.node_count(), 1); // Root node
    assert_eq!(trie.total_words(), 0);
    assert!(trie.anagrams("cat").is_empty());
  }

  #[test]
  fn test_single_word_insert_and_lookup() {
    let mut trie = Trie::new();
    let word = Word::<32>::from_str("cat").unwrap();
    assert!(trie.insert_word(&word));

    let results = trie.anagrams("cat");
    assert_eq!(results, vec!["cat"]);

    // Querying with an anagram query returns the word
    assert_eq!(trie.anagrams("act"), vec!["cat"]);
    assert_eq!(trie.anagrams("tac"), vec!["cat"]);
  }

  #[test]
  fn test_multiple_anagrams_same_bucket() {
    let mut trie = Trie::new();
    trie.insert_word(&Word::<32>::from_str("cat").unwrap());
    trie.insert_word(&Word::<32>::from_str("act").unwrap());
    trie.insert_word(&Word::<32>::from_str("tac").unwrap());

    assert_eq!(trie.total_words(), 3);

    let mut hits = trie.anagrams("cat");
    hits.sort_unstable();
    assert_eq!(hits, vec!["act", "cat", "tac"]);
  }

  #[test]
  fn test_duplicate_insertion_returns_false() {
    let mut trie = Trie::new();
    let w = Word::<32>::from_str("hello").unwrap();
    assert!(trie.insert_word(&w));
    // Second insertion should be rejected as duplicate
    assert!(!trie.insert_word(&w));
    assert_eq!(trie.total_words(), 1);
    assert_eq!(trie.anagrams("hello"), vec!["hello"]);
  }

  #[test]
  fn test_distinct_words_and_shared_prefixes() {
    let mut trie = Trie::new();
    trie.insert_word(&Word::<32>::from_str("car").unwrap());
    trie.insert_word(&Word::<32>::from_str("cart").unwrap());
    trie.insert_word(&Word::<32>::from_str("care").unwrap());
    trie.insert_word(&Word::<32>::from_str("race").unwrap());
    trie.insert_word(&Word::<32>::from_str("dog").unwrap());
    trie.insert_word(&Word::<32>::from_str("god").unwrap());

    assert_eq!(trie.total_words(), 6);

    let mut care_hits = trie.anagrams("care");
    care_hits.sort_unstable();
    assert_eq!(care_hits, vec!["care", "race"]);

    let mut dog_hits = trie.anagrams("dog");
    dog_hits.sort_unstable();
    assert_eq!(dog_hits, vec!["dog", "god"]);

    assert_eq!(trie.anagrams("car"), vec!["car"]);
    assert_eq!(trie.anagrams("cart"), vec!["cart"]);
  }

  #[test]
  fn test_middle_of_heap_insertions() {
    let mut trie = Trie::new();

    // 1. First bucket "act" -> ["cat"]
    trie.insert_word(&Word::<32>::from_str("cat").unwrap());

    // 2. Second bucket "dgo" -> ["dog"] (placed after "cat" in word_pool)
    trie.insert_word(&Word::<32>::from_str("dog").unwrap());

    // 3. Middle-of-heap insertion into bucket "act" -> ["cat", "act"]
    // Must relocate "act" bucket to the end of the pool without corrupting "dog"
    trie.insert_word(&Word::<32>::from_str("act").unwrap());

    // 4. Another bucket "aeehlnpt" -> ["elephant"]
    trie.insert_word(&Word::<32>::from_str("elephant").unwrap());

    // 5. Another middle-of-heap insertion into bucket "act" -> ["cat", "act", "tac"]
    trie.insert_word(&Word::<32>::from_str("tac").unwrap());

    // 6. Middle-of-heap insertion into bucket "dgo" -> ["dog", "god"]
    trie.insert_word(&Word::<32>::from_str("god").unwrap());

    // Verify all buckets remain 100% correct and isolated
    let mut act_hits = trie.anagrams("cat");
    act_hits.sort_unstable();
    assert_eq!(act_hits, vec!["act", "cat", "tac"]);

    let mut dog_hits = trie.anagrams("dog");
    dog_hits.sort_unstable();
    assert_eq!(dog_hits, vec!["dog", "god"]);

    assert_eq!(trie.anagrams("elephant"), vec!["elephant"]);
  }

  #[test]
  fn test_large_scale_anagram_clusters() {
    let mut trie = Trie::with_capacity(500, 4096);

    // Large anagram family: alerts, alters, artels, estral, laster, ratels, salter, slater, staler, stelar, talers
    let cluster = [
      "alerts", "alters", "artels", "estral", "laster", "ratels", "salter",
      "slater", "staler", "stelar", "talers",
    ];

    for w in cluster {
      trie.insert_word(&Word::<32>::from_str(w).unwrap());
    }

    assert_eq!(trie.total_words(), cluster.len());

    let mut found = trie.anagrams("staler");
    found.sort_unstable();
    let mut expected = cluster.to_vec();
    expected.sort_unstable();
    assert_eq!(found, expected);
  }

  #[test]
  fn test_query_non_existent_and_case_insensitivity() {
    let mut trie = Trie::new();
    trie.insert_word(&Word::<32>::from_str("apple").unwrap());

    assert!(trie.anagrams("banana").is_empty());
    assert!(trie.anagrams("xyz").is_empty());

    // Case normalization during query
    assert_eq!(trie.anagrams("APPLE"), vec!["apple"]);
    assert_eq!(trie.anagrams("ApPlE!"), vec!["apple"]);
  }
}
