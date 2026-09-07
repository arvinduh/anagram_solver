<div align="center">
    <h1>anagram_solver</h1>
    <img src="https://github.com/shogun-olives/anagram_solver/actions/workflows/python-app.yml/badge.svg" alt="Workflow status badge">
</div>

# solves anagrams - for GamePigeon

Uses a word bank to solve anagrams and sub-anagrams from a letter rack.

---

## Setup

Set up a dedicated virtual environment and install the dependencies:

```powershell
python -m venv .venv
.\.venv\Scripts\pip install -r requirements.txt
```

On macOS / Linux:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

---

## Usage

### 1. Ingesting Words into the Word Bank

The solver uses pre-filtered and length-bucketed word files stored in
`files/sorted/`. To ingest or re-build the word bank from the raw dictionary:

```powershell
.\.venv\Scripts\python -m word_bank.add_words
```

By default, this parses `./files/original/Collins Scrabble Words (2019).txt` and
creates/updates `files/sorted/{N}_letter_words.txt` for word lengths between 3
and 8 letters.

---

### 2. Running the Anagram Solver

#### Interactive CLI

Launch the interactive console solver:

```powershell
.\.venv\Scripts\python main.py
```

_(or `.\.venv\Scripts\python -m solver.console`)_

**How to Use the Interactive Console:**

1. **Input Letters**: When prompted (`[=] Enter 6 letters:`), type your letters
   (e.g. `listen`).
2. **Browse Anagrams**: The console displays the longest anagram matches first
   (e.g., 6-letter anagrams).
3. **Navigate**:
   - Press **`<Enter>`** (or any key) to step down to smaller anagrams (e.g.,
     5-letter, 4-letter, 3-letter words).
   - Enter **`p`** or **`prev`** to step back up to longer anagrams.
   - Enter a number (e.g., **`4`**) to jump directly to anagrams of that word
     length.
   - Enter **`e`** or **`exit`** to quit.

#### Programmatic Usage (Python API)

You can also import and use the solver directly in your Python code:

```python
from solver import solver

# Find all anagrams and sub-anagrams for a given letter rack
results = solver.solve(letters="listen", min_letters=3, max_letters=6)

# results is a dictionary keyed by word length:
# {
#   6: ['listen', 'silent', 'enlist', 'inlets', ...],
#   5: ['inlet', 'stein', 'tines', ...],
#   4: ['nest', 'line', 'ties', ...],
#   3: ['let', 'sit', 'ten', ...]
# }
for length, words in results.items():
    print(f"{length}-letter words ({len(words)}): {', '.join(words[:5])}...")
```

---

## Benchmark Suite & Profiling

The project includes a benchmark suite in `benches/tests/` to measure lookup
latency and word bank ingestion throughput, with support for exporting metrics
to CSV and comparing multiple engine runs (e.g., Python vs Rust).

### Benchmark Tests

- **`benches/tests/query.py`**: Benchmarks `solver.solve()` across various word
  lengths (`cat`, `stop`, `apple`, `listen`, `roaster`, `creative`,
  `algorithms`).
- **`benches/tests/parse.py`**: Benchmarks dictionary ingestion, bucketing, and
  writing throughput using Collins Scrabble Words.

---

### Profiling into CSV

Benchmark results can be exported directly to a custom CSV schema using the
`--benchmark-csv` flag configured in `conftest.py`.

#### Profile the complete suite (queries + ingestion parsing):

```powershell
.\.venv\Scripts\pytest --benchmark-csv=benches/data/python.csv
```

The resulting CSV contains columns for `op`, `target`, `rounds`, `iterations`,
`mean_s`, `min_s`, `max_s`, `median_s`, and `stddev_s`.

---

### Comparing Benchmark Results

Use `benches/compare.py` to compare benchmark CSV datasets side-by-side and
calculate relative speedups.

#### 1. Auto-discover and compare all CSVs

When run without file arguments, `compare.py` automatically discovers all `.csv`
files in `benches/data/`:

```powershell
.\.venv\Scripts\python benches/compare.py
```

#### 2. Compare specific benchmark CSVs

Provide individual CSV files to compare (the fastest and slowest engines will be
highlighted with speedup factors):

```powershell
.\.venv\Scripts\python benches/compare.py benches/data/python.csv benches/data/rust.csv
```

You can also pass glob patterns or directory paths:

```powershell
.\.venv\Scripts\python benches/compare.py benches/data/*.csv
```

#### Output Example

```text
            op                      target         python           rust        speedup
         parse collins_scrabble_words_2019      565.55 ms      120.30 ms           4.7x
         query                         cat        0.98 ms        0.05 ms          19.6x
         query                        stop        5.56 ms        0.18 ms          30.9x
         query                       apple       17.48 ms        0.42 ms          41.6x
         query                      listen       44.61 ms        0.85 ms          52.5x
         query                     roaster       77.79 ms        1.20 ms          64.8x
         query                    creative      118.88 ms        1.65 ms          72.0x
         query                  algorithms      108.29 ms        1.90 ms          57.0x
```

#### Visualization Flags

- **`--plot`** (or **`-p`**): Open an interactive Seaborn line chart displaying
  mean latency across word lengths and operations.
  ```powershell
  .\.venv\Scripts\python benches/compare.py --plot
  ```
- **`--save <PATH>`**: Save the benchmark comparison chart directly to an image
  file.
  ```powershell
  .\.venv\Scripts\python benches/compare.py --save benches/data/benchmark_comparison.png
  ```
- **`--noplot`**: Suppress chart generation (default behavior when only tabular
  output is needed in terminals or CI environments).
