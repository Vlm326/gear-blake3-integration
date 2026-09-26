use fastcdc::v2020::StreamCDC;
use std::{
    error::Error,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

pub fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() || file_type.is_dir() {
                collect_files(&entry.path(), files)?;
            }
        }
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a regular file or directory: {}", path.display()),
        )
        .into());
    }

    Ok(())
}

pub fn process_file_base(path: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let chunker = StreamCDC::new(file, 16 * 1024, 64 * 1024, 256 * 1024);
    let chunks = chunker.collect::<Result<Vec<_>, _>>()?;

    let chunk_digests: Vec<_> = chunks
        .iter()
        .map(|chunk| super::types::ChunkDigest {
            offset: chunk.offset,
            length: chunk.length,
            digest: blake3::hash(&chunk.data),
        })
        .collect();

    for chunk_digest in &chunk_digests {
        println!(
            "file={} offset={} size={} blake3={}",
            path.display(),
            chunk_digest.offset,
            chunk_digest.length,
            chunk_digest.digest
        );
    }

    let total_bytes: u64 = chunk_digests.iter().map(|chunk| chunk.length as u64).sum();
    eprintln!(
        "mode=baseline file={} chunks={} bytes={total_bytes}",
        path.display(),
        chunk_digests.len()
    );

    Ok(())
}
