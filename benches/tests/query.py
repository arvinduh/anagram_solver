import pytest
from pytest_benchmark import fixture

from solver import solver


class TestQueryBenchmark:
  """Benchmark suite for anagram query lookups."""

  @pytest.mark.parametrize(
    "word",
    [
      "cat",
      "stop",
      "apple",
      "listen",
      "roaster",
      "creative",
      "algorithms",
    ],
  )
  def test_query(self, benchmark: fixture.BenchmarkFixture, word: str) -> None:
    """Benchmark query time across different word lengths."""
    result: dict[int, list[str]] = benchmark(
      solver.solve, word, max_letters=len(word), min_letters=3
    )
    assert result is not None
    assert sum(len(w) for w in result.values()) > 0
