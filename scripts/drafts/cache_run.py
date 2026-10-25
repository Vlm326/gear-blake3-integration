DEFAULT_BATCH = 8 * 1024
DEFAULT_CHUNK_SIZES = (4 * 1024, 10 * 1024, 64 * 1024)


def run_benches(batch=DEFAULT_BATCH, chunk_sizes=DEFAULT_CHUNK_SIZES):
    """Run every batch/chunk combination and alternate which mode runs first."""
    configuration_index = 0
    with open("./artifacts/benchmarks/cache_info/results.txt", "a") as fused_log:
                print(f"Running batch={batch} with chunk sizes={chunk_sizes}")
                min_chunk, avg_chunk, max_chunk = chunk_sizes
                env = os.environ.copy()
                env["BLAKE3_BATCH"] = str(batch)
                env["MIN_CHUNK_SIZE"] = str(min_chunk)
                env["AVG_CHUNK_SIZE"] = str(avg_chunk)
                env["MAX_CHUNK_SIZE"] = str(max_chunk)

                runs = [
                    ("fused", COMMAND_FUSED_FOR_HEATING, COMMAND_FUSED, fused_log),
                    (
                        "baseline",
                        COMMAND_BASELINE_FOR_HEATING,
                        COMMAND_BASELINE,
                        baseline_log,
                    ),
                ]
                if configuration_index % 2:
                    runs.reverse()

                for mode, heating_command, benchmark_command, log in runs:
                    result = run_benchmark(heating_command, benchmark_command, env)
                    write_result(log, mode, batch, chunk_sizes, result)

                configuration_index += 1


def main():
    print("Starting benchmarks...")
    run_benches()
    print("Benchmarks completed.")
