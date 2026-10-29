# Run from repository root. Existing reports will be overwritten if rerun.
# Preparation: choose a new map path if this map already exists.
env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 ./target/release/hash_combination --mode prepare_chunks --chunk-map artifacts/cache_info/runs/2026-10-25_23-09-17/base_photo.chunks.tsv artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,instructions:u,cache-references:u,cache-misses:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/chunking_only_basic.txt -- taskset -c 3 ./target/release/hash_combination --mode chunking_only --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,instructions:u,cache-references:u,cache-misses:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/blake3_only_basic.txt -- taskset -c 3 ./target/release/hash_combination --mode blake3_only --chunk-map artifacts/cache_info/runs/2026-10-25_23-09-17/base_photo.chunks.tsv --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{L1-dcache-loads:u,L1-dcache-load-misses:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/blake3_only_l1.txt -- taskset -c 3 ./target/release/hash_combination --mode blake3_only --chunk-map artifacts/cache_info/runs/2026-10-25_23-09-17/base_photo.chunks.tsv --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{L1-dcache-loads:u,L1-dcache-load-misses:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/chunking_only_l1.txt -- taskset -c 3 ./target/release/hash_combination --mode chunking_only --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{dTLB-loads:u,dTLB-load-misses:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/chunking_only_dtlb.txt -- taskset -c 3 ./target/release/hash_combination --mode chunking_only --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{dTLB-loads:u,dTLB-load-misses:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/blake3_only_dtlb.txt -- taskset -c 3 ./target/release/hash_combination --mode blake3_only --chunk-map artifacts/cache_info/runs/2026-10-25_23-09-17/base_photo.chunks.tsv --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,l2_cache_misses_from_dc_misses:u,l2_fill_pending.l2_fill_busy:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/blake3_only_l2.txt -- taskset -c 3 ./target/release/hash_combination --mode blake3_only --chunk-map artifacts/cache_info/runs/2026-10-25_23-09-17/base_photo.chunks.tsv --iterations 50 artifacts/inputs/base_photo.png

env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -r 5 -e '{cycles:u,l2_cache_misses_from_dc_misses:u,l2_fill_pending.l2_fill_busy:u}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/chunking_only_l2.txt -- taskset -c 3 ./target/release/hash_combination --mode chunking_only --iterations 50 artifacts/inputs/base_photo.png
