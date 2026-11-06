mod baseline;
mod fused_base;
mod fused_interleaved;
mod fused_simd_check;
mod fused_simd_expanded;
mod fused_simd_stripes;
mod fused_windowed;
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
    FusedBase,
    FusedWindowed,
    FusedSimdCheck,
    FusedSimdExpanded,
    FusedSimdStripes,
    FusedInterleaved,
    ChunkingOnly,
    Blake3Only,
    PrepareChunks,
}

const MODES: &str = "baseline|fused-base|fused-windowed|fused-simd-check|fused-simd-expanded|fused-simd-stripes|fused-interleaved|chunking-only|blake3-only|prepare-chunks";

#[derive(Debug)]
struct Options {
    mode: Mode,
    iterations: usize,
    paths: Vec<PathBuf>,
    chunk_map: Option<PathBuf>,
    gearhash_avx2_unroll: Option<gearhash::Avx2Unroll>,
    gearhash_avx2_experiment: Option<gearhash::Avx2Experiment>,
    gearhash_window: Option<gearhash::Avx2Window>,
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> io::Result<Options> {
    let mut args = args.into_iter();
    let mut mode = Mode::Baseline;
    let mut iterations = 1usize;
    let mut paths = Vec::new();
    let mut chunk_map = None;
    let mut gearhash_avx2_unroll = None;
    let mut gearhash_avx2_experiment = None;
    let mut gearhash_strip_size = None;
    let mut gearhash_strips = None;

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
                "fused-base" => Mode::FusedBase,
                "fused-windowed" => Mode::FusedWindowed,
                "fused-simd-check" => Mode::FusedSimdCheck,
                "fused-simd-expanded" => Mode::FusedSimdExpanded,
                "fused-simd-stripes" => Mode::FusedSimdStripes,
                "fused-interleaved" => Mode::FusedInterleaved,
                "chunking-only" => Mode::ChunkingOnly,
                "blake3-only" => Mode::Blake3Only,
                "prepare-chunks" => Mode::PrepareChunks,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown mode {value:?}; expected {MODES}"),
                    ));
                }
            };
        } else if arg == OsStr::new("--strip-size") || arg == OsStr::new("--strips") {
            let value = args
                .next()
                .and_then(|v| v.into_string().ok())
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "expected integer after gearhash geometry option",
                    )
                })?;
            if arg == OsStr::new("--strip-size") {
                gearhash_strip_size = Some(value);
            } else {
                gearhash_strips = Some(value);
            }
        } else if arg == OsStr::new("--scanner") {
            gearhash_avx2_experiment = Some(
                match args.next().and_then(|v| v.into_string().ok()).as_deref() {
                    Some("original") => gearhash::Avx2Experiment::Original,
                    Some("control") => gearhash::Avx2Experiment::Control,
                    Some("expanded") => gearhash::Avx2Experiment::Unroll,
                    Some("grouped-check") => gearhash::Avx2Experiment::Check,
                    Some("combined") => gearhash::Avx2Experiment::Combined,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--scanner requires original|control|expanded|grouped-check|combined",
                        ));
                    }
                },
            );
        } else if arg == OsStr::new("--unroll") {
            gearhash_avx2_unroll = Some(
                match args.next().and_then(|v| v.into_string().ok()).as_deref() {
                    Some("1") => gearhash::Avx2Unroll::One,
                    Some("2") => gearhash::Avx2Unroll::Two,
                    Some("4") => gearhash::Avx2Unroll::Four,
                    Some("8") => gearhash::Avx2Unroll::Eight,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "--unroll requires 1|2|4|8",
                        ));
                    }
                },
            );
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
                ));
            }
        } else {
            paths.push(PathBuf::from(arg));
        }
    }

    let gearhash_window = match (gearhash_strip_size, gearhash_strips) {
        (None, None) => None,
        (Some(size), Some(strips))
            if mode == Mode::FusedSimdStripes
                && gearhash_avx2_unroll.is_none()
                && gearhash_avx2_experiment.is_none() =>
        {
            Some(gearhash::Avx2Window::new(size, strips).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "strip size must be 128|256|512|1024, strips must be 2|4",
                )
            })?)
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "use --strip-size and --strips together in fused-simd-stripes, without unroll/experiment",
            ));
        }
    };
    if gearhash_avx2_experiment.is_some()
        && (mode != Mode::FusedSimdStripes || gearhash_avx2_unroll.is_some())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--scanner requires fused-simd-stripes and cannot be combined with --unroll",
        ));
    }
    if gearhash_avx2_unroll.is_some() && mode != Mode::FusedSimdStripes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--unroll requires --mode fused-simd-stripes",
        ));
    }
    let uses_map = matches!(mode, Mode::PrepareChunks | Mode::Blake3Only);
    if uses_map && (chunk_map.is_none() || paths.len() != 1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "prepare-chunks/blake3_only require --chunk-map <path> and exactly one input file",
        ));
    }
    if !uses_map && chunk_map.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--chunk-map is only supported by prepare-chunks and blake3-only",
        ));
    }
    if mode == Mode::PrepareChunks && iterations != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "prepare-chunks requires --iterations 1",
        ));
    }
    if paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "usage: hash_combination [--mode {MODES}] [--chunk-map <path>] [--unroll 1|2|4|8] [--iterations N] <file-or-directory> ..."
            ),
        ));
    }
    Ok(Options {
        mode,
        iterations,
        paths,
        chunk_map,
        gearhash_avx2_unroll,
        gearhash_avx2_experiment,
        gearhash_window,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Options {
        mode,
        iterations,
        paths,
        chunk_map,
        gearhash_avx2_unroll,
        gearhash_avx2_experiment,
        gearhash_window,
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
                    Mode::FusedBase => fused_base::process_file_fused(reader)?,
                    Mode::FusedWindowed => fused_windowed::process_file_fused_windowed(reader)?,
                    Mode::FusedSimdCheck => {
                        fused_simd_check::process_file_fused_simd_check(reader)?
                    }
                    Mode::FusedSimdExpanded => {
                        fused_simd_expanded::process_file_fused_simd_expanded(reader)?
                    }
                    Mode::FusedSimdStripes => fused_simd_stripes::process_file_fused_simd_stripes(
                        reader,
                        gearhash_avx2_unroll,
                        gearhash_avx2_experiment,
                        gearhash_window,
                    )?,
                    Mode::FusedInterleaved => {
                        fused_interleaved::process_file_fused_interleaved(reader)?
                    }
                    Mode::ChunkingOnly => isolated::process_chunking_only(reader)?,
                    Mode::Blake3Only | Mode::PrepareChunks => unreachable!(),
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
    fn gearhash_geometry_cli() {
        for size in ["128", "256", "512", "1024"] {
            for strips in ["2", "4"] {
                assert!(
                    options(&[
                        "--mode",
                        "fused-simd-stripes",
                        "--strip-size",
                        size,
                        "--strips",
                        strips,
                        "input"
                    ])
                    .unwrap()
                    .gearhash_window
                    .is_some()
                );
            }
        }
        for args in [
            vec![
                "--mode",
                "fused-simd-stripes",
                "--strip-size",
                "128",
                "input",
            ],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--strip-size",
                "64",
                "--strips",
                "4",
                "input",
            ],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--strip-size",
                "128",
                "--strips",
                "3",
                "input",
            ],
            vec![
                "--mode",
                "fused-base",
                "--strip-size",
                "128",
                "--strips",
                "4",
                "input",
            ],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--strip-size",
                "128",
                "--strips",
                "4",
                "--unroll",
                "4",
                "input",
            ],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--strip-size",
                "128",
                "--strips",
                "4",
                "--scanner",
                "check",
                "input",
            ],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--strip-size",
                "bad",
                "--strips",
                "4",
                "input",
            ],
        ] {
            assert!(options(&args).is_err());
        }
    }

    #[test]
    fn avx2_experiment_cli() {
        for value in [
            "original",
            "control",
            "expanded",
            "grouped-check",
            "combined",
        ] {
            assert!(
                options(&["--mode", "fused-simd-stripes", "--scanner", value, "input"])
                    .unwrap()
                    .gearhash_avx2_experiment
                    .is_some()
            );
        }
        for args in [
            vec!["--scanner"],
            vec!["--mode", "fused-base", "--scanner", "check", "input"],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--scanner",
                "unknown",
                "input",
            ],
            vec![
                "--mode",
                "fused-simd-stripes",
                "--scanner",
                "check",
                "--unroll",
                "4",
                "input",
            ],
        ] {
            assert!(options(&args).is_err());
        }
    }

    #[test]
    fn avx2_unroll_cli() {
        for value in ["1", "2", "4", "8"] {
            assert!(
                options(&["--mode", "fused-simd-stripes", "--unroll", value, "input"])
                    .unwrap()
                    .gearhash_avx2_unroll
                    .is_some()
            );
        }
        for args in [
            vec!["--unroll"],
            vec!["--mode", "fused-simd-stripes", "--unroll", "3", "input"],
            vec!["--mode", "fused-base", "--unroll", "4", "input"],
        ] {
            assert!(options(&args).is_err());
        }
    }

    #[test]
    fn new_and_existing_modes_are_accepted() {
        for (name, expected) in [
            ("baseline", Mode::Baseline),
            ("fused-base", Mode::FusedBase),
            ("fused-windowed", Mode::FusedWindowed),
            ("fused-simd-check", Mode::FusedSimdCheck),
            ("fused-simd-expanded", Mode::FusedSimdExpanded),
            ("fused-simd-stripes", Mode::FusedSimdStripes),
            ("fused-interleaved", Mode::FusedInterleaved),
            ("chunking-only", Mode::ChunkingOnly),
        ] {
            let args = options(&["--mode", name, "--iterations", "2", "input"]).unwrap();
            assert_eq!(args.mode, expected);
            assert_eq!(args.iterations, 2);
        }
        assert_eq!(options(&["input"]).unwrap().mode, Mode::Baseline);
        assert!(options(&["--mode", "build-graph"]).is_err());
        for (name, expected) in [
            ("prepare-chunks", Mode::PrepareChunks),
            ("blake3-only", Mode::Blake3Only),
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
            vec!["--mode", "blake3-only", "input"],
            vec!["--mode", "prepare-chunks", "input"],
            vec!["--mode", "blake3-only", "--chunk-map", "map"],
            vec!["--mode", "blake3-only", "--chunk-map", "map", "a", "b"],
            vec![
                "--mode",
                "prepare-chunks",
                "--chunk-map",
                "map",
                "--iterations",
                "2",
                "input",
            ],
            vec!["--chunk-map", "map", "input"],
            vec!["--mode", "fused-base", "--chunk-map", "map", "input"],
            vec!["--mode", "fused-windowed", "--chunk-map", "map", "input"],
            vec!["--mode", "fused-simd-check", "--chunk-map", "map", "input"],
            vec![
                "--mode",
                "fused-simd-expanded",
                "--chunk-map",
                "map",
                "input",
            ],
            vec!["--mode", "blake3-only", "--chunk-map"],
            vec!["--iterations", "0", "input"],
            vec!["--mode", "unknown", "input"],
        ] {
            assert!(options(&args).is_err(), "{args:?}");
        }
    }
}
