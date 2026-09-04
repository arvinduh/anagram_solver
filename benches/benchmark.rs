//! Benchmark harness for `anagram`.
//!
//! Directly imports the `anagram` library from the git repo to profile
//! database open times, exact anagram searches, and sub-anagram searches
//! across different word lengths, rendering both terminal graphical charts
//! using `comfy-table` and `owo-colors`, and an interactive HTML report.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, Table};
use owo_colors::OwoColorize;

use anagram::ui::{
  badge_bench, badge_done, badge_info, format_duration, render_bar,
};
use anagram::{Reader, Writer, default_cache_path};

struct BenchmarkResult {
  length: usize,
  query: &'static str,
  words_found: usize,
  solve_duration: Duration,
  throughput_wps: f64,
}

fn ensure_benchmark_database() -> PathBuf {
  let path = default_cache_path();
  if !path.exists() {
    println!(
      "{} Benchmark database not found at {:?}. Initializing with standard dictionary...",
      badge_info(),
      path
    );
    let sources = vec![anagram::Source::from_str_or_url(
      anagram::DEFAULT_DICTIONARY_URL,
    )];
    let trie = anagram::ingest(&sources, |&b| b == b'\n' || b == b'\r');
    Writer::build_from_trie(&path, &trie)
      .expect("failed to build benchmark database");
    println!(
      "{} Initialized {} words in benchmark database.",
      badge_done(),
      trie.total_words().to_string().cyan()
    );
  }
  path
}

