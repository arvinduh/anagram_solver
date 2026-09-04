//! Minimal fixed-size container for normalized words and sorted trie keys.

use std::fmt;
use std::mem::MaybeUninit;

use crate::constants;
use crate::format::error::FormatError;

/// Branchless ASCII normalization logic.
///
/// Converts ASCII uppercase to lowercase and filters out any non-alphabetic
/// characters or high bytes (>= 128) using register-only bitwise operations.
#[inline]
fn normalize_ascii(byte: u8) -> Option<u8> {
  let lower = byte | 0x20;
  if lower.wrapping_sub(b'a') < 26 {
    Some(lower)
  } else {
    None
  }
}

/// Inlined insertion sort for any `Copy + Ord` type.
///
/// For small slices (<= 16 elements?), this monomorphizes and inlines
/// into optimal machine code with zero function-call overhead.
#[inline]
fn insertion_sort<T: Copy + Ord>(slice: &mut [T]) {
  let len = slice.len();
  for i in 1..len {
    let x = slice[i];
    let mut j = i;
    while j > 0 && slice[j - 1] > x {
      slice[j] = slice[j - 1];
      j -= 1;
    }
    slice[j] = x;
  }
}

/// Fixed-capacity stack buffer for normalized words and anagram keys.
///
/// # Safety & Trait Invariants
///
/// **WARNING:** Do **NOT** implement or derive [`PartialEq`], [`Eq`], or [`std::hash::Hash`]
/// without operating strictly on the initialized active slice ([`as_bytes`](Self::as_bytes)).
///
/// The tail buffer (`word[len..N]`) contains **uninitialized memory** to eliminate the
/// compute overhead of zero-padding on the hot parsing loop. Deriving equality or hashing
/// directly on `word` will read uninitialized memory and result in undefined behavior.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Word<const N: usize = { constants::MAX_WORD_LEN }> {
  /// Fixed-size buffer for normalized ASCII bytes. Unused slots beyond `len` are uninitialized.
  word: [MaybeUninit<u8>; N],
  /// Number of valid initialized bytes in `word`.
  len: u8,
}

impl<const N: usize> Word<N> {
  /// Ingests from a raw byte slice (memory-mapped chunk, network buffer, or file bytes).
  ///
  /// Bypasses all pre-zeroing and tail-zeroing compute overhead. Unused slots beyond `len`
  /// remain uninitialized.
  #[inline]
  pub fn from_bytes(bytes: &[u8]) -> Result<Self, FormatError> {
    let mut word: [MaybeUninit<u8>; N] = [MaybeUninit::uninit(); N];
    let mut count = 0usize;

    for &b in bytes {
      if let Some(norm) = normalize_ascii(b) {
        if count >= N {
          return Err(FormatError::WordTooLong {
            len: count + 1,
            max: N,
          });
        }
        word[count].write(norm);
        count += 1;
      }
    }

    Ok(Self {
      word,
      len: count as u8,
    })
  }

  /// Constructs a `Word` directly from pre-validated lowercase ASCII bytes without normalization.
  ///
  /// # Safety
  ///
  /// The caller must guarantee that:
  /// 1. `bytes.len() <= N` (does not exceed buffer capacity).
  /// 2. Every byte in `bytes` is a valid ASCII lowercase letter (`b'a'..=b'z'`).
  #[inline]
  pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> Self {
    debug_assert!(bytes.len() <= N, "word length exceeds capacity");
    debug_assert!(
      bytes.iter().all(u8::is_ascii_lowercase),
      "bytes must be lowercase ASCII"
    );

    let mut word: [MaybeUninit<u8>; N] = [MaybeUninit::uninit(); N];
    unsafe {
      std::ptr::copy_nonoverlapping(
        bytes.as_ptr(),
        word.as_mut_ptr().cast::<u8>(),
        bytes.len(),
      );
    }

    Self {
      word,
      len: bytes.len() as u8,
    }
  }

  /// Ingests from a string slice (queries, CLI args, etc.).
  #[allow(clippy::should_implement_trait)]
  #[inline]
  pub fn from_str(raw: &str) -> Result<Self, FormatError> {
    Self::from_bytes(raw.as_bytes())
  }

  /// Exposes the active normalized slice up to `len`.
  #[inline]
  #[must_use]
  pub fn as_bytes(&self) -> &[u8] {
    // SAFETY: Indices 0..self.len are guaranteed to be initialized with
    // valid lowercase ASCII bytes during ingestion.
    unsafe {
      std::slice::from_raw_parts(
        self.word.as_ptr().cast::<u8>(),
        self.len as usize,
      )
    }
  }

  /// Returns a new `Word` containing the letters sorted alphabetically via insertion sort.
  #[inline]
  #[must_use]
  pub fn key(&self) -> Self {
    let mut res = *self;
    // SAFETY: Indices 0..res.len are guaranteed to be initialized.
    let slice = unsafe {
      std::slice::from_raw_parts_mut(
        res.word.as_mut_ptr().cast::<u8>(),
        res.len as usize,
      )
    };
    if self.len <= 16 {
      insertion_sort(slice);
    } else {
      slice.sort_unstable();
    }
    res
  }
}

impl<const N: usize> std::str::FromStr for Word<N> {
  type Err = FormatError;

  #[inline]
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::from_bytes(s.as_bytes())
  }
}

