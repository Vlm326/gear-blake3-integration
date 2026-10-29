mod analysys;
mod baseline;
mod fused;
mod fused_stream_cdc;
mod historical_fused;
mod isolated;
mod pub_utils;
mod types;

use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Cursor},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Baseline,
    Fused,
    Historical,
    ChunkingOnly,
    Blake3Only,
    PrepareChunks,
    Graph,
}

const MODES: &str =
    "baseline|fused|historical|chunking_only|blake3_only|prepare_chunks|build-graph";

#[derive(Debug)]
struct Options {
    mode: Mode,
    iterations: usize,
    paths: Vec<PathBuf>,
    chunk_map: Option<PathBuf>,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> io::Result<Options> {
    let mut args = args.into_iter();
    let mut mode = Mode::Baseline;
    let mut iterations = 1usize;
    let mut paths = Vec::new();
    let mut chunk_map = None;

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--mode") {
            let value = args
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("expected {MODES} after --mode"),
                    )
                })?;
            mode = match value.as_str() {
                "baseline" => Mode::Baseline,
                "fused" => Mode::Fused,
                "historical" => Mode::Historical,
                "build-graph" => Mode::Graph,
                "chunking_only" => Mode::ChunkingOnly,
                "blake3_only" => Mode::Blake3Only,
                "prepare_chunks" => Mode::PrepareChunks,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown mode {value:?}; expected {MODES}"),
                    )
                    .into());
                }
            };
        } else if arg == OsStr::new("--chunk-map") {
            let value = args.next().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "expected a path after --chunk-map",
                )
            })?;
            chunk_map = Some(PathBuf::from(value));
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

    let uses_map = matches!(mode, Mode::PrepareChunks | Mode::Blake3Only);
    if uses_map && (chunk_map.is_none() || paths.len() != 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "prepare_chunks/blake3_only require --chunk-map <path> and exactly one input file",
        ));
    }
    if !uses_map && chunk_map.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--chunk-map is only supported by prepare_chunks and blake3_only",
        ));
    }
    if mode == Mode::PrepareChunks && iterations != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "prepare_chunks requires --iterations 1",
        ));
    }
    if paths.is_empty() && mode != Mode::Graph {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "usage: hash_combination [--mode {MODES}] [--chunk-map <path>] [--iterations N] <file-or-directory> ..."
            ),
        ));
    }
    Ok(Options {
        mode,
        iterations,
        paths,
        chunk_map,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Options {
        mode,
        iterations,
        paths,
        chunk_map,
    } = parse_args(env::args_os().skip(1))?;

    if matches!(mode, Mode::PrepareChunks | Mode::Blake3Only) {
        let input = &paths[0];
        if !fs::metadata(input)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "chunk-map modes require a regular input file",
            )
            .into());
        }
        let map_path = chunk_map.as_ref().unwrap();
        let file_data = fs::read(input)?;
        if mode == Mode::PrepareChunks {
            let map = isolated::ChunkMap::prepare(&file_data, isolated::ChunkSizes::from_env()?)?;
            // Refuse an existing output: a fresh map path also protects hard-linked inputs.
            use std::io::Write;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(map_path)?;
            output.write_all(map.encode().as_bytes())?;
        } else {
            let map = isolated::ChunkMap::decode(&fs::read_to_string(map_path)?, file_data.len())?;
            for _ in 0..iterations {
                map.process_blake3_only(&file_data);
            }
        }
        return Ok(());
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
            "build-graph requires no input paths",
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
                    Mode::ChunkingOnly => isolated::process_chunking_only(reader)?,
                    Mode::Graph | Mode::Blake3Only | Mode::PrepareChunks => unreachable!(),
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> io::Result<Options> {
        parse_args(args.iter().map(OsString::from))
    }

    #[test]
    fn new_and_existing_modes_are_accepted() {
        for (name, expected) in [
            ("baseline", Mode::Baseline),
            ("fused", Mode::Fused),
            ("historical", Mode::Historical),
            ("chunking_only", Mode::ChunkingOnly),
        ] {
            let args = options(&["--mode", name, "--iterations", "2", "input"]).unwrap();
            assert_eq!(args.mode, expected);
            assert_eq!(args.iterations, 2);
        }
        assert_eq!(options(&["input"]).unwrap().mode, Mode::Baseline);
        assert_eq!(
            options(&["--mode", "build-graph"]).unwrap().mode,
            Mode::Graph
        );
        for (name, expected) in [
            ("prepare_chunks", Mode::PrepareChunks),
            ("blake3_only", Mode::Blake3Only),
        ] {
            assert_eq!(
                options(&["--mode", name, "--chunk-map", "chunks.tsv", "input"])
                    .unwrap()
                    .mode,
                expected
            );
        }
    }

    #[test]
    fn invalid_chunk_map_cli_combinations_are_rejected() {
        for args in [
            vec!["--mode", "blake3_only", "input"],
            vec!["--mode", "prepare_chunks", "input"],
            vec!["--mode", "blake3_only", "--chunk-map", "map"],
            vec!["--mode", "blake3_only", "--chunk-map", "map", "a", "b"],
            vec![
                "--mode",
                "prepare_chunks",
                "--chunk-map",
                "map",
                "--iterations",
                "2",
                "input",
            ],
            vec!["--chunk-map", "map", "input"],
            vec!["--mode", "fused", "--chunk-map", "map", "input"],
            vec!["--mode", "blake3_only", "--chunk-map"],
            vec!["--iterations", "0", "input"],
            vec!["--mode", "unknown", "input"],
        ] {
            assert!(options(&args).is_err(), "{args:?}");
        }
    }
}
