# Run from repository root in a terminal with sudo authentication.
# Two missing system-wide L3 measurements; existing L3 reports will be overwritten.

sudo env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -a -r 5 -e '{l3_lookup_state.all_l3_req_typs,l3_comb_clstr_state.request_miss}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/chunking_only_l3.txt -- taskset -c 3 ./target/release/hash_combination --mode chunking_only --iterations 50 artifacts/inputs/base_photo.png

sudo env LC_ALL=C BLAKE3_BATCH=8192 MIN_CHUNK_SIZE=4096 AVG_CHUNK_SIZE=10240 MAX_CHUNK_SIZE=65536 perf stat -a -r 5 -e '{l3_lookup_state.all_l3_req_typs,l3_comb_clstr_state.request_miss}' -o artifacts/cache_info/runs/2026-10-25_23-09-17/blake3_only_l3.txt -- taskset -c 3 ./target/release/hash_combination --mode blake3_only --chunk-map artifacts/cache_info/runs/2026-10-25_23-09-17/base_photo.chunks.tsv --iterations 50 artifacts/inputs/base_photo.png
