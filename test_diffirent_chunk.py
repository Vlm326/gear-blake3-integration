import os
import re
import subprocess
from dataclasses import dataclass, field
from typing import Optional


COMMAND_FUSED = "perf stat -r 10 -e cycles,instructions,cache-references,cache-misses -- ./target/release/hash_combination --mode fused --iterations 50 ./test-files/wkiss_1.png"
COMMAND_FUSED_FOR_HEATING = "./target/release/hash_combination --mode fused --iterations 50 ./test-files/wkiss_1.png"
COMMAND_BASELINE = "perf stat -r 10 -e cycles,instructions,cache-references,cache-misses -- ./target/release/hash_combination --mode baseline --iterations 50 ./test-files/wkiss_1.png"
COMMAND_BASELINE_FOR_HEATING = "./target/release/hash_combination --mode baseline --iterations 50 ./test-files/wkiss_1.png"
batches = [64, 256, 1024, 8 * 1024, 16 * 1024, 32 * 1024, 64 * 1024, 128 * 1024]

chunk_sizes_list = [
    (16 * 1024, 64 * 1024, 256 * 1024),
    (8 * 1024, 32 * 1024, 128 * 1024),
    (32 * 128, 128 * 1024, 512 * 1024),
]


@dataclass(frozen=True)
class PerfResult:
    command: Optional[str]
    runs: Optional[int]
    cycles: Optional[int]
    instructions: Optional[int]
    cache_references: Optional[int]
    cache_misses: Optional[int]
    elapsed_seconds: Optional[float]
    elapsed_spread_seconds: Optional[float]
    elapsed_spread_percent: Optional[float]
    counter_spread_percent: dict[str, float] = field(default_factory=dict)


_HEADER_RE = re.compile(
    r"Performance counter stats for (?P<command>.+?)\s+"
    r"\((?P<runs>\d+) runs?\):"
)
_COUNTER_RE = re.compile(
    r"^\s*(?P<value>[\d][\d\s\u00a0\u202f,]*)\s+"
    r"(?P<event>cycles(?::\w+)?|instructions(?::\w+)?|"
    r"cache-references(?::\w+)?|cache-misses(?::\w+)?)\b"
    r".*?(?:\(\s*\+\-\s*(?P<spread>[\d.,]+)%\s*\))?\s*$",
    re.MULTILINE,
)
_ELAPSED_RE = re.compile(
    r"(?P<elapsed>[\d]+(?:[.,][\d]+)?)\s*\+\-\s*"
    r"(?P<spread>[\d]+(?:[.,][\d]+)?)\s+seconds time elapsed"
    r"\s*\(\s*\+\-\s*(?P<percent>[\d]+(?:[.,][\d]+)?)%\s*\)"
)


def _parse_float(value: str) -> float:
    return float(value.replace(",", "."))


def _parse_counter(value: str) -> int:
    return int(re.sub(r"[\s\u00a0\u202f,]", "", value))


def parse_output(output: str) -> list[PerfResult]:

    headers = list(_HEADER_RE.finditer(output))
    if not headers:
        raise ValueError("В выводе не найден заголовок 'Performance counter stats'")

    results = []
    for index, header in enumerate(headers):
        end = headers[index + 1].start() if index + 1 < len(headers) else len(output)
        block = output[header.end() : end]
        counters: dict[str, Optional[int]] = {
            "cycles": None,
            "instructions": None,
            "cache-references": None,
            "cache-misses": None,
        }
        counter_spread_percent = {}

        for match in _COUNTER_RE.finditer(block):
            event = match.group("event").split(":", maxsplit=1)[0]
            counters[event] = _parse_counter(match.group("value"))
            spread = match.group("spread")
            if spread is not None:
                counter_spread_percent[event] = _parse_float(spread)

        elapsed_match = _ELAPSED_RE.search(block)
        if (
            not any(value is not None for value in counters.values())
            and not elapsed_match
        ):
            raise ValueError(
                f"В блоке perf не найдены счётчики или время: {header.group('command')}"
            )

        results.append(
            PerfResult(
                command=header.group("command").strip().strip("'\""),
                runs=int(header.group("runs")),
                cycles=counters["cycles"],
                instructions=counters["instructions"],
                cache_references=counters["cache-references"],
                cache_misses=counters["cache-misses"],
                elapsed_seconds=(
                    _parse_float(elapsed_match.group("elapsed"))
                    if elapsed_match
                    else None
                ),
                elapsed_spread_seconds=(
                    _parse_float(elapsed_match.group("spread"))
                    if elapsed_match
                    else None
                ),
                elapsed_spread_percent=(
                    _parse_float(elapsed_match.group("percent"))
                    if elapsed_match
                    else None
                ),
                counter_spread_percent=counter_spread_percent,
            )
        )

    return results


def run_benches():
    """
    Run benchmarks for different batch sizes and chunk sizes.
    """

    with open("./batches_tests/results.txt", "a") as f:
        for batch in batches:
            for chunk_sizes in chunk_sizes_list:
                min_chunk, avg_chunk, max_chunk = chunk_sizes
                f.write(f"Running batch {batch} with chunk sizes: min={min_chunk}, avg={avg_chunk}, max={max_chunk}\n")
                f.flush()
                env = os.environ.copy()
                """
                        push env vars into rust code
                    """
                env["BLAKE3_BATCH"] = str(batch)
                env["MIN_CHUNK_SIZE"] = str(min_chunk)
                env["AVG_CHUNK_SIZE"] = str(avg_chunk)
                env["MAX_CHUNK_SIZE"] = str(max_chunk)

                """
                        Run the benchmark for the current batch and chunk sizes.
                        first of all running heating phase.
                    """

                heating = subprocess.run(
                    COMMAND_FUSED_FOR_HEATING,
                    shell=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    env=env,
                    text=True,
                )

                finished_process = subprocess.run(
                    COMMAND_FUSED,
                    shell=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    env=env,
                    text=True,
                )
                result = parse_output(finished_process.stdout)[0]

                f.write(
                    f"Batch {batch}:\n cycles: {result.cycles}\n instructions: {result.instructions}\n cache_references: {result.cache_references}\n cache_misses: {result.cache_misses}\n elapsed_seconds: {result.elapsed_seconds}\n elapsed_spread_seconds: {result.elapsed_spread_seconds}\n elapsed_spread_percent: {result.elapsed_spread_percent}\n counter_spread_percent: {result.counter_spread_percent}\n"
                )
                f.flush()

                with open("./batches_tests/results_baseline.txt", "a") as f_base:
                    heating = subprocess.run(
                        COMMAND_BASELINE_FOR_HEATING,
                        shell=True,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT,
                        env=env,
                        text=True,
                    )

                    finished_process = subprocess.run(
                        COMMAND_BASELINE,
                        shell=True,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT,
                        text=True,
                        env=env,
                    )
                    result = parse_output(finished_process.stdout)[0]
                    f_base.write(
                        f"Baseline:\n cycles: {result.cycles}\n instructions: {result.instructions}\n cache_references: {result.cache_references}\n cache_misses: {result.cache_misses}\n elapsed_seconds: {result.elapsed_seconds}\n elapsed_spread_seconds: {result.elapsed_spread_seconds}\n elapsed_spread_percent: {result.elapsed_spread_percent}\n counter_spread_percent: {result.counter_spread_percent}\n"
                    )
                    f_base.flush()


def main():
    run_benches()


if __name__ == "__main__":
    main()