impl<const N: usize> fmt::Debug for Word<N> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Word")
      .field("word", &String::from_utf8_lossy(self.as_bytes()))
      .field("len", &self.len)
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_exhaustive_256_byte_domain() {
    for byte in 0u8..=255 {
      let result = normalize_ascii(byte);
      match byte {
        b'a'..=b'z' => assert_eq!(result, Some(byte)),
        b'A'..=b'Z' => assert_eq!(result, Some(byte + 32)),
        _ => assert_eq!(result, None, "Byte 0x{byte:02X} was not discarded"),
      }
    }
  }

  #[test]
  fn test_normalization_and_lowercase() {
    let w = Word::<32>::from_str("HELLO").unwrap();
    assert_eq!(w.as_bytes(), b"hello");
    assert_eq!(w.as_bytes().len(), 5);
    assert!(!w.as_bytes().is_empty());

    let mixed = Word::<32>::from_str("aNaGrAm").unwrap();
    assert_eq!(mixed.as_bytes(), b"anagram");
  }

  #[test]
  fn test_ascii_punctuation_digits_and_whitespace() {
    let s = "\0\t\r\n 123!@#$%^&*()-_=+[{]}\\|;:'\",<.>/?`~ c-a_t! \r\n";
    let w = Word::<32>::from_str(s).unwrap();
    assert_eq!(w.as_bytes(), b"cat");

    let w2 = Word::<32>::from_str("twenty-one").unwrap();
    assert_eq!(w2.as_bytes(), b"twentyone");

    let w3 = Word::<32>::from_str("don't").unwrap();
    assert_eq!(w3.as_bytes(), b"dont");

    let w4 = Word::<32>::from_str("O'Connor").unwrap();
    assert_eq!(w4.as_bytes(), b"oconnor");

    let empty = Word::<32>::from_str("  12345 _!@#  ").unwrap();
    assert_eq!(empty.as_bytes(), b"");
    assert!(empty.as_bytes().is_empty());
  }

  #[test]
  fn test_utf8_multibyte_accents_and_foreign_alphabets() {
    let cafe = Word::<32>::from_str("café").unwrap();
    assert_eq!(cafe.as_bytes(), b"caf");

    let naive = Word::<32>::from_str("naïve").unwrap();
    assert_eq!(naive.as_bytes(), b"nave");

    let facade = Word::<32>::from_str("façade").unwrap();
    assert_eq!(facade.as_bytes(), b"faade");

    let strasse = Word::<32>::from_str("straße").unwrap();
    assert_eq!(strasse.as_bytes(), b"strae");

    let anio = Word::<32>::from_str("año").unwrap();
    assert_eq!(anio.as_bytes(), b"ao");

    let greek = Word::<32>::from_str("αβγδε").unwrap();
    assert!(greek.as_bytes().is_empty());

    let cyrillic = Word::<32>::from_str("Москва").unwrap();
    assert!(cyrillic.as_bytes().is_empty());

    let cjk = Word::<32>::from_str("東京Tokyo").unwrap();
    assert_eq!(cjk.as_bytes(), b"tokyo");
  }

  #[test]
  fn test_emojis_and_unicode_symbols() {
    let w = Word::<32>::from_str("crab🦀cat🐱rocket🚀").unwrap();
    assert_eq!(w.as_bytes(), b"crabcatrocket");

    let symbols =
      Word::<32>::from_str("price: $100 €50 £20 ¥3000 ±5% → OK").unwrap();
    assert_eq!(symbols.as_bytes(), b"priceok");

    let pure_emoji = Word::<32>::from_str("🦀🐱🚀🔥🎉").unwrap();
    assert!(pure_emoji.as_bytes().is_empty());
  }

  #[test]
  fn test_special_characters_do_not_consume_buffer_capacity() {
    let padded = "-".repeat(100)
      + "c"
      + &"-".repeat(50)
      + "a"
      + &"🐱".repeat(20)
      + "t"
      + &"+".repeat(50);
    let w = Word::<4>::from_str(&padded).unwrap();
    assert_eq!(w.as_bytes(), b"cat");
    assert_eq!(w.as_bytes().len(), 3);
  }

  #[test]
  fn test_length_overflow() {
    let small = Word::<4>::from_str("cats");
    assert!(small.is_ok());
    assert_eq!(small.unwrap().as_bytes(), b"cats");

    let overflow = Word::<4>::from_str("apple");
    assert_eq!(
      overflow.unwrap_err(),
      FormatError::WordTooLong { len: 5, max: 4 }
    );

    let exact_32 = "a".repeat(32) + "---123---🦀";
    assert!(Word::<32>::from_str(&exact_32).is_ok());

    let overflow_33 = "a".repeat(33) + "---123---🦀";
    assert_eq!(
      Word::<32>::from_str(&overflow_33).unwrap_err(),
      FormatError::WordTooLong { len: 33, max: 32 }
    );
  }

  #[test]
  fn test_key_sorting() {
    let cat = Word::<32>::from_str("cat").unwrap();
    assert_eq!(cat.key().as_bytes(), b"act");

    let banana = Word::<32>::from_str("banana").unwrap();
    assert_eq!(banana.key().as_bytes(), b"aaabnn");
    assert_eq!(banana.as_bytes(), b"banana");

    let mixed = Word::<32>::from_str("B-a-N-a-N-a!").unwrap();
    assert_eq!(mixed.key().as_bytes(), b"aaabnn");
  }

  #[test]
  fn test_debug_output() {
    let w = Word::<32>::from_str("cat").unwrap();
    let debug_str = format!("{w:?}");
    assert!(debug_str.contains("cat"));
    assert!(debug_str.contains("len: 3"));
  }

  #[test]
  fn test_from_bytes_unchecked() {
    let raw = b"banana";
    let w = unsafe { Word::<32>::from_bytes_unchecked(raw) };
    assert_eq!(w.as_bytes(), b"banana");
    assert_eq!(w.key().as_bytes(), b"aaabnn");
  }
}
