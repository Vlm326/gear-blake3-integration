# FastCDC + BLAKE3

Исследуется совмещение разбиения данных на чанки FastCDC с вычислением хешей BLAKE3.

- `baseline`: сначала формирует чанки FastCDC, затем отдельно хеширует каждый чанк.
- `fused`: вычисляет BLAKE3 одновременно с поиском границ чанков.
- Тест сравнивает границы, содержимое чанков и хеши обеих реализаций.

Запуск: `cargo run --release -- [--mode baseline|fused] [--iterations N] <файл-или-каталог>`

Экспериментальные данные и результаты находятся в `artifacts/`: входные файлы — в `inputs/`, замеры — в `benchmarks/`, flamegraph — в `flamegraphs/`, а график ускорения создаётся как `charts/speedup.svg`. Схема каталогов описана в `artifacts/README.md`.

## Структура проекта

```text
.
├── artifacts
│   ├── benchmarks
│   │   ├── batch_sweep
│   │   │   ├── results2.txt
│   │   │   ├── results3.txt
│   │   │   ├── results5.txt
│   │   │   ├── results_baseline2.txt
│   │   │   ├── results_baseline3.txt
│   │   │   ├── results_baseline4.txt
│   │   │   ├── results_baseline5.txt
│   │   │   └── summary.md
│   │   └── chunk_sweep
│   │       ├── results.txt
│   │       ├── results4.txt
│   │       └── results_baseline.txt
│   ├── charts
│   │   ├── archive
│   │   │   ├── speedup_01.svg
│   │   │   ├── speedup_02.svg
│   │   │   ├── speedup_03.svg
│   │   │   └── speedup_04.svg
│   │   └── speedup.svg
│   ├── flamegraphs
│   │   ├── historical
│   │   │   ├── baseline-chunks-8-32-128.svg
│   │   │   ├── fused-64kib-chunks-8-32-128.svg
│   │   │   └── legacy_flamegraph.svg
│   │   ├── iterations_1
│   │   │   ├── baseline_min4_avg16_max64_batch1KiB.svg
│   │   │   ├── baseline_min4_avg16_max64_batch8KiB.svg
│   │   │   ├── baseline_min4_avg32_max64_batch1KiB.svg
│   │   │   ├── baseline_min4_avg32_max64_batch8KiB.svg
│   │   │   ├── fused_min4_avg16_max64_batch1KiB.svg
│   │   │   ├── fused_min4_avg16_max64_batch8KiB.svg
│   │   │   ├── fused_min4_avg32_max64_batch1KiB.svg
│   │   │   └── fused_min4_avg32_max64_batch8KiB.svg
│   │   └── iterations_10
│   │       ├── baseline_min4_avg16_max64_batch1KiB.svg
│   │       ├── baseline_min4_avg16_max64_batch8KiB.svg
│   │       ├── baseline_min4_avg32_max64_batch1KiB.svg
│   │       ├── baseline_min4_avg32_max64_batch8KiB.svg
│   │       ├── fused_min4_avg16_max64_batch1KiB.svg
│   │       ├── fused_min4_avg16_max64_batch8KiB.svg
│   │       ├── fused_min4_avg32_max64_batch1KiB.svg
│   │       └── fused_min4_avg32_max64_batch8KiB.svg
│   ├── inputs
│   │   ├── kernels/ (локальный корпус, исключён из Git)
│   │   └── wkiss_1.png
│   ├── profiles
│   │   └── perf
│   │       ├── perf.data
│   │       └── perf.data.old
│   ├── README.md
│   └── reports
│       ├── chunk_size_summary.md
│       └── initial_study
│           ├── feedback.md
│           ├── first_report.html
│           └── first_results.md
├── Cargo.lock
├── Cargo.toml
├── crates
│   └── fastcdc-rs (submodule)
│       ├── benches/chunking.rs
│       ├── Cargo.toml
│       ├── CHANGELOG.md
│       ├── .claude/skills/
│       │   ├── pr-review/SKILL.md
│       │   └── regenerate-gear-tables/SKILL.md
│       ├── CLAUDE.md
│       ├── examples/
│       │   ├── async2020.rs
│       │   ├── ronomon.rs
│       │   ├── stream2016.rs
│       │   ├── stream2020.rs
│       │   ├── table32.rs
│       │   ├── table64.rs
│       │   ├── table64ls.rs
│       │   ├── v2016.rs
│       │   ├── v2020.rs
│       │   └── v2020_cut.rs
│       ├── .github/workflows/test.yml
│       ├── .gitignore
│       ├── LICENSE
│       ├── README.md
│       ├── src/
│       │   ├── lib.rs
│       │   ├── ronomon/mod.rs
│       │   ├── v2016/mod.rs
│       │   └── v2020/
│       │       ├── async_stream_cdc.rs
│       │       └── mod.rs
│       ├── test/fixtures/SekienAkashita.jpg
│       └── TODO.org
├── .gitignore
├── .gitmodules
├── README.md
├── src
│   ├── analysys
│   │   ├── graph_builder.rs
│   │   ├── mod.rs
│   │   └── parser.rs
│   ├── baseline.rs
│   ├── fused.rs
│   ├── fused_stream_cdc.rs
│   ├── main.rs
│   ├── pub_utils.rs
│   └── types.rs
├── test_diffirent_chunk.py
└── .vscode
    └── settings.json
```
