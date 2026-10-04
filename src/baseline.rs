use fastcdc::v2020::StreamCDC;
use std::{error::Error, hint::black_box, io::Read};

pub fn process_file_base<R: Read>(reader: R) -> Result<(), Box<dyn Error>> {
    let chunker = StreamCDC::new(reader, 16 * 1024, 64 * 1024, 256 * 1024);

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
