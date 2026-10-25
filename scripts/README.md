# Скрипты замеров

Все команды ниже выполняются из корня репозитория. Для скриптов достаточно стандартной библиотеки Python 3.9 или новее.

## Перебор batch и размеров чанков

Перед запуском собрать release-бинарник и подготовить локальный входной файл `artifacts/inputs/wkiss_1.png`. В системе должен быть установлен `perf` и разрешён доступ к используемым аппаратным счётчикам.

```sh
cargo build --release
python3 scripts/benchmark_chunk_sweep.py
```

Скрипт перебирает заданные в нём размеры чанков и BLAKE3 batch. Для каждой конфигурации выполняется прогрев, затем `perf stat -r 5` для 50 обработок файла. Порядок baseline и fused чередуется между конфигурациями.

Результаты **дописываются** в `artifacts/benchmarks/batch_sweep/results.txt` и `results_baseline.txt`. Перед новой серией следует сохранить предыдущие файлы отдельно, чтобы не смешивать условия измерений. Эти два рабочих файла исключены из Git; существующие архивные серии остаются на месте.

## Локальные профили

Временные профили сохранять в игнорируемый каталог `artifacts/profiles/local/`. Для каждой новой серии выбирать отдельный подкаталог; пример для серии `my-run`:

```sh
mkdir -p artifacts/profiles/local/my-run
perf record -o artifacts/profiles/local/my-run/perf.data -- \
  ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/wkiss_1.png
valgrind --tool=cachegrind \
  --cachegrind-out-file=artifacts/profiles/local/my-run/cachegrind.out.%p \
  ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/wkiss_1.png
```

Для этих команд нужны соответственно `perf` и Valgrind. Сохраняемые для публикации профили экспериментов находятся отдельно в `artifacts/profiles/perf/`.

## Черновики

`drafts/cache_run.py` — незавершённый черновик, сохранённый без изменений. В нём отсутствуют необходимые импорты и определения переменных; он не является готовой командой запуска. Его исправление требует отдельной работы.
