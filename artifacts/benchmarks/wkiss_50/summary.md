# Тестовое изображение, 50 итераций

Вход: тестовое изображение (22 022 007 байт). Сборка `release`, один и тот
же бинарник для всех режимов. Размеры чанков по умолчанию: min/avg/max =
16384/65536/262144. `perf stat -r 3`, приведено среднее.

| Режим | Время, с | Cycles | Instructions | Cache references | Cache misses |
|---|---:|---:|---:|---:|---:|
| baseline | 2,581 | 2 948 531 446 | 8 185 206 603 | 264 322 728 | 26 950 326 |
| fused | 1,348 | 2 523 976 138 | 8 235 315 039 | 258 283 195 | 15 931 419 |
| historical | 3,165 | 6 093 935 690 | 18 500 643 056 | 252 549 657 | 15 099 419 |

Замечание: абсолютные времена зависят от состояния частот CPU между сессиями.
Сравнивать имеет смысл отношения внутри одной серии замеров.

## Повторный замер `perf stat -r 5`

Повторный парный замер выполнен на тестовом изображении (22 022 007 байт), тем же release-бинарником, по 50 итераций на запуск и с параметрами чанков по умолчанию (16/64/256 KiB). Для fused не задавался `BLAKE3_BATCH`, поэтому использовалось значение по умолчанию — 64 байта. `perf stat -r 5` усредняет пять запусков каждого режима.

| Режим | Время, с | Cycles | Instructions | Cache references | Cache misses |
|---|---:|---:|---:|---:|---:|
| baseline | 2,892 ± 0,219 | 2 872 240 650 | 8 185 230 881 | 285 008 623 | 24 221 813 |
| fused | 1,588 ± 0,125 | 2 535 810 007 | 8 236 772 883 | 278 589 859 | 14 243 814 |

В этой серии instructions практически одинаковы: у fused на 0,63% больше. Cache-misses у fused меньше на 41,2%, cycles — на 11,7%. Среднее wall-clock время fused в 1,82 раза меньше, но разброс времени между повторами около 8%, поэтому для слайда главное сопоставление аппаратных счётчиков, а не точная величина ускорения по времени.

Полные выводы последней серии `perf stat`:

- `perf-stat-r5-baseline-c.txt`
- `perf-stat-r5-fused-c.txt`

Они содержат средние значения, разброс по пяти запускам и время. Пара файлов без суффикса `-c` — отдельный повторный прогон с теми же параметрами, сохранённый в CSV-формате.

## Топ символов (`perf report`, доля сэмплов)

| Режим | Символ | Доля |
|---|---|---:|
| baseline | `_blake3_hash_many_avx2` | 37,26% |
| baseline | `fastcdc::v2020::scan_region` | 35,86% |
| baseline | `__memmove_avx_unaligned_erms` | 16,00% |
| fused | `fused_stream_cdc::scan_region` | 44,41% |
| fused | `_blake3_hash_many_avx2` | 34,31% |
| fused | `__memmove_avx_unaligned_erms` | 17,60% |
| historical | `_blake3_compress_in_place_sse41` | 58,27% |
| historical | `scan_region_with_interleaved_blake3` | 23,06% |
| historical | `__memmove_avx_unaligned_erms` | 9,97% |
| historical | `blake3::ChunkState::update` | 2,70% |

Наблюдение: `baseline` и `fused` тратят основное время в AVX2-пути BLAKE3
(`_blake3_hash_many_avx2`). Исторический interleaved-режим с обновлением порциями
по 64 байта уходит на одиночный `_blake3_compress_in_place_sse41`, что совпадает с
ростом числа инструкций примерно в 2,25 раза относительно baseline и fused.

## Артефакты

- Flamegraph (palette `rust`): `artifacts/flamegraphs/wkiss_50/{baseline,fused,historical}_rust.svg`
- Flamegraph (palette `hot`): `artifacts/flamegraphs/wkiss_50/{baseline,fused,historical}_hot.svg`
- Профили: `artifacts/profiles/perf/{baseline,fused,historical}_wkiss_50.data`
