//! Historical interleaved Gear/BLAKE3 implementation from commit 194611c.
//!
//! Unlike the current fused scanner, this version checks whether enough bytes
//! have been scanned and may call `Hasher::update` after every Gear candidate
//! pair. The 64-byte update threshold and the placement of the progress call
//! intentionally match that historical implementation.

use crate::fused_stream_cdc::FusedChunkData;
use fastcdc::v2020::{
    AVERAGE_MAX, AVERAGE_MIN, Error as CdcError, MAXIMUM_MAX, MAXIMUM_MIN, MINIMUM_MAX,
    MINIMUM_MIN, Normalization, get_gear_with_seed, select_masks,
};
use std::{borrow::Cow, error::Error, hint::black_box, io::Read};

struct HistoricalStreamCDC<R: Read> {
    buffer: Vec<u8>,
    capacity: usize,
    length: usize,
    source: R,
    processed: u64,
    eof: bool,
    min_size: usize,
    avg_size: usize,
    max_size: usize,
    mask_s: u64,
    mask_l: u64,
    mask_s_ls: u64,
    mask_l_ls: u64,
    gear: Cow<'static, [u64]>,
    gear_ls: Cow<'static, [u64]>,
}

impl<R: Read> HistoricalStreamCDC<R> {
    fn new(source: R, min_size: usize, avg_size: usize, max_size: usize) -> Self {
        debug_assert!(min_size >= MINIMUM_MIN);
        debug_assert!(min_size <= MINIMUM_MAX);
        debug_assert!(avg_size >= AVERAGE_MIN);
        debug_assert!(avg_size <= AVERAGE_MAX);
        debug_assert!(max_size >= MAXIMUM_MIN);
        debug_assert!(max_size <= MAXIMUM_MAX);
        debug_assert!(min_size.is_multiple_of(2), "min_size must be even");
        debug_assert!(avg_size.is_multiple_of(2), "avg_size must be even");
        debug_assert!(max_size.is_multiple_of(2), "max_size must be even");

        let (mask_s, mask_l) = select_masks(avg_size, Normalization::Level1);
        let (gear, gear_ls) = get_gear_with_seed(0);
        Self {
            buffer: vec![0_u8; max_size],
            capacity: max_size,
            length: 0,
            source,
            processed: 0,
            eof: false,
            min_size,
            avg_size,
            max_size,
            mask_s,
            mask_l,
            mask_s_ls: mask_s << 1,
            mask_l_ls: mask_l << 1,
            gear,
            gear_ls,
        }
    }

    fn fill_buffer(&mut self) -> Result<(), CdcError> {
        while !self.eof && self.length < self.capacity {
            let bytes_read = self.source.read(&mut self.buffer[self.length..])?;
            if bytes_read == 0 {
                self.eof = true;
            } else {
                self.length += bytes_read;
            }
        }
        Ok(())
    }

    fn drain_bytes(&mut self, count: usize) -> Result<Vec<u8>, CdcError> {
        if count > self.length {
            return Err(CdcError::Other(format!(
                "drain_bytes() called with count larger than length: {count} > {}",
                self.length
            )));
        }

        let mut data = Vec::with_capacity(count);
        data.extend_from_slice(&self.buffer[..count]);
        self.buffer.copy_within(count..self.length, 0);
        self.length -= count;
        Ok(data)
    }

    fn read_chunk(&mut self) -> Result<FusedChunkData, CdcError> {
        self.fill_buffer()?;
        if self.length == 0 {
            return Err(CdcError::Empty);
        }

        let (hash, count, blake_hash) = cut_gear_with_interleaved_blake3(
            &self.buffer[..self.length],
            self.min_size,
            self.avg_size,
            self.max_size,
            self.mask_s,
            self.mask_l,
            self.mask_s_ls,
            self.mask_l_ls,
            &self.gear,
            &self.gear_ls,
        );
        if count == 0 {
            return Err(CdcError::Empty);
        }

        let offset = self.processed;
        self.processed += count as u64;
        let data = self.drain_bytes(count)?;
        Ok(FusedChunkData {
            hash,
            offset,
            length: count,
            data,
            blake_hash,
        })
    }
}

