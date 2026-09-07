import pathlib
from typing import Any

import pandas as pd
import pytest


def pytest_addoption(parser: pytest.Parser) -> None:
  """Register custom CLI and ini options for CSV benchmark reporting."""
  parser.addoption(
    "--benchmark-csv",
    action="store",
    default=None,
    metavar="PATH",
    help="Store benchmark results into a custom CSV file schema.",
  )
  parser.addini(
    "benchmark_csv",
    default=None,
    help="Default path for custom benchmark CSV export.",
  )


def pytest_sessionfinish(session: pytest.Session) -> None:
  """Hook called after the test session finishes."""
  csv_path: str | None = session.config.getoption(
    "--benchmark-csv"
  ) or session.config.getini("benchmark_csv")
  if not csv_path:
    return

  bench_session = getattr(session.config, "_benchmarksession", None)
  if not bench_session or not bench_session.benchmarks:
    return

  output_file = pathlib.Path(csv_path)
  output_file.parent.mkdir(parents=True, exist_ok=True)

  rows: list[dict[str, Any]] = []

  for bench in bench_session.benchmarks:
    s = bench.stats
    params: dict[str, Any] = bench.params or {}

    op = "parse" if "parse" in bench.name.lower() else "query"
    target = str(
      params.get("source")
      or params.get("word")
      or params.get("file_name")
      or params.get("query")
      or (",".join(f"{k}={v}" for k, v in params.items()) if params else "")
    )

    rows.append(
      {
        "op": op,
        "target": target,
        "rounds": s.rounds,
        "iterations": bench.iterations,
        "mean_s": s.mean,
        "min_s": s.min,
        "max_s": s.max,
        "median_s": s.median,
        "stddev_s": s.stddev,
      }
    )

  df = pd.DataFrame(rows)
  df.to_csv(output_file, index=False)

  reporter = session.config.pluginmanager.get_plugin("terminalreporter")
  if reporter:
    reporter.write_line(
      f"\n[benchmark-csv] Saved {len(df)} benchmark results via pandas to {output_file}"
    )
