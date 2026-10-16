mod analysys;
mod baseline;
mod fused;
mod fused_stream_cdc;
mod historical_fused;
mod pub_utils;
mod types;

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, Cursor},
    path::PathBuf,
};

#[derive(Clone, Copy)]
enum Mode {
    Baseline,
    Fused,
    Historical,
    Graph,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let mut mode = Mode::Baseline;
    let mut iterations = 1usize;
    let mut paths = Vec::new();

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--mode") {
            let value = args
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "expected baseline, fused, historical, or build-graph after --mode",
                    )
                })?;
            mode = match value.as_str() {
                "baseline" => Mode::Baseline,
                "fused" => Mode::Fused,
                "historical" => Mode::Historical,
                "build-graph" => Mode::Graph,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "unknown mode {value:?}; expected baseline/fused/historical/build-graph"
                        ),
                    )
                    .into());
                }
            };
        } else if arg == OsStr::new("--iterations") {
            let value = args
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "expected a positive integer after --iterations",
                    )
                })?;
            iterations = value.parse().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--iterations must be a positive integer",
                )
            })?;
            if iterations == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "--iterations must be greater than zero",
                )
                .into());
            }
        } else {
            paths.push(PathBuf::from(arg));
        }
    }

    if paths.is_empty() {
        match mode {
            Mode::Graph => {
                let root_path = std::env::current_dir()?;
                let graph_data_path = root_path.join("artifacts/benchmarks/chunk_sweep");
                let fused_path = graph_data_path.join("results.txt");
                let baseline_path = graph_data_path.join("results_baseline.txt");

                let fused_data = fs::read_to_string(fused_path)?;
                let baseline_data = fs::read_to_string(baseline_path)?;
                let fused_results = analysys::parser::parse_run_results(&fused_data)?;
                let baseline_results = analysys::parser::parse_run_results(&baseline_data)?;
                analysys::graph_builder::build_graph(fused_results, baseline_results)?;
                return Ok(());
            }
            _ => {}
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: hash_combination [--mode baseline|fused|historical] [--iterations N] <file-or-directory> ...",
        )
        .into());
    }

    for path in paths {
        let mut files = Vec::new();
        pub_utils::collect_files(&path, &mut files)?;
        files.sort();

        for file_path in files {
            // Файл загружается один раз; каждый повтор получает новый Cursor
            // на тот же самый буфер в памяти.
            let file_data = fs::read(&file_path)?;

            for _ in 0..iterations {
                let reader = Cursor::new(file_data.as_slice());
                match mode {
                    Mode::Baseline => baseline::process_file_base(reader)?,
                    Mode::Fused => fused::process_file_fused(reader)?,
                    Mode::Historical => historical_fused::process_file_historical(reader)?,
                    Mode::Graph => unreachable!(),
                }
            }
        }
    }

    Ok(())
}
