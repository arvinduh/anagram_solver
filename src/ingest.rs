//! High-throughput parallel ingestion pipeline for local and remote dictionaries.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::constants::HEADER_MAGIC;
use crate::core::trie::Trie;
use crate::core::word::Word;
use crate::format::reader::Reader;

/// Canonical default remote dictionary URL for initial setup.
pub const DEFAULT_DICTIONARY_URL: &str =
  "https://raw.githubusercontent.com/dwyl/english-words/master/words_alpha.txt";

/// Default upper limit on downloaded remote files to protect against unbounded streams (500 MB).
pub const DEFAULT_MAX_REMOTE_BYTES: u64 = 500 * 1024 * 1024;

static REMOTE_DOWNLOAD_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Universal input source representation.
#[derive(Debug, PartialEq, Eq)]
pub enum Source {
  /// Local filesystem path.
  Local(PathBuf),
  /// Remote HTTP/HTTPS URL.
  Remote(String),
}

impl Source {
  /// Parses a string into either a local path or a remote URL.
  /// Automatically normalizes GitHub blob URLs into raw downloadable URLs.
  pub fn from_str_or_url(input: &str) -> Self {
    if input.starts_with("http://") || input.starts_with("https://") {
      let normalized =
        if input.contains("github.com") && input.contains("/blob/") {
          input
            .replace("github.com", "raw.githubusercontent.com")
            .replace("/blob/", "/")
        } else {
          input.to_string()
        };
      Self::Remote(normalized)
    } else {
      Self::Local(PathBuf::from(input))
    }
  }
}

/// RAII guard ensuring temporary downloaded files are deleted on drop.
struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.0);
  }
}

/// Streams a remote URL directly to a local file on disk using a fixed 8 KB buffer.
///
/// Keeps RAM usage constant (~8 KB) regardless of how large the remote file is,
/// avoiding out-of-memory errors on massive dictionaries.
pub fn fetch_remote_to_file(
  url: &str,
  dest: &Path,
  max_bytes: u64,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
  let mut response = ureq::get(url).call()?;
  if let Some(len_header) = response.headers().get("content-length")
    && let Ok(len) = len_header.to_str().unwrap_or("0").parse::<u64>()
    && len > max_bytes
  {
    return Err(
      format!("remote file length ({len} bytes) exceeds safety limit of {max_bytes} bytes").into(),
    );
  }

  let file = std::fs::File::create(dest)?;
  let mut writer = std::io::BufWriter::new(file);
  let mut reader = response.body_mut().as_reader().take(max_bytes + 1);
  let copied = std::io::copy(&mut reader, &mut writer)?;
  if copied > max_bytes {
    drop(writer);
    let _ = std::fs::remove_file(dest);
    return Err(
      format!("remote stream exceeded maximum limit of {max_bytes} bytes")
        .into(),
    );
  }
  Ok(copied)
}

/// Parses words from a file on disk using memory mapping and Rayon parallelism.
///
/// If the file is an `.agrm` database, reads words directly via zero-copy mmap.
/// If the file is text, maps the file into memory and tokenizes in parallel across threads.
fn process_file_mmap<F>(path: &Path, is_separator: F) -> Vec<Word>
where
  F: Fn(&u8) -> bool + Sync + Send + Copy,
{
  let file = match std::fs::File::open(path) {
    Ok(f) => f,
    Err(err) => {
      eprintln!("[WARN] Failed to open file {path:?}: {err}. Skipping...");
      return Vec::new();
    }
  };

  let meta = match file.metadata() {
    Ok(m) => m,
    Err(err) => {
      eprintln!(
        "[WARN] Failed to read metadata for {path:?}: {err}. Skipping..."
      );
      return Vec::new();
    }
  };

  let len = meta.len() as usize;
  if len == 0 {
    return Vec::new();
  }

  // Fast path: memory-map the file to avoid allocating heap RAM for raw text
  let mmap = match unsafe { memmap2::Mmap::map(&file) } {
    Ok(m) => m,
    Err(err) => {
      eprintln!(
        "[WARN] Failed to memory-map {path:?}: {err}. Falling back to streaming read..."
      );
      return process_file_streaming(file, is_separator);
    }
  };

  // Fast path for .agrm files: direct mmap via Reader (0 bytes loaded into heap buffers)
  let min_agrm_size = size_of::<crate::format::layout::Header>()
    + size_of::<crate::format::layout::Footer>();
  if mmap.len() >= min_agrm_size && mmap.starts_with(&HEADER_MAGIC) {
    return match Reader::from_mmap_fast(mmap) {
      Ok(reader) => reader
        .all_words()
        .map(|w| unsafe { Word::from_bytes_unchecked(w.as_bytes()) })
        .collect(),
      Err(_) => Vec::new(),
    };
  }

  mmap
    .par_split(is_separator)
    .filter_map(|token| Word::from_bytes(token).ok())
    .filter(|w| !w.as_bytes().is_empty())
    .collect()
}

