mod baseline;
mod fused;
mod types;

use std::{env, error::Error, ffi::OsStr, io, path::PathBuf};

#[derive(Clone, Copy)]
enum Mode {
    Baseline,
    Fused,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let first = args.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: hash_combination [--mode baseline|fused] <file-or-directory> ...",
        )
    })?;

    let (mode, paths) = if first == OsStr::new("--mode") {
        let mode = args
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "expected baseline or fused after --mode",
                )
            })?;
        let mode = match mode.as_str() {
            "baseline" => Mode::Baseline,
            "fused" => Mode::Fused,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown mode {mode:?}; expected baseline or fused"),
                )
                .into());
            }
        };
        (mode, args.map(PathBuf::from).collect::<Vec<_>>())
    } else {
        let mut paths = vec![PathBuf::from(first)];
        paths.extend(args.map(PathBuf::from));
        (Mode::Baseline, paths)
    };

    if paths.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provide at least one file or directory",
        )
        .into());
    }

    for path in paths {
        let mut files = Vec::new();
        baseline::collect_files(&path, &mut files)?;
        files.sort();

        for file_path in files {
            match mode {
                Mode::Baseline => baseline::process_file_base(&file_path)?,
                Mode::Fused => fused::process_file_fused(&file_path)?,
            }
        }
    }

    Ok(())
}
