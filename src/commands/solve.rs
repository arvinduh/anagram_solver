//! Implementation of `agrm solve`.

use clap::Args;
use std::path::PathBuf;

use crate::commands::init::{InitArgs, run as run_init};
use crate::commands::resolve_db_path;
use crate::format::reader::Reader;

/// Arguments for the `agrm solve` command.
#[derive(Args)]
pub struct SolveArgs {
  /// Word or letters to solve (e.g. "listen", "cat").
  #[arg(value_name = "QUERY")]
  pub query: String,

  /// Path to the .agrm database (defaults to $TMP/english_words.agrm).
  #[arg(short, long, value_name = "PATH")]
  pub db: Option<PathBuf>,

  /// Find all sub-anagrams (words formed by subsets of letters) instead of only exact matches.
  #[arg(short, long)]
  pub sub: bool,

  /// Minimum word length when searching for sub-anagrams (defaults to 3).
  #[arg(short = 'm', long, default_value_t = 3)]
  pub min_len: usize,

  /// Limit the maximum number of results printed.
  #[arg(short = 'n', long, value_name = "COUNT")]
  pub limit: Option<usize>,

  /// Output results as a JSON array.
  #[arg(long)]
  pub json: bool,

  /// Fast open (skips full CRC32 payload verification for microsecond startup).
  #[arg(long, default_value_t = true)]
  pub fast: bool,
}

/// Executes the `solve` command.
pub fn run(
  args: SolveArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let db_path = resolve_db_path(args.db);

  // Auto-init if database does not exist
  if !db_path.exists() {
    eprintln!(
      "[INFO] Database not found at {:?}. Auto-initializing with default dictionary...",
      db_path
    );
    let init_args = InitArgs {
      sources: Vec::new(),
      output: Some(db_path.clone()),
      force: false,
      quiet: false,
    };
    run_init(init_args)?;
  }

  // Open database via mmap (fast or full)
  let reader = if args.fast {
    Reader::open_fast(&db_path)?
  } else {
    Reader::open(&db_path)?
  };

  let mut hits: Vec<&str> = if args.sub {
    let mut words = reader.sub_anagrams(&args.query, args.min_len);
    // Sort by word length descending, then alphabetical
    words.sort_unstable_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    words
  } else {
    let mut words: Vec<_> = reader.anagrams(&args.query).collect();
    words.sort_unstable();
    words
  };

  if let Some(limit) = args.limit {
    hits.truncate(limit);
  }

  use std::io::Write;
  let stdout = std::io::stdout();
  let mut out = std::io::BufWriter::new(stdout.lock());

  if args.json {
    write_json(&mut out, &hits)?;
    writeln!(out)?;
  } else {
    for word in &hits {
      writeln!(out, "{word}")?;
    }
  }
  out.flush()?;

  Ok(())
}

fn write_json<W: std::io::Write>(
  writer: &mut W,
  words: &[&str],
) -> std::io::Result<()> {
  writer.write_all(b"[")?;
  for (i, word) in words.iter().enumerate() {
    if i > 0 {
      writer.write_all(b",")?;
    }
    writer.write_all(b"\"")?;
    writer.write_all(word.as_bytes())?;
    writer.write_all(b"\"")?;
  }
  writer.write_all(b"]")
}
