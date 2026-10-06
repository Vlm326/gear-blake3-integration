use fastcdc::v2020::StreamCDC;
use std::{error::Error, hint::black_box, io::Read};

pub fn process_file_base<R: Read>(reader: R) -> Result<(), Box<dyn Error>> {
    let min_chunk = std::env::var("MIN_CHUNK_SIZE")
        .unwrap_or_else(|_| "16384".to_string())
        .parse()
        .unwrap();
    let avg_chunk = std::env::var("AVG_CHUNK_SIZE")
        .unwrap_or_else(|_| "65536".to_string())
        .parse()
        .unwrap();
    let max_chunk = std::env::var("MAX_CHUNK_SIZE")
        .unwrap_or_else(|_| "262144".to_string())
        .parse()
        .unwrap();
    let chunker = StreamCDC::new(reader, min_chunk, avg_chunk, max_chunk);

    // Базовый вариант сначала сохраняет все чанки, а затем отдельным проходом
    // вычисляет BLAKE3 по каждому сохранённому буферу.
    let chunks = chunker.collect::<Result<Vec<_>, _>>()?;
    let mut total_bytes = 0u64;

    for chunk in &chunks {
        total_bytes += chunk.length as u64;
        // Дайджест должен оставаться наблюдаемым, иначе оптимизатор может
        // удалить вычисление, результат которого больше нигде не используется.
        let result = super::types::ChunkDigest {
            offset: chunk.offset,
            length: chunk.length,
            digest: blake3::hash(&chunk.data),
        };
        black_box((result.offset, result.length, result.digest));
    }

    black_box((chunks.len(), total_bytes));
    Ok(())
}
