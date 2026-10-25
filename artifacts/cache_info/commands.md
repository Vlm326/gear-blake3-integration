# Команды для сравнения baseline и fused

Вставляй команды по одной из корня репозитория. Каждая команда самостоятельная.
Параметры одинаковые: CPU 3, 50 итераций, 5 повторов. AMD-события проверены на Ryzen 7 7730U.
Результаты пишутся в отдельные файлы; повторный запуск перезаписывает соответствующий файл.

## Подготовка

```bash
cargo build --release
```

```bash
mkdir -p artifacts/cache_info
```

# Fused

## Общие метрики

Такты CPU, выполненные инструкции и generic-события обращений/промахов кэша. Последние зависят от CPU и не являются суммой всех уровней кэша.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,instructions:u,cache-references:u,cache-misses:u}' -o artifacts/cache_info/fused_basic.txt -- taskset -c 3 ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/base_photo.png
```

## Кэш данных L1

Обращения чтения и промахи L1 по generic-событиям perf. Можно сравнивать промахи/MiB и, с учётом аппаратного отображения, misses/loads.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{L1-dcache-loads:u,L1-dcache-load-misses:u}' -o artifacts/cache_info/fused_l1.txt -- taskset -c 3 ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/base_photo.png
```

## TLB данных

Generic-события обращений и промахов TLB — кэша трансляций адресов. Сравнивай промахи/MiB; смысл loads зависит от CPU.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{dTLB-loads:u,dTLB-load-misses:u}' -o artifacts/cache_info/fused_dtlb.txt -- taskset -c 3 ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/base_photo.png
```

## Кэш L2

Промахи L2 после промахов кэша данных L1 и циклы с незавершёнными запросами из L2. l2_fill_busy/cycles — доля циклов с запросами в полёте, а не доля простоя CPU.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,l2_cache_misses_from_dc_misses:u,l2_fill_pending.l2_fill_busy:u}' -o artifacts/cache_info/fused_l2.txt -- taskset -c 3 ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/base_photo.png
```

## Общий кэш L3

Запросы и промахи общего L3. Из-за -a счётчики включают фон всей системы; taskset закрепляет только программу. Отношение misses/requests характеризует общий L3 при согласованном охвате событий. env после sudo сохраняет параметры чанков и batch.

```bash
sudo env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -a -r 5 -e '{l3_lookup_state.all_l3_req_typs,l3_comb_clstr_state.request_miss}' -o artifacts/cache_info/fused_l3.txt -- taskset -c 3 ./target/release/hash_combination --mode fused --iterations 50 artifacts/inputs/base_photo.png
```

# Baseline

## Общие метрики

Такты CPU, выполненные инструкции и generic-события обращений/промахов кэша. Последние зависят от CPU и не являются суммой всех уровней кэша.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,instructions:u,cache-references:u,cache-misses:u}' -o artifacts/cache_info/baseline_basic.txt -- taskset -c 3 ./target/release/hash_combination --mode baseline --iterations 50 artifacts/inputs/base_photo.png
```

## Кэш данных L1

Обращения чтения и промахи L1 по generic-событиям perf. Можно сравнивать промахи/MiB и, с учётом аппаратного отображения, misses/loads.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{L1-dcache-loads:u,L1-dcache-load-misses:u}' -o artifacts/cache_info/baseline_l1.txt -- taskset -c 3 ./target/release/hash_combination --mode baseline --iterations 50 artifacts/inputs/base_photo.png
```

## TLB данных

Generic-события обращений и промахов TLB — кэша трансляций адресов. Сравнивай промахи/MiB; смысл loads зависит от CPU.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{dTLB-loads:u,dTLB-load-misses:u}' -o artifacts/cache_info/baseline_dtlb.txt -- taskset -c 3 ./target/release/hash_combination --mode baseline --iterations 50 artifacts/inputs/base_photo.png
```

## Кэш L2

Промахи L2 после промахов кэша данных L1 и циклы с незавершёнными запросами из L2. l2_fill_busy/cycles — доля циклов с запросами в полёте, а не доля простоя CPU.

```bash
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,l2_cache_misses_from_dc_misses:u,l2_fill_pending.l2_fill_busy:u}' -o artifacts/cache_info/baseline_l2.txt -- taskset -c 3 ./target/release/hash_combination --mode baseline --iterations 50 artifacts/inputs/base_photo.png
```

## Общий кэш L3

Запросы и промахи общего L3. Из-за -a счётчики включают фон всей системы; taskset закрепляет только программу. Отношение misses/requests характеризует общий L3 при согласованном охвате событий. env после sudo сохраняет параметры чанков и batch.

```bash
sudo env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -a -r 5 -e '{l3_lookup_state.all_l3_req_typs,l3_comb_clstr_state.request_miss}' -o artifacts/cache_info/baseline_l3.txt -- taskset -c 3 ./target/release/hash_combination --mode baseline --iterations 50 artifacts/inputs/base_photo.png
```

# Как сравнивать

- Используй одну сборку и одинаковые входы. Желательно оставить CPU 2 (SMT-сосед CPU 3) без тяжёлой нагрузки и закрыть фоновые задачи.
- Проверяй ошибки, `<not counted>` и `<not supported>`. Доля времени работы счётчиков в последних скобках должна быть близка к 100%; иначе сократи набор событий.
- `-r 5` выводит среднее: умножать счётчики на 5 не нужно. Учитывай разброс. Для проверки влияния нагрева повтори серию в обратном порядке режимов, сохранив предыдущие отчёты.
- Программа читает файл один раз, затем 50 раз обрабатывает буфер в памяти. Замер включает запуск, загрузку и аллокации; CPU-кэши между итерациями не очищаются.

Пусть `B = размер файла в байтах × 50`. Размер файла:

```bash
stat -c %s artifacts/inputs/base_photo.png
```

| Показатель | Формула |
| --- | --- |
| Ускорение fused | `time_baseline / time_fused` |
| Циклы на байт | `cycles / B` |
| Инструкции на байт | `instructions / B` |
| Промахи на MiB | `misses / (B / 1048576)` |
| Снижение промахов, % | `100 × (1 − misses_fused / misses_baseline)` |
| IPC | `instructions / cycles` |

Сравнивай одинаковые события с одинаковым охватом. При нулевом знаменателе отношение не определено.
Главные показатели — время, циклы/байт и промахи/MiB; одна доля промахов не доказывает ускорение.
Baseline сохраняет все чанки перед хэшированием, fused обрабатывает их потоком: результат включает различия в аллокациях и рабочем наборе.
Для проверки зависимости от размера данных полезны также входы меньше и существенно больше L3 (на этой машине 16 MiB; текущий вход около 21 MiB).
