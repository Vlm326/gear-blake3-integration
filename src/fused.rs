use fastcdc::v2020::StreamCDC;
use std::{error::Error, fs::File, path::Path};

pub fn process_file_fused(path: &Path) -> Result<(), Box<dyn Error>> {
    let file = File::open(path)?;
    let chunker = StreamCDC::new(file, 16 * 1024, 64 * 1024, 256 * 1024);
    let mut chunk_count = 0usize;
    let mut total_bytes = 0u64;

    for result in chunker {
        let chunk = result?;
        chunk_count += 1;
        total_bytes += chunk.length as u64;

        let chunk_digest = super::types::ChunkDigest {
            offset: chunk.offset,
            length: chunk.length,
            digest: blake3::hash(&chunk.data),
        };

        println!(
            "file={} offset={} size={} blake3={}",
            path.display(),
            chunk_digest.offset,
            chunk_digest.length,
            chunk_digest.digest
        );
    }

    eprintln!(
        "mode=fused file={} chunks={chunk_count} bytes={total_bytes}",
        path.display()
    );
    Ok(())
}
