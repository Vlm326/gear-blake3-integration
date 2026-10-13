# FastCDC + BLAKE3

Исследуется совмещение разбиения данных на чанки FastCDC с вычислением хешей BLAKE3.

- `baseline`: сначала формирует чанки FastCDC, затем отдельно хеширует каждый чанк.
- `fused`: вычисляет BLAKE3 одновременно с поиском границ чанков.
- Тест сравнивает границы, содержимое чанков и хеши обеих реализаций.

Запуск: `cargo run --release -- [--mode baseline|fused] [--iterations N] <файл-или-каталог>`

Экспериментальные данные и результаты находятся в `artifacts/`. Схема каталогов приведена ниже; `artifacts/inputs/kernels/` — локальная папка, исключённая из Git.

## Структура проекта

```text
.
├── artifacts/
│   ├── benchmarks/
│   │   ├── batch_sweep/
│   │   └── chunk_sweep/
│   ├── charts/
│   │   └── archive/
│   ├── flamegraphs/
│   │   ├── historical/
│   │   ├── iterations_1/
│   │   └── iterations_10/
│   ├── inputs/
│   │   └── kernels/ (локальная папка)
│   ├── profiles/
│   │   └── perf/
│   └── reports/
│       └── initial_study/
├── crates/
│   └── fastcdc-rs/ (submodule)
│       ├── .claude/
│       │   └── skills/
│       │       ├── pr-review/
│       │       └── regenerate-gear-tables/
│       ├── .github/
│       │   └── workflows/
│       ├── benches/
│       ├── examples/
│       ├── src/
│       │   ├── ronomon/
│       │   ├── v2016/
│       │   └── v2020/
│       └── test/
│           └── fixtures/
├── src/
│   └── analysys/
└── .vscode/
```
