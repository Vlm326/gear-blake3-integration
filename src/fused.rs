use crate::fused_stream_cdc;
use std::{error::Error, hint::black_box, io::Read};

pub fn process_file_fused<R: Read>(reader: R) -> Result<(), Box<dyn Error>> {
    let chunker = fused_stream_cdc::StreamCDC::new(reader, 16 * 1024, 64 * 1024, 256 * 1024);
    let mut chunk_count = 0usize;
    let mut total_bytes = 0u64;

    for result in chunker {
        let chunk = result?;
        chunk_count += 1;
        total_bytes += chunk.length as u64;

        // Используем дайджест, который уже вычислил FusedStreamCDC; второй
        // проход BLAKE3 здесь не запускается.
        let result = super::types::ChunkDigest {
            offset: chunk.offset,
            length: chunk.length,
            digest: chunk.blake_hash,
        };
        black_box((result.offset, result.length, result.digest));
    }

    black_box((chunk_count, total_bytes));
    Ok(())
}