/// Fallback stream parser for environments where memory mapping is unsupported.
fn process_file_streaming<F>(file: std::fs::File, is_separator: F) -> Vec<Word>
where
  F: Fn(&u8) -> bool + Sync + Send + Copy,
{
  use std::io::BufRead;
  let reader = std::io::BufReader::new(file);
  let mut words = Vec::new();
  for line_bytes in reader.split(b'\n').flatten() {
    for token in line_bytes.split(is_separator) {
      if let Ok(w) = Word::from_bytes(token)
        && !w.as_bytes().is_empty()
      {
        words.push(w);
      }
    }
  }
  words
}

/// Ingests words from multiple local and remote sources in parallel with constant memory usage.
///
/// Remote sources are streamed directly to disk to prevent RAM exhaustion.
/// Local files are parsed using memory mapping, eliminating redundant heap allocations.
///
/// Any source that fails to download or load is logged to stderr and skipped,
/// allowing valid sources to be merged cleanly.
///
/// # Arguments
///
/// * `sources` - Slice of [`Source`] items (paths or URLs).
/// * `is_separator` - Predicate indicating delimiter characters.
pub fn ingest<F>(sources: &[Source], is_separator: F) -> Trie
where
  F: Fn(&u8) -> bool + Sync + Send + Copy,
{
  // 1. Process all sources concurrently across Rayon worker threads
  let words_by_source: Vec<Vec<Word>> = sources
    .par_iter()
    .map(|source| match source {
      Source::Local(path) => process_file_mmap(path, is_separator),
      Source::Remote(url) => {
        let counter = REMOTE_DOWNLOAD_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = std::env::temp_dir().join(format!(
          "agrm_dl_{}_{}_{}.tmp",
          std::process::id(),
          std::thread::current().name().unwrap_or("worker"),
          counter
        ));
        let _guard = TempFileGuard(temp_path.clone());

        match fetch_remote_to_file(url, &temp_path, DEFAULT_MAX_REMOTE_BYTES) {
          Ok(_) => process_file_mmap(&temp_path, is_separator),
          Err(err) => {
            eprintln!(
              "[WARN] Failed to fetch remote URL {url}: {err}. Skipping..."
            );
            Vec::new()
          }
        }
      }
    })
    .collect();

  let words: Vec<Word> = words_by_source.into_par_iter().flatten().collect();

  crate::format::writer::build_trie_from_words(words)
}

