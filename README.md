# FastCDC + BLAKE3

[English](README.md) | [Русский](README.ru.md)

[![CI](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows-blue)](.github/workflows/ci.yml)
[![Code license: MIT](https://img.shields.io/badge/code_license-MIT-green)](LICENSE)

A Rust 2024 CLI for studying how combining FastCDC chunking and BLAKE3 hashing affects execution time, instruction counts, and cache behavior. Comparisons require identical chunk boundaries, contents, and BLAKE3 digests.

## Build and run

Use a Rust toolchain with Rust 2024 support. Run all commands from the repository root. FastCDC is a Git submodule; initialize it before building:

```sh
git submodule update --init --recursive
cargo build --release --locked
./target/release/hash_combination --mode baseline --iterations 10 README.md
./target/release/hash_combination --mode fused-base --iterations 10 README.md
```

On Windows, the executable is `target/release/hash_combination.exe`. The commands above use a file included in the repository to demonstrate the CLI; use your own representative data for performance measurements.

```text
hash_combination [--mode MODE] [--iterations N] <file-or-directory> ...
```

The default is `baseline` with one iteration. Directories are traversed recursively. Each file is read once, then processed from memory for every iteration.

## Modes

| Mode | Behavior |
| --- | --- |
| `baseline` | Collect FastCDC chunks, then hash each chunk separately. |
| `fused-base` | Scalar scanning; BLAKE3 updates at region or chunk boundaries. |
| `fused-windowed` | Experimental alternating Gear/BLAKE3 windows bounded by `BLAKE3_BATCH`. |
| `fused-simd-check` | Experimental AVX2 scanner checking four candidates together, with scalar Gear state updates. |
| `fused-simd-expanded` | Expanded Gear state formulas with AVX2 candidate checks. |
| `fused-simd-stripes` | Parallel strip scanning using the vendored `gearhash` library. |
| `fused-interleaved` | Historical interleaved implementation checking BLAKE3 progress after every Gear candidate and updating in 64-byte portions. |
| `chunking-only` | FastCDC with baseline chunk allocations and copies, without hashing. |
| `prepare-chunks` | Save FastCDC boundaries for one input file. |
| `blake3-only` | Hash slices of the original input using a saved chunk map. |

`fused-simd-check` and `fused-simd-expanded` detect AVX2 at runtime and have scalar fallbacks. `fused-simd-stripes` requires AVX2 on x86_64 or NEON on aarch64. Global `target-feature` flags are unnecessary.

Chunk sizes are configured in bytes with `MIN_CHUNK_SIZE`, `AVG_CHUNK_SIZE`, and `MAX_CHUNK_SIZE`; defaults are 16384, 65536, and 262144. Use valid even FastCDC sizes with `min <= avg <= max`. `BLAKE3_BATCH` controls batching in implementations that use it; its meaning differs between modes.

`fused-windowed` defaults to a window of 8192 bytes and requires a positive even batch. It retains chunk copies and stream-buffer shifts. It reads settings once per stream, while the original fused-base implementation reads batch per chunk. Reducing batch in `fused-base` is not equivalent to bounding windows in `fused-windowed`.

Experimental AVX2 options for `fused-simd-stripes`:

- `--unroll 1|2|4|8`: choose the unroll factor; `1` selects the original scanner.
- `--scanner original|control|expanded|grouped-check|combined`: compare sequential computation, unrolling, combined boundary checks, or both changes.
- `--strip-size 128|256|512|1024 --strips 2|4`: set strip size and count together; their product is the SIMD window size (originally 256 × 4 bytes).

These three option groups are mutually exclusive and require AVX2. Without them, architecture selection remains automatic. Short input slices use scalar tails, so scanner window experiments also depend on batch size. In `fused-simd-stripes`, BLAKE3 batch thresholds are checked after complete FastCDC regions, with the remaining bytes hashed at chunk boundaries; scanner windows do not trigger intermediate BLAKE3 updates. Allocation and copying differences must be accounted for when comparing modes.

## Isolate chunking and hashing

Choose an unused map path; an existing map is never overwritten:

```sh
./target/release/hash_combination --mode prepare-chunks --chunk-map /tmp/readme.chunks.tsv README.md
./target/release/hash_combination --mode chunking-only --iterations 10 README.md
./target/release/hash_combination --mode blake3-only --chunk-map /tmp/readme.chunks.tsv --iterations 10 README.md
```

On Windows, replace `/tmp/readme.chunks.tsv` with a writable local path. `prepare-chunks` and `blake3-only` require exactly one regular file and `--chunk-map`. Preparation allows only one iteration.

Maps are TSV: a `# hash_combination chunk-map v1` header, `file_size<TAB>bytes`, `chunk_sizes<TAB>min<TAB>avg<TAB>max`, then `offset<TAB>length` pairs. Loading checks file size, settings, and complete contiguous coverage. It does not detect changed content of the same size. Recreate the map when the input or chunking settings change. Chunk-size variables and batch do not affect `blake3-only`.

`chunking-only` retains baseline allocations; `blake3-only` hashes original-buffer slices. Their times need not add up to baseline time.

## Measurement and optional graphs

See [perf commands](docs/cache_profiling_commands.md), [measurement scripts](scripts/README.md), and the [research specification](docs/TZ.md). Build first, then run `perf` directly on the binary. Record the input, machine, commit and local changes, settings, iterations, repetitions, CPU, and supported events. External `perf stat` includes loading and allocations; caches are not cleared between iterations.

`main` contains source code, instructions, and scripts. Reports, presentations, benchmark results, charts, flamegraphs, profiles, and input datasets are excluded from this branch. Published research materials are kept separately in the Git branch `reports`; its contents may reflect an earlier experiment or code revision.

Local runs may create `artifacts/`; the entire directory is ignored in `main`. It is not part of a fresh checkout. Scripts document which inputs and output directories must be prepared. `.local-research/` and `kernels/` are also local and ignored.

The optional graph tool remains available:

```sh
cargo run --release --locked --features reports --bin build-graph
```

It takes no arguments. First supply local measurement files `results.txt` and `results_baseline.txt` in `artifacts/benchmarks/chunk_sweep/`; it writes `artifacts/charts/speedup.svg`. The batch sweep script instead writes to `artifacts/benchmarks/batch_sweep/`: copy the selected matching pair to `chunk_sweep/` before plotting. These inputs are not shipped in `main`. Plotters is enabled only by the Cargo feature `reports`, using its SVG backend without Fontconfig or FreeType. The Cargo feature and Git branch named `reports` are separate things.

## Checks and CI

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
python3 scripts/ci_smoke.py
cargo test --features reports --bin build-graph --locked
cargo clippy --all-targets --features reports --locked -- -D warnings
```

The smoke test requires Python 3.9+ and a release build, creates temporary data, and checks CLI execution and the chunk-map round trip. Rust tests compare chunk boundaries, contents, and digests and validate maps without local datasets.

[CI](.github/workflows/ci.yml) runs on pushes to `main`, pull requests, and manual dispatch. It checks formatting, strict Clippy, release builds, tests, and the CLI on Linux x86_64, Linux aarch64, and Windows x86_64. Reporting-tool checks run separately on Linux. CI does not run benchmarks or generate research artifacts.

## Repository layout

Tracked directories in `main` (generated and local files omitted):

```text
.
├── README.md                  # English
├── README.ru.md               # Russian
├── AGENTS.md                  # Contributor instructions
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── LICENSES/                  # Creative Commons scope notices
├── .github/workflows/ci.yml
├── .vscode/                   # Editor settings
├── crates/
│   ├── fastcdc-rs/            # Git submodule
│   └── gearhash/              # Vendored library
├── docs/
│   ├── TZ.md
│   └── cache_profiling_commands.md
├── scripts/                   # Measurements and CLI smoke test
│   ├── README.md
│   └── drafts/                # Unfinished, unsupported scripts
└── src/
    ├── main.rs                # CLI and input loading
    ├── bin/build-graph.rs      # Optional graph tool
    └── analysys/              # Result parsing and SVG output
```

## Components and licenses

Project source code is [MIT](LICENSE). FastCDC uses Nathan Fiedler's [`fastcdc-rs`](https://github.com/nlfiedler/fastcdc-rs) submodule under MIT. Vendored [`gearhash`](https://github.com/srijs/rust-gearhash) 0.1.4 retains its MIT and Apache-2.0 licenses and copyright notices.

Both README translations are covered by [CC BY-SA 4.0](LICENSES/CC-BY-SA-4.0.md). The existing scope notices for research summaries, reports, and visualizations ([CC BY-SA 4.0](LICENSES/CC-BY-SA-4.0.md)) and raw benchmark results and profiles ([CC BY 4.0](LICENSES/CC-BY-4.0.md)) remain applicable to those materials when stored separately. Third-party code and materials retain their own licenses.

Renamed modes: `fused` → `fused-base`, `historical` → `fused-interleaved`, `simd` → `fused-simd-check`, `simd-bad` → `fused-simd-expanded`, `gearhash-simd` → `fused-simd-stripes`. Utility mode names now use hyphens. `BLAKE3_BATCH` retains its existing mode-dependent behavior.
