import pathlib
import shutil
import tempfile

import pytest
from pytest_benchmark import fixture

from word_bank import add_words

WORD_BANKS: dict[str, pathlib.Path] = {
  "dwyl_english": pathlib.Path("./files/original/words_alpha.txt"),
}


class TestParseBenchmark:
  """Benchmark suite for word bank parsing and ingestion."""

  @pytest.mark.parametrize("source", list(WORD_BANKS.keys()))
  def test_parse(
    self, benchmark: fixture.BenchmarkFixture, source: str
  ) -> None:
    """Benchmark reading, parsing, length-bucketing, and writing word files."""
    file_path = WORD_BANKS[source]
    created_dirs: list[str] = []

    def run_parse() -> None:
      tmpdir = tempfile.mkdtemp(prefix="agrm_bench_parse_")
      created_dirs.append(tmpdir)
      add_words.add_words(str(file_path), dst_dir=tmpdir)

    try:
      benchmark.pedantic(run_parse, rounds=5, iterations=1)
    finally:
      for d in created_dirs:
        shutil.rmtree(d, ignore_errors=True)
