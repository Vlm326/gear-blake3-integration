# FastCDC + BLAKE3

Исследуется совмещение разбиения данных на чанки FastCDC с вычислением хешей BLAKE3. В проекте сравниваются два режима:

- `baseline`: сначала формирует чанки FastCDC, затем отдельно хеширует каждый чанк;
- `fused`: вычисляет BLAKE3 одновременно с поиском границ чанков.

Тесты сравнивают границы, содержимое чанков и хеши обеих реализаций.

## Запуск

Обработка файла или всех файлов в каталоге:

```sh
cargo run --release -- [--mode baseline|fused] [--iterations N] <файл-или-каталог>
```

По умолчанию используется режим `baseline` и одна итерация. Например:

```sh
cargo run --release -- --mode baseline --iterations 10 ./путь/к/данным
cargo run --release -- --mode fused --iterations 10 ./путь/к/данным
```

Каждый файл загружается в память один раз; `--iterations N` повторно обрабатывает эти данные N раз.

## Построение графика

Режим `build-graph` сравнивает замеры `fused` и `baseline` для одинаковых конфигураций batch и размеров чанков. Он читает `artifacts/benchmarks/chunk_sweep/results.txt` и `results_baseline.txt`, вычисляет ускорение по медианному времени замеров и создаёт SVG-график `artifacts/charts/speedup.svg`:

```sh
cargo run --release -- --mode build-graph
```

В этом режиме входной файл или каталог не указывается. Формат и примеры экспериментальных данных находятся в `artifacts/`.

## Используемые компоненты

Для FastCDC используется submodule [`fastcdc-rs`](https://github.com/nlfiedler/fastcdc-rs), автор — Nathan Fiedler; лицензия — MIT. Уведомления об авторстве и файл лицензии сохранены в submodule `crates/fastcdc-rs`.

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
│   │   └── chunk_sweep/
│   ├── charts/
│   │   └── archive/
│   ├── flamegraphs/
│   │   ├── historical/
│   │   ├── iterations_1/
│   │   └── iterations_10/
│   ├── inputs/
│   │   └── (локальные входные данные, исключённые из Git)
│   ├── profiles/
│   │   └── perf/
│   └── reports/ (только в ветке reports)
│       └── initial_study/
├── crates/
│   └── fastcdc-rs/ (submodule, MIT, Nathan Fiedler)
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

Папка `artifacts/reports/` находится только в ветке `reports`; остальной код и содержимое проекта синхронизируются между ветками `main` и `reports`.