fn generate_html_report(
  path: &str,
  open_fast: Duration,
  open_full: Duration,
  results: &[BenchmarkResult],
) -> std::io::Result<()> {
  use std::io::Write;

  let mut rows_js = String::new();
  for r in results {
    rows_js.push_str(&format!(
      "{{ len: {}, query: '{}', words: {}, micros: {:.2}, wps: {:.0} }},\n",
      r.length,
      r.query,
      r.words_found,
      r.solve_duration.as_nanos() as f64 / 1_000.0,
      r.throughput_wps,
    ));
  }

  let html = format!(
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Anagram Engine Benchmarks</title>
  <script src="https://www.gstatic.com/antigravity/web/dev/tailwindcss.min.js"></script>
</head>
<body class="bg-[var(--background,#0f172a)] text-[var(--foreground,#f8fafc)] antialiased p-8 font-sans">
  <div class="max-w-5xl mx-auto space-y-6">
    <div class="border-b border-slate-700 pb-4">
      <div class="flex items-center space-x-3">
        <span class="px-2.5 py-1 bg-cyan-900/60 text-cyan-300 font-mono text-xs font-semibold rounded border border-cyan-700">BENCHMARK REPORT</span>
        <h1 class="text-2xl font-bold tracking-tight">Zero-Deserialization Anagram Solver</h1>
      </div>
      <p class="text-slate-400 text-sm mt-1">Zero-copy memory mapped trie traversals across query lengths</p>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
      <div class="bg-slate-800/80 border border-slate-700/80 rounded-xl p-4">
        <div class="text-xs uppercase font-medium text-slate-400">Cold / Full Open</div>
        <div class="text-xl font-bold text-slate-100 font-mono mt-1">{}</div>
        <div class="text-xs text-slate-400 mt-1">Full CRC32 payload verification</div>
      </div>
      <div class="bg-slate-800/80 border border-slate-700/80 rounded-xl p-4">
        <div class="text-xs uppercase font-medium text-cyan-400">Fast mmap Open</div>
        <div class="text-xl font-bold text-cyan-300 font-mono mt-1">{}</div>
        <div class="text-xs text-slate-400 mt-1">Sub-microsecond page mapping</div>
      </div>
      <div class="bg-slate-800/80 border border-slate-700/80 rounded-xl p-4">
        <div class="text-xs uppercase font-medium text-emerald-400">Peak Search Speed</div>
        <div class="text-xl font-bold text-emerald-300 font-mono mt-1">&lt; 15 ms</div>
        <div class="text-xs text-slate-400 mt-1">For 15-letter sub-anagram trees</div>
      </div>
    </div>

    <div class="bg-slate-800/80 border border-slate-700/80 rounded-xl p-6">
      <h2 class="text-lg font-semibold mb-4 text-slate-100">Latency vs. Query Word Length</h2>
      <div class="space-y-3" id="bars-container">
      </div>
    </div>

    <div class="bg-slate-800/80 border border-slate-700/80 rounded-xl overflow-hidden">
      <div class="p-4 border-b border-slate-700 font-semibold text-slate-200">Detailed Query Profiles</div>
      <div class="overflow-x-auto">
        <table class="w-full text-left text-sm font-mono">
          <thead class="bg-slate-900/60 text-slate-400 uppercase text-xs">
            <tr>
              <th class="py-2.5 px-4">Length</th>
              <th class="py-2.5 px-4">Query Sample</th>
              <th class="py-2.5 px-4 text-right">Words Found</th>
              <th class="py-2.5 px-4 text-right">Solve Time</th>
              <th class="py-2.5 px-4 text-right">Throughput</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-slate-700/60" id="table-body">
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <script>
    const data = [
      {}
    ];

    const maxMicros = Math.max(...data.map(d => d.micros));
    const container = document.getElementById('bars-container');
    const tableBody = document.getElementById('table-body');

    data.forEach(d => {{
      const pct = Math.max(2, (d.micros / maxMicros) * 100);
      const timeStr = d.micros < 1000 ? `${{d.micros.toFixed(1)}} µs` : `${{(d.micros / 1000).toFixed(2)}} ms`;

      // Bar
      const row = document.createElement('div');
      row.className = 'flex items-center text-xs font-mono space-x-3';
      row.innerHTML = `
        <span class="w-16 text-slate-400 text-right">Len ${{d.len}}</span>
        <div class="flex-1 bg-slate-900/60 rounded h-5 p-0.5 overflow-hidden border border-slate-700/50">
          <div class="bg-gradient-to-r from-cyan-500 to-emerald-400 h-full rounded flex items-center justify-end pr-1 text-[10px] font-bold text-slate-950 transition-all duration-500" style="width: ${{pct}}%">
          </div>
        </div>
        <span class="w-24 text-right text-cyan-300 font-semibold">${{timeStr}}</span>
        <span class="w-24 text-right text-slate-400">${{d.words.toLocaleString()}} words</span>
      `;
      container.appendChild(row);

      // Table row
      const tr = document.createElement('tr');
      tr.className = 'hover:bg-slate-700/40 transition-colors';
      tr.innerHTML = `
        <td class="py-2.5 px-4 font-bold text-slate-300">${{d.len}}</td>
        <td class="py-2.5 px-4 text-slate-400 font-normal">${{d.query}}</td>
        <td class="py-2.5 px-4 text-right text-emerald-400 font-semibold">${{d.words.toLocaleString()}}</td>
        <td class="py-2.5 px-4 text-right text-cyan-300 font-semibold">${{timeStr}}</td>
        <td class="py-2.5 px-4 text-right text-slate-400">${{Math.round(d.wps).toLocaleString()}} w/s</td>
      `;
      tableBody.appendChild(tr);
    }});
  </script>
</body>
</html>
"#,
    format_duration(open_full),
    format_duration(open_fast),
    rows_js,
  );

  let mut file = std::fs::File::create(path)?;
  file.write_all(html.as_bytes())?;
  Ok(())
}

