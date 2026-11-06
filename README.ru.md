<div align="center">

# FastCDC + BLAKE3

Исследование конвейеров чанкирования и хеширования на Rust

[English](README.md) &nbsp; / &nbsp; **Русский**

[![CI](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Vlm326/gear-blake3-integration/actions/workflows/ci.yml)
[![Rust 2024](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows-blue)](.github/workflows/ci.yml)
[![Code license: MIT](https://img.shields.io/badge/code_license-MIT-green)](LICENSE)

[Быстрый старт](#быстрый-старт) · [Режимы](#режимы) · [Бенчмарки](#бенчмарки) · [Документация](#документация)

</div>

FastCDC + BLAKE3 — CLI для сравнения способов совместного выполнения контентно-зависимого разбиения данных и хеширования. Проект исследует влияние порядка обработки, частоты обновления BLAKE3 и SIMD-сканирования на время, число инструкций и поведение кэшей.

Корректность сравнения определяется совпадением границ чанков, их содержимого и BLAKE3-дайджестов. Проект служит исследовательским стендом; результаты зависят от входных данных, параметров, архитектуры и методики измерения.

## Быстрый старт

Требуется Rust с поддержкой редакции 2024 и Git. Все команды выполняются из корня репозитория.

```sh
git clone --recurse-submodules https://github.com/Vlm326/gear-blake3-integration.git
cd gear-blake3-integration
cargo build --release --locked

./target/release/hash_combination --mode baseline --iterations 10 README.md
./target/release/hash_combination --mode fused-base --iterations 10 README.md
```

Для существующего checkout выполните `git submodule update --init --recursive`. В Windows бинарник имеет путь `target/release/hash_combination.exe`.

Команды выше проверяют запуск на файле из репозитория. Для замеров используйте репрезентативный вход. Файл читается один раз, итерации обрабатывают буфер в памяти. CLI не печатает дайджесты: результаты вычислений используются внутри программы.

## Режимы

| Режим | Организация обработки |
| --- | --- |
| `baseline` | Сохранение всех чанков FastCDC, затем отдельный проход BLAKE3. |
| `fused-base` | Скалярный поиск границ; BLAKE3 обновляется после региона сканирования или на границе чанка. |
| `fused-windowed` | Чередование сканирования и BLAKE3 окнами заданного размера. |
| `fused-interleaved` | Проверка продвижения BLAKE3 после каждой пары байт; фиксированный порог 64 байта. |
| `fused-simd-check` | Последовательные состояния Gear и SIMD-проверка четырёх кандидатов. |
| `fused-simd-expanded` | Раскрытые формулы состояний Gear и SIMD-проверка кандидатов. |
| `fused-simd-stripes` | Параллельное сканирование участков через вендорированный `gearhash`. |

Для раздельного исследования этапов доступны `chunking-only`, `prepare-chunks` и `blake3-only`. Параметры, требования к CPU, формат карт и варианты SIMD описаны в [справочнике CLI](docs/cli.md) (English).

По умолчанию размеры min/avg/max равны 16/64/256 KiB. Они задаются переменными `MIN_CHUNK_SIZE`, `AVG_CHUNK_SIZE`, `MAX_CHUNK_SIZE`. Смысл `BLAKE3_BATCH` зависит от режима: порог обновления BLAKE3 и размер окна чередования — разные настройки эксперимента.

## Бенчмарки

### Воспроизведение

Сначала соберите release-бинарник. Для Linux нужны `perf`, `taskset` и доступ к выбранным аппаратным счётчикам. Замените вход и CPU на доступные в вашей системе.

```sh
# Linux: choose a CPU available to this process.
env LC_ALL=C BLAKE3_BATCH=8192 \
  MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 \
  perf stat -r 5 -e cycles:u,instructions:u -- \
  taskset -c 3 ./target/release/hash_combination \
  --mode fused-base --iterations 50 /path/to/input.bin
```

Повторите команду с `--mode baseline` при тех же параметрах. `perf stat -r` повторяет весь процесс, `--iterations` — обработку внутри процесса. Внешний замер включает чтение файла и аллокации; кэши между итерациями не очищаются.

Записывайте размер и происхождение входа, CPU/архитектуру, commit, локальные изменения, параметры, порядок запусков и разброс. Недоступные счётчики нельзя считать нулевыми. Все полные режимы сохраняют копирование чанков; `baseline` удерживает их все, fused-варианты освобождают по одному. Сумма изолированных этапов не обязана равняться времени `baseline`.

- [Команды perf stat и методика сравнения](docs/cache_profiling_commands.md).
- [Перебор batch и размеров чанков](scripts/benchmark_chunk_sweep.py) — требует локальный вход и `perf`, дописывает результаты.
- [Замеры на архивах ядер Linux](scripts/benchmark_kernel_cache.py) — требует Python 3.11+, GNU tar, `perf`, `taskset` и локальные деревья ядер.
- [Подготовка входов, запуск скриптов и расположение результатов](scripts/README.md).

### Опубликованные примеры

Материалы ниже закреплены на ревизии ветки `reports`. Они относятся к историческим реализациям и сохраняют прежние имена режимов; их цифры не характеризуют автоматически текущую версию CLI.

| Материал | Ссылка |
| --- | --- |
| Перебор batch | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/benchmarks/batch_sweep/summary.md) |
| Сравнение размеров чанков | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/reports/chunk_size_summary.md) |
| Исходные замеры fused | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/benchmarks/chunk_sweep/results.txt) |
| Исходные замеры baseline | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/benchmarks/chunk_sweep/results_baseline.txt) |
| График ускорения | [GitHub](https://github.com/Vlm326/gear-blake3-integration/blob/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/charts/speedup.svg) |
| Исторические флеймграфы | [GitHub](https://github.com/Vlm326/gear-blake3-integration/tree/fc8dbec80636fd3ee444d6fa3c506dce16bcbee9/artifacts/flamegraphs/historical) |

Исходные данные, профили и результаты новых запусков храните локально в `artifacts/`. Публикуемые материалы ведутся отдельно в [ветке reports](https://github.com/Vlm326/gear-blake3-integration/tree/reports).

## Документация

| Документ | Содержание |
| --- | --- |
| [Справочник CLI](docs/cli.md) | Параметры, SIMD-варианты, карты чанков и SVG-отчёты (English). |
| [Постановка исследования](docs/TZ.md) | Цели и критерии сравнения. |
| [Руководство по замерам](docs/cache_profiling_commands.md) | События perf, закрепление CPU и формулы. |
| [Скрипты](scripts/README.md) | Требования, входы и результаты серий. |
| [Правила разработки](AGENTS.md) | Структура проекта и требования к изменениям. |

## Проверки

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
python3 scripts/ci_smoke.py
```

Smoke test требует Python 3.9+, создаёт временные данные и проверяет запуск режимов и цикл карты чанков. Тесты Rust проверяют границы, содержимое и дайджесты относительно FastCDC. [CI](.github/workflows/ci.yml) проверяет Linux x86_64/aarch64 и Windows x86_64; бенчмарки выполняются отдельно.

SVG-инструмент включается feature `reports`; проверки: `cargo test --features reports --bin build-graph --locked` и `cargo clippy --all-targets --features reports --locked -- -D warnings`.

## Лицензии

Собственный код — [MIT](LICENSE); README — [CC BY-SA 4.0](LICENSES/CC-BY-SA-4.0.md). Для отчётных материалов и сырых результатов действуют отдельные области лицензирования из [LICENSES](LICENSES/). FastCDC (`crates/fastcdc-rs`) сохраняет MIT, вендорированный `gearhash` — MIT/Apache-2.0 и уведомления авторов.
