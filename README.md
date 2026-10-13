# FastCDC + BLAKE3

Исследуется совмещение разбиения данных на чанки FastCDC с вычислением хешей BLAKE3.

- `baseline`: сначала формирует чанки FastCDC, затем отдельно хеширует каждый чанк.
- `fused`: вычисляет BLAKE3 одновременно с поиском границ чанков.
- Тест сравнивает границы, содержимое чанков и хеши обеих реализаций.

Запуск: `cargo run --release -- [--mode baseline|fused] [--iterations N] <файл-или-каталог>`

`main` содержит код проекта. Экспериментальные данные, графики и отчёты находятся в ветке `reports`.

## Структура кода в `main`

```text
.
├── crates/
│   └── fastcdc-rs/ (submodule)
├── src/
│   └── analysys/
└── .vscode/
```

## Структура данных в ветке `reports`

```text
artifacts/
├── benchmarks/
│   ├── batch_sweep/
│   └── chunk_sweep/
├── charts/
│   └── archive/
├── flamegraphs/
│   ├── historical/
│   ├── iterations_1/
│   └── iterations_10/
├── inputs/
├── profiles/
│   └── perf/
└── reports/
    └── initial_study/
```
