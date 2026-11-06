# FastCDC + BLAKE3

[English](README.md) | [Русский](README.ru.md)

[![CI](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows-blue)](.github/workflows/ci.yml)
[![Code license: MIT](https://img.shields.io/badge/code_license-MIT-green)](LICENSE)

Rust 2024 CLI для исследования того, как совмещение чанкирования FastCDC и хеширования BLAKE3 влияет на время выполнения, число инструкций и поведение кэшей. Сравнение требует одинаковых границ, содержимого чанков и итоговых BLAKE3-хешей.

## Сборка и запуск

Нужен Rust с поддержкой редакции 2024. Все команды выполняются из корня репозитория. FastCDC подключён как Git submodule; перед сборкой инициализируй его:

```sh
git submodule update --init --recursive
cargo build --release --locked
./target/release/hash_combination --mode baseline --iterations 10 README.md
./target/release/hash_combination --mode fused-base --iterations 10 README.md
```

В Windows исполняемый файл — `target/release/hash_combination.exe`. Для демонстрации CLI команды используют файл из репозитория; для измерения производительности нужны собственные репрезентативные данные.

```text
hash_combination [--mode MODE] [--iterations N] <file-or-directory> ...
```

По умолчанию используются `baseline` и одна итерация. Каталоги обходятся рекурсивно. Каждый файл читается один раз, затем на каждой итерации обрабатывается из памяти.

## Режимы

| Режим | Поведение |
| --- | --- |
| `baseline` | Собирает чанки FastCDC, затем отдельно хеширует каждый чанк. |
| `fused-base` | Скалярный поиск границ; обновление BLAKE3 после региона или на границе чанка. |
| `fused-windowed` | Экспериментальное чередование окон Gear/BLAKE3, ограниченных `BLAKE3_BATCH`. |
| `fused-simd-check` | Экспериментальный AVX2-сканер проверяет четыре кандидата вместе; Gear-состояние обновляется скалярно. |
| `fused-simd-expanded` | Раскрытые формулы Gear-состояний и проверка кандидатов через AVX2. |
| `fused-simd-stripes` | Параллельное сканирование участков через вендорированный `gearhash`. |
| `fused-interleaved` | Исторический interleaved-вариант проверяет продвижение BLAKE3 после каждого кандидата Gear и обновляет хеш порциями по 64 байта. |
| `chunking-only` | FastCDC с аллокациями и копированием чанков baseline, без хеширования. |
| `prepare-chunks` | Сохраняет границы FastCDC для одного входного файла. |
| `blake3-only` | Хеширует срезы исходного буфера по сохранённой карте чанков. |

`fused-simd-check` и `fused-simd-expanded` проверяют AVX2 во время выполнения и имеют скалярный fallback. `fused-simd-stripes` требует AVX2 на x86_64 или NEON на aarch64. Глобальные флаги `target-feature` не нужны.

Размеры чанков задаются в байтах через `MIN_CHUNK_SIZE`, `AVG_CHUNK_SIZE` и `MAX_CHUNK_SIZE`; значения по умолчанию — 16384, 65536 и 262144. Используй допустимые чётные размеры FastCDC с `min <= avg <= max`. `BLAKE3_BATCH` управляет пакетной обработкой в реализациях, которые его используют; смысл параметра различается между режимами.

Окно `fused-windowed` по умолчанию равно 8192 байтам; batch должен быть положительным и чётным. Копирование чанков и сдвиги потокового буфера сохранены. Настройки читаются один раз на поток, а в прежнем fused-base batch читается для каждого чанка. Уменьшение batch в `fused-base` не эквивалентно ограничению окон в `fused-windowed`.

Экспериментальные параметры AVX2 для `fused-simd-stripes`:

- `--unroll 1|2|4|8`: выбор развёртки; `1` включает исходный сканер.
- `--scanner original|control|expanded|grouped-check|combined`: сравнение последовательных вычислений, развёртки, общей проверки границ и сочетания изменений.
- `--strip-size 128|256|512|1024 --strips 2|4`: размер и число участков задаются вместе; их произведение — размер SIMD-окна (исходно 256 × 4 байта).

Эти три группы параметров взаимно исключаются и требуют AVX2. Без них архитектура выбирается автоматически. Короткие срезы обрабатываются скалярным хвостом, поэтому эксперименты с окном сканера зависят и от batch. В `fused-simd-stripes` порог BLAKE3 batch проверяется после полных регионов FastCDC, остаток хешируется на границе чанка; окна сканера не вызывают промежуточных обновлений BLAKE3. При сравнении режимов нужно учитывать различия в аллокациях и копировании.

## Раздельное измерение чанкирования и хеширования

Выбери свободный путь карты: существующая карта не перезаписывается.

```sh
./target/release/hash_combination --mode prepare-chunks --chunk-map /tmp/readme.chunks.tsv README.md
./target/release/hash_combination --mode chunking-only --iterations 10 README.md
./target/release/hash_combination --mode blake3-only --chunk-map /tmp/readme.chunks.tsv --iterations 10 README.md
```

В Windows замени `/tmp/readme.chunks.tsv` на доступный локальный путь. `prepare-chunks` и `blake3-only` требуют ровно один обычный файл и `--chunk-map`. Подготовка допускает только одну итерацию.

Карта — TSV: заголовок `# hash_combination chunk-map v1`, строки `file_size<TAB>байты`, `chunk_sizes<TAB>min<TAB>avg<TAB>max`, затем пары `смещение<TAB>длина`. При загрузке проверяются размер файла, настройки и полное непрерывное покрытие границами. Изменение содержимого при прежнем размере не обнаруживается. Пересоздавай карту при изменении входа или настроек чанкирования. Переменные размеров чанков и batch не влияют на `blake3-only`.

`chunking-only` сохраняет аллокации baseline, а `blake3-only` хеширует срезы исходного буфера. Сумма их времён не обязана равняться времени baseline.

## Измерения и построение графиков

См. [команды perf](docs/cache_profiling_commands.md), [скрипты замеров](scripts/README.md) и [задание исследования](docs/TZ.md). Сначала собери проект, затем запускай `perf` непосредственно над бинарником. Фиксируй вход, машину, коммит и локальные изменения, настройки, итерации, повторы, CPU и поддерживаемые события. Внешний `perf stat` включает загрузку и аллокации; кэши между итерациями не очищаются.

В `main` находятся исходники, инструкции и скрипты. Отчёты, презентации, результаты бенчмарков, графики, флеймграфы, профили и входные наборы данных в эту ветку не включаются. Опубликованные материалы исследования хранятся отдельно в Git-ветке `reports`; они могут относиться к более раннему эксперименту или версии кода.

Локальные запуски могут создавать `artifacts/`; в `main` весь каталог исключён из Git. В свежем checkout его нет. Инструкции скриптов описывают необходимые входы и подготовку каталогов вывода. `.local-research/` и `kernels/` также локальны и исключены из Git.

Отдельная утилита графиков остаётся доступной:

```sh
cargo run --release --locked --features reports --bin build-graph
```

Она не принимает аргументов. Сначала подготовь локальные файлы замеров `results.txt` и `results_baseline.txt` в `artifacts/benchmarks/chunk_sweep/`; результат — `artifacts/charts/speedup.svg`. Скрипт перебора batch пишет в другой каталог, `artifacts/benchmarks/batch_sweep/`: перед построением графика скопируй выбранную сопоставимую пару в `chunk_sweep/`. Эти входы не поставляются в `main`. Plotters подключается только feature `reports` и использует SVG-backend без Fontconfig и FreeType. Cargo feature и Git-ветка с именем `reports` — разные вещи.

## Проверки и CI

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
python3 scripts/ci_smoke.py
cargo test --features reports --bin build-graph --locked
cargo clippy --all-targets --features reports --locked -- -D warnings
```

Smoke test требует Python 3.9+ и release-сборку, создаёт временные данные и проверяет выполнение CLI и цикл карты чанков. Rust-тесты сравнивают границы, содержимое и хеши чанков, проверяют карты и не требуют локальных входов.

[CI](.github/workflows/ci.yml) запускается при push в `main`, для pull request и вручную. Он проверяет форматирование, строгий Clippy, release-сборку, тесты и CLI на Linux x86_64, Linux aarch64 и Windows x86_64. Утилита отчётов проверяется отдельно на Linux. CI не запускает бенчмарки и не создаёт исследовательские артефакты.

## Структура репозитория

Отслеживаемые каталоги `main` (генерируемые и локальные файлы опущены):

```text
.
├── README.md                  # Английский
├── README.ru.md               # Русский
├── AGENTS.md                  # Инструкции для работы с репозиторием
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── LICENSES/                  # Области действия лицензий CC
├── .github/workflows/ci.yml
├── .vscode/                   # Настройки редактора
├── crates/
│   ├── fastcdc-rs/            # Git submodule
│   └── gearhash/              # Вендорированная библиотека
├── docs/
│   ├── TZ.md
│   └── cache_profiling_commands.md
├── scripts/                   # Замеры и smoke test CLI
│   ├── README.md
│   └── drafts/                # Незавершённые неподдерживаемые скрипты
└── src/
    ├── main.rs                # CLI и загрузка входов
    ├── bin/build-graph.rs      # Дополнительная утилита графиков
    └── analysys/              # Разбор результатов и вывод SVG
```

## Компоненты и лицензии

Собственный исходный код проекта распространяется по [MIT](LICENSE). FastCDC использует submodule [`fastcdc-rs`](https://github.com/nlfiedler/fastcdc-rs) Nathan Fiedler под MIT. Вендорированная копия [`gearhash`](https://github.com/srijs/rust-gearhash) 0.1.4 сохраняет лицензии MIT и Apache-2.0 и уведомления об авторстве.

Обе языковые версии README распространяются по [CC BY-SA 4.0](LICENSES/CC-BY-SA-4.0.md). Прежние области действия лицензий для сводок, отчётов и визуализаций ([CC BY-SA 4.0](LICENSES/CC-BY-SA-4.0.md)), сырых результатов бенчмарков и профилей ([CC BY 4.0](LICENSES/CC-BY-4.0.md)) сохраняются при отдельном хранении этих материалов. Сторонний код и материалы сохраняют собственные лицензии.

Переименования: `fused` → `fused-base`, `historical` → `fused-interleaved`, `simd` → `fused-simd-check`, `simd-bad` → `fused-simd-expanded`, `gearhash-simd` → `fused-simd-stripes`. Служебные режимы теперь пишутся через дефис. `BLAKE3_BATCH` сохраняет прежнее поведение, зависящее от режима.