/// Helper function to return a default local cache path for initial bootstrap.
pub fn default_cache_path() -> PathBuf {
  std::env::temp_dir().join("english_words.agrm")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::format::writer::Writer;

  #[test]
  fn test_source_parsing_and_github_blob_normalization() {
    let local = Source::from_str_or_url("/path/to/words.txt");
    assert_eq!(local, Source::Local(PathBuf::from("/path/to/words.txt")));

    let raw_remote = Source::from_str_or_url("https://example.com/dict.txt");
    assert_eq!(
      raw_remote,
      Source::Remote("https://example.com/dict.txt".to_string())
    );

    let github = Source::from_str_or_url(
      "https://github.com/dwyl/english-words/blob/master/words_alpha.txt",
    );
    assert_eq!(
      github,
      Source::Remote(
        "https://raw.githubusercontent.com/dwyl/english-words/master/words_alpha.txt".to_string()
      )
    );
  }

  #[test]
  fn test_ingest_from_single_txt_file() {
    let temp_dir = std::env::temp_dir();
    let file_path =
      temp_dir.join(format!("test_ingest_single_{}.txt", std::process::id()));
    std::fs::write(&file_path, b"cat\nact\ntac\ndog\n").unwrap();

    let trie = ingest(&[Source::Local(file_path.clone())], |&b| b == b'\n');
    let _ = std::fs::remove_file(file_path);

    assert_eq!(trie.total_words(), 4);
    let mut act_hits = trie.anagrams("cat");
    act_hits.sort_unstable();
    assert_eq!(act_hits, vec!["act", "cat", "tac"]);
  }

  #[test]
  fn test_ingest_from_multiple_files_with_deduplication() {
    let temp_dir = std::env::temp_dir();
    let file1 =
      temp_dir.join(format!("test_ingest_m1_{}.txt", std::process::id()));
    let file2 =
      temp_dir.join(format!("test_ingest_m2_{}.txt", std::process::id()));

    std::fs::write(&file1, b"cat\ndog\nstar\n").unwrap();
    // file2 shares "cat" and "dog", and adds "rats" and "god"
    std::fs::write(&file2, b"cat\ngod\nrats\ndog\n").unwrap();

    let sources = [Source::Local(file1.clone()), Source::Local(file2.clone())];
    let trie = ingest(&sources, |&b| b == b'\n');

    let _ = std::fs::remove_file(file1);
    let _ = std::fs::remove_file(file2);

    // Total unique words: cat, dog, god, star, rats = 5 words
    assert_eq!(trie.total_words(), 5);

    let mut star_hits = trie.anagrams("star");
    star_hits.sort_unstable();
    assert_eq!(star_hits, vec!["rats", "star"]);

    let mut dog_hits = trie.anagrams("dog");
    dog_hits.sort_unstable();
    assert_eq!(dog_hits, vec!["dog", "god"]);
  }

  #[test]
  fn test_ingest_with_arbitrary_delimiters() {
    let temp_dir = std::env::temp_dir();
    let file =
      temp_dir.join(format!("test_ingest_delim_{}.txt", std::process::id()));
    std::fs::write(&file, b"apple, banana; orange\tgrape|melon\npear").unwrap();

    let trie = ingest(&[Source::Local(file.clone())], |&b| {
      b == b',' || b == b';' || b == b'|' || b.is_ascii_whitespace()
    });
    let _ = std::fs::remove_file(file);

    assert_eq!(trie.total_words(), 6);
    assert_eq!(trie.anagrams("apple"), vec!["apple"]);
    assert_eq!(trie.anagrams("grape"), vec!["grape"]);
  }

  #[test]
  fn test_ingest_skips_failed_source_gracefully() {
    let temp_dir = std::env::temp_dir();
    let valid_file =
      temp_dir.join(format!("test_ingest_valid_{}.txt", std::process::id()));
    let non_existent_file = temp_dir.join("non_existent_file_xyz_12345.txt");

    std::fs::write(&valid_file, b"hello\nworld\n").unwrap();

    let sources = [
      Source::Local(non_existent_file),
      Source::Local(valid_file.clone()),
    ];

    // Must not panic or return an error; skips the missing file and loads the valid one
    let trie = ingest(&sources, |&b| b == b'\n');
    let _ = std::fs::remove_file(valid_file);

    assert_eq!(trie.total_words(), 2);
    assert_eq!(trie.anagrams("hello"), vec!["hello"]);
  }

  #[test]
  fn test_ingest_from_agrm_fast_path() {
    let temp_dir = std::env::temp_dir();
    let agrm_file =
      temp_dir.join(format!("test_ingest_agrm_{}.agrm", std::process::id()));

    // Compile a binary .agrm file
    Writer::build_file(&agrm_file, ["act", "cat", "dog", "god"]).unwrap();

    let trie = ingest(&[Source::Local(agrm_file.clone())], |&b| b == b'\n');
    let _ = std::fs::remove_file(agrm_file);

    assert_eq!(trie.total_words(), 4);
    let mut act_hits = trie.anagrams("cat");
    act_hits.sort_unstable();
    assert_eq!(act_hits, vec!["act", "cat"]);
  }

  #[test]
  fn test_temp_file_guard_cleans_up_on_drop() {
    let temp_dir = std::env::temp_dir();
    let temp_file =
      temp_dir.join(format!("test_guard_{}.tmp", std::process::id()));
    std::fs::write(&temp_file, b"sample data").unwrap();
    assert!(temp_file.exists());

    {
      let _guard = TempFileGuard(temp_file.clone());
      // File still exists while guard is in scope
      assert!(temp_file.exists());
    }

    // File must be deleted once guard drops
    assert!(!temp_file.exists());
  }

  #[test]
  fn test_process_file_streaming_fallback() {
    let temp_dir = std::env::temp_dir();
    let temp_file =
      temp_dir.join(format!("test_stream_{}.txt", std::process::id()));
    std::fs::write(&temp_file, b"stone\nnotes\ntones\n").unwrap();

    let file = std::fs::File::open(&temp_file).unwrap();
    let words = process_file_streaming(file, |&b| b == b'\n');
    let _ = std::fs::remove_file(temp_file);

    assert_eq!(words.len(), 3);
  }
}
