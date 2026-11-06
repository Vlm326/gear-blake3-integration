<div align="center">

# FastCDC + BLAKE3

A Rust research harness for chunking and hashing pipelines

**English** &nbsp; / &nbsp; [Русский](README.ru.md)

[![CI](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows-blue)](.github/workflows/ci.yml)
[![Code license: MIT](https://img.shields.io/badge/code_license-MIT-green)](LICENSE)

[Quick start](#quick-start) · [Modes](#modes) · [Benchmarks](#benchmarks) · [Documentation](#documentation)

</div>

FastCDC + BLAKE3 is a CLI for comparing ways to combine content-defined chunking with hashing. It studies how processing order, BLAKE3 update frequency, and SIMD boundary scanning affect execution time, instruction counts, and cache behavior.

Valid comparisons require identical chunk boundaries, contents, and BLAKE3 digests. This project is a research harness; results depend on the input, configuration, architecture, and measurement procedure.

## Quick start

Requires a Rust toolchain supporting edition 2024 and Git. Run all commands from the repository root.

```sh
git clone --recurse-submodules https://github.com/Vlm326/gear-blake3-integration.git
cd gear-blake3-integration
cargo build --release --locked

./target/release/hash_combination --mode baseline --iterations 10 README.md
./target/release/hash_combination --mode fused-base --iterations 10 README.md
```

For an existing checkout, run `git submodule update --init --recursive`. On Windows, use `target/release/hash_combination.exe`.

These commands demonstrate execution using a repository file. Use representative data for measurements. Each file is read once; iterations process the in-memory buffer. The CLI consumes digests internally rather than printing them.

## Modes

| Mode | Processing strategy |
| --- | --- |
| `baseline` | Collect all FastCDC chunks, then run a separate BLAKE3 pass. |
| `fused-base` | Scalar scanning; update BLAKE3 at region or chunk boundaries. |
| `fused-windowed` | Alternate scanning and BLAKE3 in bounded windows. |
| `fused-interleaved` | Check BLAKE3 progress after each byte pair; fixed 64-byte threshold. |
| `fused-simd-check` | Sequential Gear states with SIMD checks of four candidates. |
| `fused-simd-expanded` | Expanded Gear state formulas with SIMD candidate checks. |
| `fused-simd-stripes` | Parallel strip scanning using vendored `gearhash`. |

`chunking-only`, `prepare-chunks`, and `blake3-only` isolate individual stages. See the [CLI reference](docs/cli.md) for parameters, CPU requirements, chunk maps, and SIMD variants.

Default min/avg/max chunk sizes are 16/64/256 KiB, configured through `MIN_CHUNK_SIZE`, `AVG_CHUNK_SIZE`, and `MAX_CHUNK_SIZE`. `BLAKE3_BATCH` has mode-specific semantics: an update threshold and a bounded interleaving window represent different experiments.

## Benchmarks

### Reproduce a comparison

Build the release binary first. On Linux, install `perf` and `taskset` and enable access to the required counters. Choose an input file and a CPU available on your machine.

```sh
# Linux: choose a CPU available to this process.
env LC_ALL=C BLAKE3_BATCH=8192 \
  MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 \
  perf stat -r 5 -e cycles:u,instructions:u -- \
  taskset -c 3 ./target/release/hash_combination \
  --mode fused-base --iterations 50 /path/to/input.bin
```

Repeat with `--mode baseline` using the same settings. `perf stat -r` repeats the process; `--iterations` repeats processing within the process. External measurements include input loading and allocations; caches are not cleared between iterations.

Record input size and provenance, CPU/architecture, commit, local changes, configuration, execution order, and variance. Unavailable counters are not zero. All full processing modes retain chunk copies; baseline retains all chunks while fused variants release them individually. Isolated stage times need not add up to baseline time.

- [perf stat commands and comparison methodology](docs/cache_profiling_commands.md) (Russian).
- [Batch and chunk-size sweep](scripts/benchmark_chunk_sweep.py) — requires a local input and `perf`; appends results.
- [Linux kernel archive measurements](scripts/benchmark_kernel_cache.py) — requires Python 3.11+, GNU tar, `perf`, `taskset`, and local kernel trees.
- [Input preparation, script usage, and output locations](scripts/README.md) (Russian).

### Published examples

The links below pin materials from the `reports` branch to a specific revision. They describe historical implementations and retain earlier mode names; their figures do not automatically apply to the current CLI.

| Material | Link |
| --- | --- |
| Batch sweep | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/benchmarks/batch_sweep/summary.md) |
| Chunk-size comparison | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/reports/chunk_size_summary.md) |
| Raw fused measurements | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/benchmarks/chunk_sweep/results.txt) |
| Raw baseline measurements | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/benchmarks/chunk_sweep/results_baseline.txt) |
| Speedup chart | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/charts/speedup.svg) |
| Historical flamegraphs | [GitHub](https://github.com/Vlm326/gear-blake3-integration/tree/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/flamegraphs/historical) |

Store inputs, profiles, and new run results locally under `artifacts/`. Published materials are maintained separately in the [reports branch](https://github.com/Vlm326/gear-blake3-integration/tree/reports).

## Documentation

| Document | Scope |
| --- | --- |
| [CLI reference](docs/cli.md) | Parameters, SIMD variants, chunk maps, and SVG reports. |
| [Research specification](docs/TZ.md) | Objectives and comparison criteria (Russian). |
| [Measurement guide](docs/cache_profiling_commands.md) | perf events, CPU affinity, and formulas (Russian). |
| [Scripts](scripts/README.md) | Requirements, inputs, and run outputs (Russian). |
| [Contributor guide](AGENTS.md) | Project structure and change requirements (Russian). |

## Validation

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
python3 scripts/ci_smoke.py
```

The smoke test requires Python 3.9+, generates temporary data, and checks mode execution and the chunk-map round trip. Rust tests compare boundaries, contents, and digests against FastCDC. [CI](.github/workflows/ci.yml) covers Linux x86_64/aarch64 and Windows x86_64; benchmarks run separately.

The SVG tool uses the `reports` feature. Validate it with `cargo test --features reports --bin build-graph --locked` and `cargo clippy --all-targets --features reports --locked -- -D warnings`.

## Licenses

Original code: [MIT](LICENSE). READMEs: [CC BY-SA 4.0](LICENSES/CC-BY-SA-4.0.md). Research materials and raw results have separate license scopes documented in [LICENSES](LICENSES/). FastCDC (`crates/fastcdc-rs`) retains MIT; vendored `gearhash` retains MIT/Apache-2.0 and author notices.
