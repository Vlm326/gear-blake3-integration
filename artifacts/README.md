# Артефакты экспериментов

- `inputs/` — локальные входные файлы; эта папка не публикуется в Git.
- `benchmarks/batch_sweep/` — результаты прогонов с разными BLAKE3 batch.
- `benchmarks/chunk_sweep/` — результаты по размерам чанков; отсюда `--mode build-graph` читает данные.
- `charts/speedup.svg` — актуальный график ускорения; предыдущие варианты — в `charts/archive/`.
- `flamegraphs/iterations_1/` и `flamegraphs/iterations_10/` — flamegraph для одного и десяти проходов.
- `flamegraphs/historical/` — профили предыдущих экспериментов.
- `profiles/perf/` — исходные файлы `perf.data`.
- `reports/` — сводки и отчёты экспериментов.

Каталог `reports/` присутствует только в ветке `reports`; остальное содержимое проекта синхронизировано между ветками `main` и `reports`.

Лицензирование материалов:

- тексты в `reports/`, файлы `summary.md` в каталогах замеров, графики в `charts/` и flamegraph-визуализации в `flamegraphs/` — CC BY-SA 4.0;
- исходные результаты замеров в `benchmarks/` (кроме `summary.md`) и файлы профилирования в `profiles/` — CC BY 4.0.

SPDX-идентификаторы и ссылки на официальные условия CC BY-SA 4.0 и CC BY 4.0 приведены в `../LICENSES/`.
