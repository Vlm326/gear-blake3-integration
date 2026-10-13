# Артефакты экспериментов

- `inputs/` — входной файл `wkiss_1.png` и дополнительные тестовые данные.
- `benchmarks/batch_sweep/` — результаты прогонов с разными BLAKE3 batch.
- `benchmarks/chunk_sweep/` — результаты по размерам чанков; отсюда `--mode build-graph` читает данные.
- `charts/speedup.svg` — актуальный график ускорения; предыдущие варианты — в `charts/archive/`.
- `flamegraphs/iterations_1/` и `flamegraphs/iterations_10/` — flamegraph для одного и десяти проходов.
- `flamegraphs/historical/` — профили предыдущих экспериментов.
- `profiles/perf/` — исходные файлы `perf.data`.
- `reports/` — сводки и отчёты экспериментов.