impl<R: Read> Iterator for HistoricalStreamCDC<R> {
    type Item = Result<FusedChunkData, CdcError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_chunk() {
            Err(CdcError::Empty) => None,
            result => Some(result),
        }
    }
}

pub fn process_file_historical<R: Read>(reader: R) -> Result<(), Box<dyn Error>> {
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

    let chunker = HistoricalStreamCDC::new(reader, min_chunk, avg_chunk, max_chunk);
    let mut chunk_count = 0usize;
    let mut total_bytes = 0u64;

    for result in chunker {
        let chunk: FusedChunkData = result?;
        chunk_count += 1;
        total_bytes += chunk.length as u64;

        let digest = super::types::ChunkDigest {
            offset: chunk.offset,
            length: chunk.length,
            digest: chunk.blake_hash,
        };
        black_box((digest.offset, digest.length, digest.digest));
    }

    black_box((chunk_count, total_bytes));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cut_gear_with_interleaved_blake3(
    source: &[u8],
    min_size: usize,
    avg_size: usize,
    max_size: usize,
    mask_s: u64,
    mask_l: u64,
    mask_s_ls: u64,
    mask_l_ls: u64,
    gear: &[u64],
    gear_ls: &[u64],
) -> (u64, usize, blake3::Hash) {
    let gear: &[u64; 256] = gear.try_into().expect("GEAR table must have 256 entries");
    let gear_ls: &[u64; 256] = gear_ls
        .try_into()
        .expect("GEAR_LS table must have 256 entries");
    cut_gear_arr_with_interleaved_blake3(
        source, min_size, avg_size, max_size, mask_s, mask_l, mask_s_ls, mask_l_ls, gear, gear_ls,
    )
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn cut_gear_arr_with_interleaved_blake3(
    source: &[u8],
    min_size: usize,
    avg_size: usize,
    max_size: usize,
    mask_s: u64,
    mask_l: u64,
    mask_s_ls: u64,
    mask_l_ls: u64,
    gear: &[u64; 256],
    gear_ls: &[u64; 256],
) -> (u64, usize, blake3::Hash) {
    let mut blake_hasher = blake3::Hasher::new();
    let mut blake_fed_until = 0usize;

    let mut remaining = source.len();
    if remaining <= min_size {
        blake_hasher.update(source);
        return (0, remaining, blake_hasher.finalize());
    }

    debug_assert!(min_size.is_multiple_of(2), "min_size must be even");
    debug_assert!(avg_size.is_multiple_of(2), "avg_size must be even");
    debug_assert!(max_size.is_multiple_of(2), "max_size must be even");

    let mut center = avg_size;
    if remaining > max_size {
        remaining = max_size;
    } else if remaining < center {
        center = remaining;
    }
    let src = &source[..remaining];
    let mut hash = 0u64;
    let center = center & !1;
    let end = remaining & !1;
    let mut scan_start = min_size & !1;

    if scan_start < center {
        if let Some(offset) = scan_region_with_interleaved_blake3(
            src,
            scan_start,
            center,
            &mut hash,
            mask_s_ls,
            mask_s,
            gear,
            gear_ls,
            &mut blake_hasher,
            &mut blake_fed_until,
        ) {
            let digest =
                finish_blake_hasher_at(src, offset, &mut blake_hasher, &mut blake_fed_until);
            return (hash, offset, digest);
        }
        scan_start = center;
    }

    if scan_start < end
        && let Some(offset) = scan_region_with_interleaved_blake3(
            src,
            scan_start,
            end,
            &mut hash,
            mask_l_ls,
            mask_l,
            gear,
            gear_ls,
            &mut blake_hasher,
            &mut blake_fed_until,
        )
    {
        let digest = finish_blake_hasher_at(src, offset, &mut blake_hasher, &mut blake_fed_until);
        return (hash, offset, digest);
    }

    if remaining % 2 == 1 {
        hash = (hash << 1).wrapping_add(gear[src[remaining - 1] as usize]);
    }
    let digest = finish_blake_hasher_at(src, remaining, &mut blake_hasher, &mut blake_fed_until);
    (hash, remaining, digest)
}

fn finish_blake_hasher_at(
    source: &[u8],
    cut: usize,
    blake_hasher: &mut blake3::Hasher,
    blake_fed_until: &mut usize,
) -> blake3::Hash {
    blake_hasher.update(&source[*blake_fed_until..cut]);
    *blake_fed_until = cut;
    blake_hasher.finalize()
}

#[allow(clippy::too_many_arguments)]
fn scan_region_with_interleaved_blake3(
    source: &[u8],
    start: usize,
    end: usize,
    hash: &mut u64,
    mask_ls: u64,
    mask: u64,
    gear: &[u64; 256],
    gear_ls: &[u64; 256],
    blake_hasher: &mut blake3::Hasher,
    blake_fed_until: &mut usize,
) -> Option<usize> {
    let mut offset = start;
    let (blocks, remainder) = source[start..end].as_chunks::<6>();
    let mut current_hash = *hash;

    macro_rules! scan_pair_or_return {
        ($first:expr, $second:expr, $delta:literal) => {{
            current_hash = (current_hash << 2).wrapping_add($first);
            if current_hash & mask_ls == 0 {
                *hash = current_hash;
                let cut = offset + $delta;
                feed_blake3_progress(source, blake_hasher, blake_fed_until, cut);
                return Some(cut);
            }
            current_hash = current_hash.wrapping_add($second);
            if current_hash & mask == 0 {
                *hash = current_hash;
                let cut = offset + $delta + 1;
                feed_blake3_progress(source, blake_hasher, blake_fed_until, cut);
                return Some(cut);
            }

            feed_blake3_progress(source, blake_hasher, blake_fed_until, offset + $delta + 2);
        }};
    }

    for bytes in blocks {
        let g0 = gear_ls[bytes[0] as usize];
        let g1 = gear[bytes[1] as usize];
        let g2 = gear_ls[bytes[2] as usize];
        let g3 = gear[bytes[3] as usize];
        let g4 = gear_ls[bytes[4] as usize];
        let g5 = gear[bytes[5] as usize];

        scan_pair_or_return!(g0, g1, 0);
        scan_pair_or_return!(g2, g3, 2);
        scan_pair_or_return!(g4, g5, 4);
        offset += 6;
    }

    let (pairs, _) = remainder.as_chunks::<2>();
    for bytes in pairs {
        let first = gear_ls[bytes[0] as usize];
        let second = gear[bytes[1] as usize];
        scan_pair_or_return!(first, second, 0);
        offset += 2;
    }

    *hash = current_hash;
    None
}

// Intentionally retains the historical per-candidate progress check.
fn feed_blake3_progress(
    source: &[u8],
    hasher: &mut blake3::Hasher,
    fed_until: &mut usize,
    scanned_until: usize,
) {
    const BLAKE3_BATCH: usize = 64;

    if scanned_until - *fed_until >= BLAKE3_BATCH {
        hasher.update(&source[*fed_until..scanned_until]);
        *fed_until = scanned_until;
    }
}

#[cfg(test)]
mod tests {
    use super::HistoricalStreamCDC;
    use fastcdc::v2020::StreamCDC as ReferenceStreamCDC;
    use std::io::Cursor;

    #[test]
    fn historical_stream_matches_reference_chunks_and_digests() {
        let inputs = [
            Vec::new(),
            vec![0; 63],
            vec![0; 64],
            vec![0; 65],
            vec![0; 2_049],
            (0usize..20_000)
                .map(|index| (index.wrapping_mul(37) ^ (index >> 3)) as u8)
                .collect(),
        ];

        for input in inputs {
            let expected = ReferenceStreamCDC::new(Cursor::new(input.as_slice()), 64, 256, 1024)
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            let actual = HistoricalStreamCDC::new(Cursor::new(input.as_slice()), 64, 256, 1024)
                .map(Result::unwrap)
                .collect::<Vec<_>>();

            assert_eq!(actual.len(), expected.len());
            for (historical, reference) in actual.iter().zip(&expected) {
                assert_eq!(historical.hash, reference.hash, "Gear hash differs");
                assert_eq!(historical.offset, reference.offset, "chunk offset differs");
                assert_eq!(historical.length, reference.length, "chunk length differs");
                assert_eq!(historical.data, reference.data, "chunk bytes differ");
                assert_eq!(
                    historical.blake_hash,
                    blake3::hash(&reference.data),
                    "BLAKE3 digest differs"
                );
            }
        }
    }
}
