mod baseline;
mod fused;
mod fused_stream_cdc;
mod pub_utils;
mod types;

use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs,
    io::{self, Cursor},
    path::PathBuf,
};

#[derive(Clone, Copy)]
enum Mode {
    Baseline,
    Fused,
}

fn main() -> Result<(), Box<dyn Error>> {
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
                        "expected baseline or fused after --mode",
                    )
                })?;
            mode = match value.as_str() {
                "baseline" => Mode::Baseline,
                "fused" => Mode::Fused,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown mode {value:?}; expected baseline or fused"),
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
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: hash_combination [--mode baseline|fused] [--iterations N] <file-or-directory> ...",
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
                }
            }
        }
    }

    Ok(())
}
