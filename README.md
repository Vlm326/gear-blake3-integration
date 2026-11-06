# FastCDC + BLAKE3

[![CI](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml)

Исследуется совмещение разбиения данных на чанки FastCDC с вычислением хешей BLAKE3. Основные режимы сравнения:

- `baseline`: сначала формирует чанки FastCDC, затем отдельно хеширует каждый чанк;
- `fused-base`: вычисляет BLAKE3 одновременно с поиском границ чанков;
- `fused-simd-check`: AVX2 одновременно проверяет четыре кандидата, Gear-состояние между блоками вычисляется скалярно;
- `fused-simd-expanded`: экспериментальный вариант, который хранит Gear-состояние в YMM и оказался медленнее в текущем замере;
- `fused-simd-stripes`: экспериментальная реализация на локальной копии `gearhash`, требует AVX2 на x86_64 или NEON на aarch64;
- `fused-interleaved`: исторический interleaved-вариант из коммита `194611c`, проверяющий
  продвижение BLAKE3 после каждого кандидата Gear и обновляющий хешер порциями
  по 64 байта.

Для раздельного измерения этапов добавлены:

- `chunking-only`: FastCDC и сохранение всех чанков, как в baseline, без BLAKE3;
- `blake3-only`: BLAKE3 каждого среза входа по заранее сохранённым границам, без FastCDC и копирования данных в отдельные чанки;
- `prepare-chunks`: однократное сохранение границ FastCDC для последующего запуска `blake3-only`.

Тесты сравнивают границы, содержимое чанков и хеши реализаций и проверяют загрузку карт границ.

## Непрерывная интеграция

GitHub Actions запускает CI при каждом push в `main` и для каждого pull request;
также доступен ручной запуск. Push в `reports` отдельно не запускает CI.
Проверяются форматирование (`cargo fmt --all -- --check`), Clippy со строгой
обработкой предупреждений (`cargo clippy --all-targets --locked -- -D warnings`),
release-сборка и тесты (`cargo build --release --locked`, `cargo test --release --locked`).
Сборка, тесты и smoke test CLI выполняются в Linux x86_64, Linux aarch64
и Windows x86_64. Подмодуль FastCDC загружается рекурсивно, зависимости кешируются.

`python scripts/ci_smoke.py` после release-сборки проверяет все режимы обработки
на 8 МиБ воспроизводимых псевдослучайных данных, включая
цикл `prepare-chunks` → `blake3-only`. Нужен Python 3.9 или новее;
запускать из корня репозитория. Данные и карта создаются во временном каталоге
и удаляются после проверки. Тесты Rust также генерируют собственные данные
и не требуют `artifacts/inputs/`.

`fused-simd-check` и `fused-simd-expanded` выбирают AVX2 во время выполнения и используют скалярную
реализацию при его отсутствии, в том числе на aarch64. `fused-simd-stripes` требует
AVX2 на x86_64 или NEON на aarch64. Архитектурные ветки защищены `#[cfg]`;
глобальные `RUSTFLAGS` с `target-feature` не нужны.

CI не собирает показатели производительности, не запускает скрипты бенчмарков
и не создаёт графики, флеймграфы или профили. Утилита `build-graph` в smoke test
не включена: он читает результаты замеров и создаёт артефакты. Измерения
производительности по-прежнему выполняются локально с `perf`.

## Запуск

Обработка файла или всех файлов в каталоге:

```sh
cargo run --release -- [--mode baseline|fused-base|fused-simd-check|fused-simd-expanded|fused-simd-stripes|fused-interleaved|chunking-only] [--iterations N] <файл-или-каталог>
```

По умолчанию используется режим `baseline` и одна итерация. Например:

```sh
cargo run --release -- --mode baseline --iterations 10 ./путь/к/данным
cargo run --release -- --mode fused-base --iterations 10 ./путь/к/данным
cargo run --release -- --mode fused-simd-check --iterations 10 ./путь/к/данным
cargo run --release -- --mode fused-simd-expanded --iterations 10 ./путь/к/данным
cargo run --release -- --mode fused-simd-stripes --iterations 10 ./путь/к/данным
cargo run --release -- --mode fused-interleaved --iterations 10 ./путь/к/данным
```

Каждый файл загружается в память один раз; `--iterations N` повторно обрабатывает эти данные N раз.

## Раздельное измерение FastCDC и BLAKE3

Сначала один раз сохрани границы для конкретного входа. Для сопоставимости задай
те же параметры чанков, что использованы в baseline/fused:

```sh
env MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 ./target/release/hash_combination --mode prepare-chunks --chunk-map /tmp/base_photo.chunks.tsv artifacts/inputs/base_photo.png
```

Затем запускай этапы отдельно:

```sh
env MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 ./target/release/hash_combination --mode chunking-only --iterations 50 artifacts/inputs/base_photo.png
./target/release/hash_combination --mode blake3-only --chunk-map /tmp/base_photo.chunks.tsv --iterations 50 artifacts/inputs/base_photo.png
```

`chunking-only` поддерживает файлы и каталоги. `prepare-chunks` и `blake3-only`
принимают ровно один обычный файл и требуют `--chunk-map`; подготовка допускает
только одну итерацию. Существующая карта не перезаписывается: для новой серии
выбери новый путь или заранее удали старую карту.

Формат карты — текстовый TSV: строка версии `# hash_combination chunk-map v1`,
строка `file_size<TAB>байты`, строка `chunk_sizes<TAB>min<TAB>avg<TAB>max`,
далее пары `смещение<TAB>длина`. Для пустого входа пар нет. При загрузке проверяются
размер файла, корректность настроек и полное непрерывное покрытие входа границами.

`blake3-only` использует границы из карты: переменные размеров чанков и
`BLAKE3_BATCH` на него не влияют. Пересоздавай карту при изменении входа или
настроек FastCDC. Изменение содержимого при прежнем размере автоматически
не обнаруживается: проверка полным хешем сама добавила бы хеширование в замер.
Не добавляй карты локальных входных данных в Git.

Обычный `perf stat` измеряет весь процесс, включая однократную загрузку входа
и карты, но FastCDC в `blake3-only` вообще не вызывается. Большое число итераций
уменьшает долю этой подготовки. `chunking-only` сохраняет аллокации baseline,
а `blake3-only` хеширует срезы исходного буфера: разности времени этапов служат
ориентиром, их сумма не обязана совпадать со временем baseline из-за различий
в размещении данных и состоянии кэша.

Самостоятельные команды `perf stat` для всех четырёх режимов находятся в
[docs/cache_profiling_commands.md](docs/cache_profiling_commands.md).

## Построение графика

Отдельная утилита `build-graph` сравнивает замеры `fused-base` и `baseline` для одинаковых конфигураций batch и размеров чанков. Он читает `artifacts/benchmarks/chunk_sweep/results.txt` и `results_baseline.txt`, вычисляет ускорение по медианному времени замеров и создаёт SVG-график `artifacts/charts/speedup.svg`:

```sh
cargo run --release --features reports --bin build-graph
```

Утилита не принимает аргументов. Обычные `cargo build`, `cargo test` и
`cargo run` собирают только CLI чанкирования и хеширования: Plotters и анализ
замеров подключаются исключительно через feature `reports` и отдельный бинарник.
Для графика используется только SVG-backend Plotters без системных зависимостей
Fontconfig и FreeType; текст SVG отображается шрифтами программы просмотра.
Для проверки инструмента отчётов: `cargo test --features reports --bin build-graph`. Формат и примеры экспериментальных данных находятся в `artifacts/`.

## Скрипты и материалы

- [Скрипты замеров и профилирование](scripts/README.md) — запуск sweep и сохранение локальных профилей.
- [Задание исследования](docs/TZ.md) и [заметки по контрольным замерам](docs/explanation.html).
- [Материалы выступления 15.10.2026](artifacts/presentations/2026-10-15/README.md) — презентации, PDF, текст выступления и флеймграфы для показа.

## Используемые компоненты

Для FastCDC используется submodule [`fastcdc-rs`](https://github.com/nlfiedler/fastcdc-rs), автор — Nathan Fiedler; лицензия — MIT. Уведомления об авторстве и файл лицензии сохранены в submodule `crates/fastcdc-rs`.

Экспериментальный режим `fused-simd-stripes` использует локальную копию `gearhash` 0.1.4 из `crates/gearhash/`. Исходный проект — [`srijs/rust-gearhash`](https://github.com/srijs/rust-gearhash); вендорированная копия сохраняет лицензии MIT и Apache-2.0 и уведомления об авторстве.

## Лицензии

- Собственный исходный код проекта распространяется по MIT; текст лицензии — в [`LICENSE`](LICENSE). В Cargo-метаданных проекта также указано `MIT`.
- Тексты этого README, `artifacts/README.md` и отчётов в `artifacts/reports/`, сводки `summary.md` в каталогах замеров и созданные в проекте графики в `artifacts/charts/` и `artifacts/flamegraphs/` распространяются по CC BY-SA 4.0.
- Сырые результаты измерений в `artifacts/benchmarks/` (кроме сводок `summary.md`) и профили в `artifacts/profiles/` распространяются по CC BY 4.0.
- Submodule, зависимости и любые сторонние материалы сохраняют собственные лицензии и уведомления об авторстве; лицензии этого проекта их не заменяют.

Область действия лицензий CC, их SPDX-идентификаторы и ссылки на официальные условия приведены в `LICENSES/`.

Экспериментальные данные и результаты находятся в `artifacts/`. Локальные входные данные, включая `artifacts/inputs/wkiss_1.png` и `artifacts/inputs/kernels/`, не включаются в Git.

## Структура проекта

```text
.
├── artifacts/
│   ├── benchmarks/
│   │   ├── batch_sweep/
│   │   ├── chunk_sweep/
│   │   └── wkiss_50/
│   ├── charts/
│   │   └── archive/
│   ├── flamegraphs/
│   │   ├── historical/
│   │   ├── iterations_1/
│   │   ├── iterations_10/
│   │   └── photo_50_times/
│   ├── inputs/
│   │   └── (локальные входные данные, исключённые из Git)
│   ├── presentations/
│   │   └── 2026-10-15/
│   │       └── flamegraphs/
│   ├── profiles/
│   │   ├── local/ (локальные профили, исключённые из Git)
│   │   └── perf/ (сохранённые профили экспериментов)
│   └── reports/ (только в ветке reports)
│       └── initial_study/
├── crates/
│   └── fastcdc-rs/ (submodule, MIT, Nathan Fiedler)
├── docs/
│   ├── TZ.md
│   └── explanation.html
├── scripts/
│   ├── benchmark_chunk_sweep.py
│   └── drafts/
├── src/
│   └── analysys/
└── .vscode/
```

Папка `artifacts/reports/` находится только в ветке `reports`. Изменения структуры из `main` следует переносить в `reports` отдельно.

После уборки `artifacts/` содержит итоговые материалы. Сырые замеры и локальные входы сохранены вне него в игнорируемом `.local-research/2026-10-29-artifacts/`; для повторного запуска входы нужно восстановить в `artifacts/inputs/`.

## Имена экспериментальных режимов

Режимы организованы в семейство `fused-*`: `fused-base` — скалярная основа, `fused-interleaved` — частое чередование с BLAKE3, `fused-windowed` — чередование окнами, `fused-simd-check` — SIMD-проверка кандидатов, `fused-simd-expanded` — раскрытые формулы состояний, `fused-simd-stripes` — параллельное сканирование участков.

Для `fused-windowed` переменная `BLAKE3_BATCH` задаёт положительное чётное окно (по умолчанию 8192). Для остальных режимов прежний смысл переменной сохранён.

В `fused-simd-stripes` доступны взаимоисключающие группы настроек AVX2:

- `--scanner original|control|expanded|grouped-check|combined` — исходный сканер, контроль, раскрытие зависимостей, групповая проверка и сочетание изменений.
- `--unroll 1|2|4|8` — число байт на шаг каждого участка.
- `--strip-size 128|256|512|1024 --strips 2|4` — геометрия параллельных участков.

Переименования CLI несовместимы со старыми командами; парсер отчётов принимает обе метки `fused` и `fused-base`.
