//! Implementation of `agrm init`.

use clap::Args;
use std::path::PathBuf;
use std::time::Instant;

use crate::commands::resolve_db_path;
use crate::format::writer::Writer;
use crate::ingest::{DEFAULT_DICTIONARY_URL, Source, ingest};

/// Arguments for the `agrm init` command.
#[derive(Args)]
pub struct InitArgs {
  /// Input wordlist files (paths or URLs). Defaults to standard English dictionary if omitted.
  #[arg(value_name = "SOURCES")]
  pub sources: Vec<String>,

  /// Destination database file path (defaults to $TMP/english_words.agrm).
  #[arg(short, long, value_name = "PATH")]
  pub output: Option<PathBuf>,

  /// Overwrite existing database file without confirmation.
  #[arg(short, long)]
  pub force: bool,

  /// Silence progress and summary output.
  #[arg(short, long)]
  pub quiet: bool,
}

/// Executes the `init` command.
pub fn run(
  args: InitArgs,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let output_path = resolve_db_path(args.output);

  if output_path.exists() && !args.force {
    if !args.quiet {
      eprintln!(
        "[INFO] Database already exists at {:?}. Use --force to overwrite.",
        output_path
      );
    }
    return Ok(());
  }

  let source_strings = if args.sources.is_empty() {
    vec![DEFAULT_DICTIONARY_URL.to_string()]
  } else {
    args.sources
  };

  let sources: Vec<Source> = source_strings
    .iter()
    .map(|s| Source::from_str_or_url(s.as_str()))
    .collect();

  if !args.quiet {
    eprintln!(
      "[INFO] Initializing database from {} source(s)...",
      sources.len()
    );
    for source in &sources {
      match source {
        Source::Local(p) => eprintln!("  - Local:  {}", p.display()),
        Source::Remote(u) => eprintln!("  - Remote: {u}"),
      }
    }
  }

  let start = Instant::now();

  // Ingest sources in parallel across Rayon workers
  let trie = ingest(&sources, |&b| b == b'\n' || b == b'\r');

  if !args.quiet {
    eprintln!(
      "[INFO] Ingested {} unique words in {:.2?}",
      trie.total_words(),
      start.elapsed()
    );
  }

  // Atomically build and serialize .agrm database
  let write_start = Instant::now();
  Writer::build_from_trie(&output_path, &trie)?;

  if !args.quiet {
    let file_size = std::fs::metadata(&output_path)?.len();
    eprintln!(
      "[INFO] Saved database to {} ({:.2} MB) in {:.2?}",
      output_path.display(),
      file_size as f64 / (1024.0 * 1024.0),
      write_start.elapsed()
    );
  }

  Ok(())
}
