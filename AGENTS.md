# Repository Guidelines

## Project Structure & Module Organization

This is a Rust 2024 command-line project. The executable and application modules live in `src/`: `baseline.rs`, `fused.rs`, and `historical_fused.rs` implement the comparison modes, while `analysys/` parses benchmark output and builds graphs. The FastCDC dependency is a Git submodule at `crates/fastcdc-rs`; avoid editing it for application changes. `artifacts/` contains benchmark inputs, results, profiles, and charts. Some inputs are local-only and are not committed. There is no separate top-level `tests/` directory; focused Rust unit tests live beside their code.

Keep research documents in `docs/`, benchmark scripts in `scripts/`, and unfinished scripts in `scripts/drafts/`. Presentation decks, speaker notes, and display copies of flamegraphs belong together under `artifacts/presentations/<date>/`. Write temporary profiles to the ignored `artifacts/profiles/local/` directory rather than the repository root; keep curated profiles in `artifacts/profiles/perf/`.

## Build, Test, and Development Commands

- `cargo build` builds the debug executable; use `cargo build --release` for performance runs.
- `cargo test` runs the project’s Rust unit tests.
- `cargo fmt --check` checks standard Rust formatting; run `cargo fmt` before submitting changes.
- `cargo run --release -- --mode fused --iterations 10 <file-or-directory>` runs a benchmark mode over a file or recursively collected directory. Modes are `baseline`, `fused`, and `historical`.
- `cargo run --release -- --mode build-graph` reads the chunk-sweep results in `artifacts/benchmarks/chunk_sweep/` and writes a chart under `artifacts/charts/`.
- `python3 scripts/benchmark_chunk_sweep.py` runs the batch/chunk sweep from the repository root; see `scripts/README.md` for prerequisites and profiling output paths. Scripts in `scripts/drafts/` are unfinished and are not supported entrypoints.

## Coding Style & Naming Conventions

Follow rustfmt defaults: four-space indentation, trailing commas in multiline constructs, and idiomatic Rust formatting. Use `snake_case` for modules, functions, and variables; use `UpperCamelCase` for types and enums. Keep benchmark and analysis logic separated from CLI argument handling. Preserve the three modes’ comparable behavior and document any intentional differences in output or measurement methodology.

## Testing Guidelines

Add unit tests beside the implementation under `#[cfg(test)]` modules. Name tests for observable behavior, such as `fused_matches_baseline_chunks`. When changing chunking or hashing logic, cover chunk boundaries, chunk contents, and resulting hashes where applicable. Run `cargo test` and `cargo fmt --check` before submitting.

## Commit & Pull Request Guidelines

Recent commits use short, imperative, conventional prefixes, for example `feat(cli): add historical interleaved hashing mode` and `docs: clarify licenses and benchmark modes`. Follow that pattern (`feat`, `fix`, `docs`, `chore`) with a concise subject. Pull requests should explain the behavior or measurement change, list validation commands, and include representative benchmark results or chart screenshots when results are central. Keep generated profiles and local input data out of unrelated code changes, and do not commit changes inside the FastCDC submodule unless the dependency itself is intentionally updated. 

## Data and Configuration

Benchmark timings depend on the input and machine. Record the mode, iteration count, and relevant chunk or batch settings with results. Check `.gitignore` before adding generated outputs; never add private or unlicensed input files.
