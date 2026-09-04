//! The `agrm` binary entry point.

use clap::Parser;

fn main() {
  let cli = anagram::commands::Cli::parse();
  if let Err(err) = anagram::commands::run(cli) {
    eprintln!("[ERROR] {err}");
    std::process::exit(1);
  }
}
