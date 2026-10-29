import subprocess

with open("/home/vlm326/Documents/Projects/Rust/hash_combination/docs/cache_profiling_commands.md", "r") as f:
    for line in f:
        if line.startswith("#"):
            continue
        if line.startswith("<!--"):
            continue
        if line.strip() == "":
            continue
        print(f"Running command: {line.strip()}")
        subprocess.run(line.strip(), shell=True, check=True)