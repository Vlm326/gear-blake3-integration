# Артефакты экспериментов

- `inputs/` — локальные входные файлы; эта папка не публикуется в Git.
- `benchmarks/batch_sweep/` — результаты прогонов с разными BLAKE3 batch.
- `benchmarks/chunk_sweep/` — результаты по размерам чанков; отсюда `--mode build-graph` читает данные.
- `benchmarks/wkiss_50/` — повторные замеры и sweep для тестового изображения за 50 итераций.
- `charts/speedup.svg` — актуальный график ускорения; предыдущие варианты — в `charts/archive/`.
- `charts/speedup_4_64_r5.svg` — график повторного sweep 4/16/64 с пятью повторами.
- `flamegraphs/iterations_1/` и `flamegraphs/iterations_10/` — flamegraph для одного и десяти проходов.
- `flamegraphs/photo_50_times/` — профили baseline, fused и historical для 50 обработок изображения.
- `флеймграфы_для_показа/` — растровые и SVG-копии flamegraph для презентации.
- `flamegraphs/historical/` — профили предыдущих экспериментов.
- `profiles/perf/` — исходные файлы `perf.data`.
- `profiles/perf/*_wkiss_50.data` — исходные профили для серии `benchmarks/wkiss_50/`.
- `reports/` — сводки и отчёты экспериментов.

Каталог `reports/` присутствует только в ветке `reports`; остальное содержимое проекта синхронизировано между ветками `main` и `reports`.

Лицензирование материалов:

- тексты в `reports/`, файлы `summary.md` в каталогах замеров, графики в `charts/` и flamegraph-визуализации в `flamegraphs/` — CC BY-SA 4.0;
- исходные результаты замеров в `benchmarks/` (кроме `summary.md`) и файлы профилирования в `profiles/` — CC BY 4.0.

SPDX-идентификаторы и ссылки на официальные условия CC BY-SA 4.0 и CC BY 4.0 приведены в `../LICENSES/`.