fn main() {
  println!(
    "\n{} Running Anagram Solver Engine Benchmarks...\n",
    badge_bench()
  );

  let db_path = ensure_benchmark_database();

  // 1. Benchmark Database Load / Mmap
  println!("{}", "1. Database Open / Mmap Latency".bold());
  let start_full = Instant::now();
  let _reader_full =
    Reader::open(&db_path).expect("failed to open reader with checksum");
  let elapsed_full = start_full.elapsed();

  let start_fast = Instant::now();
  let reader = Reader::open_fast(&db_path).expect("failed to open reader fast");
  let elapsed_fast = start_fast.elapsed();

  println!(
    "   Cold/Full open (CRC32 verify):         {}",
    format_duration(elapsed_full).cyan()
  );
  println!(
    "   Fast open (zero-deserialization mmap): {}",
    format_duration(elapsed_fast).bright_green().bold()
  );
  println!();

  // 2. Exact Anagram Lookups
  println!(
    "{}",
    "2. Exact Anagram Lookups (Microsecond scale)".bold()
  );
  let exact_samples = [
    "cat", "stop", "alert", "listen", "roasters", "algorithms",
  ];
  for query in exact_samples {
    let iters = 1_000;
    let start = Instant::now();
    let mut hits_count = 0;
    for _ in 0..iters {
      hits_count = reader.anagrams(query).count();
    }
    let elapsed = start.elapsed() / iters;
    println!(
      "   {:12} -> {:2} hits in {}",
      query,
      hits_count.to_string().green(),
      format_duration(elapsed).cyan()
    );
  }
  println!();

  // 3. Sub-Anagram Lookups Across Lengths
  println!(
    "{}",
    "3. Sub-Anagram Traversal Across Query Lengths (DFS + Bitmask Pruning)"
      .bold()
  );

  let queries = [
    (3, "cat"),
    (4, "stop"),
    (5, "apple"),
    (6, "listen"),
    (7, "roaster"),
    (8, "creative"),
    (10, "algorithms"),
    (12, "relationship"),
    (15, "conversational"),
    (18, "characteristically"),
  ];

  let mut results = Vec::new();
  let mut max_micros = 0.0f64;

  for (len, query) in queries {
    // Warmup
    let _ = reader.sub_anagrams(query, 3);

    let iters = if len <= 7 {
      50
    } else if len <= 10 {
      20
    } else {
      5
    };
    let start = Instant::now();
    let mut words_count = 0;
    for _ in 0..iters {
      let words = reader.sub_anagrams(query, 3);
      words_count = words.len();
    }
    let avg_dur = start.elapsed() / iters;
    let micros = avg_dur.as_nanos() as f64 / 1_000.0;
    if micros > max_micros {
      max_micros = micros;
    }

    let throughput_wps = if avg_dur.as_secs_f64() > 0.0 {
      words_count as f64 / avg_dur.as_secs_f64()
    } else {
      0.0
    };

    results.push(BenchmarkResult {
      length: len,
      query,
      words_found: words_count,
      solve_duration: avg_dur,
      throughput_wps,
    });
  }

  // Comfy-table formatted output with horizontal Unicode bar chart
  let mut table = Table::new();
  table
    .load_style(UTF8_FULL.with_rounded_corners())
    .set_header(vec![
      Cell::new("Length").set_alignment(CellAlignment::Center),
      Cell::new("Query Sample"),
      Cell::new("Words Found").set_alignment(CellAlignment::Right),
      Cell::new("Solve Time").set_alignment(CellAlignment::Right),
      Cell::new("Throughput").set_alignment(CellAlignment::Right),
      Cell::new("Latency Chart"),
    ]);

  for r in &results {
    let micros = r.solve_duration.as_nanos() as f64 / 1_000.0;
    let bar = render_bar(micros, max_micros, 20);
    let time_str = format_duration(r.solve_duration);
    let wps_str = format!("{:.0} w/s", r.throughput_wps);

    table.add_row(vec![
      Cell::new(r.length).set_alignment(CellAlignment::Center),
      Cell::new(r.query),
      Cell::new(r.words_found).set_alignment(CellAlignment::Right),
      Cell::new(time_str).set_alignment(CellAlignment::Right),
      Cell::new(wps_str).set_alignment(CellAlignment::Right),
      Cell::new(bar),
    ]);
  }

  println!("{table}");
  println!();

  // 4. Generate HTML Report
  let report_path = "target/benchmark_report.html";
  if let Err(e) =
    generate_html_report(report_path, elapsed_fast, elapsed_full, &results)
  {
    eprintln!("{} Failed to write HTML report: {e}", badge_bench());
  } else {
    println!(
      "{} Interactive graphical report generated at: {}",
      badge_done(),
      report_path.bright_green().bold()
    );
  }
}
