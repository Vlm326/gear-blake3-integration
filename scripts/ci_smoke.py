#!/usr/bin/env python3
"""Check processing modes on generated input without collecting benchmark results."""

import os
from pathlib import Path
import random
import subprocess
import tempfile


def main():
    executable = Path("target/release") / (
        "hash_combination.exe" if os.name == "nt" else "hash_combination"
    )
    executable = executable.resolve(strict=True)
    env = dict(
        os.environ,
        MIN_CHUNK_SIZE="16384",
        AVG_CHUNK_SIZE="65536",
        MAX_CHUNK_SIZE="262144",
        BLAKE3_BATCH="8192",
    )

    with tempfile.TemporaryDirectory(prefix="hash-combination-ci-") as directory:
        input_path = Path(directory) / "input.bin"
        map_path = Path(directory) / "input.chunks.tsv"
        input_path.write_bytes(random.Random(0).randbytes(8 * 1024 * 1024))

        def run(mode, *extra):
            print(f"Checking CLI mode: {mode}", flush=True)
            subprocess.run(
                [
                    str(executable), "--mode", mode, "--iterations", "1",
                    *extra, str(input_path),
                ],
                env=env,
                check=True,
                timeout=120,
            )

        for mode in (
            "baseline", "fused-base", "fused-windowed", "fused-simd-check", "fused-simd-expanded",
            "fused-simd-stripes", "fused-interleaved", "chunking-only",
        ):
            run(mode)
        run("prepare-chunks", "--chunk-map", str(map_path))
        run("blake3-only", "--chunk-map", str(map_path))


if __name__ == "__main__":
    main()
