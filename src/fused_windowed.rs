//! Experimental bounded-window Gear/BLAKE3 interleaving.
use fastcdc::v2020::{
    AVERAGE_MAX, AVERAGE_MIN, Error, MAXIMUM_MAX, MAXIMUM_MIN, MINIMUM_MAX, MINIMUM_MIN,
    Normalization, get_gear_with_seed, select_masks,
};
use std::{borrow::Cow, io::Read};

use crate::fused_base::FusedChunkData;

pub struct StreamCDC<R: Read> {
    /// Buffer of data from source for finding cut points.
    buffer: Vec<u8>,
    window: usize,
    /// Maximum capacity of the buffer (always `max_size`).
    capacity: usize,
    /// Number of relevant bytes in the `buffer`.
    length: usize,
    /// Source from which data is read into `buffer`.
    source: R,
    /// Number of bytes read from the source so far.
    processed: u64,
    /// True when the source produces no more data.
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

impl<R: Read> StreamCDC<R> {
    ///
    /// Construct a [`StreamCDC`] that will process bytes from the given source.
    ///
    /// Uses chunk size normalization level 1 by default.
    ///
    pub fn new(
        source: R,
        min_size: usize,
        avg_size: usize,
        max_size: usize,
        window: usize,
    ) -> std::io::Result<Self> {
        StreamCDC::with_level_and_seed(
            source,
            min_size,
            avg_size,
            max_size,
            Normalization::Level1,
            0,
            window,
        )
    }

    ///
    /// Create a new [`StreamCDC`] with the given normalization level and hash seed.
    ///
    pub fn with_level_and_seed(
        source: R,
        min_size: usize,
        avg_size: usize,
        max_size: usize,
        level: Normalization,
        seed: u64,
        window: usize,
    ) -> std::io::Result<Self> {
        validate_window(window)?;
        debug_assert!(min_size >= MINIMUM_MIN);
        debug_assert!(min_size <= MINIMUM_MAX);
        debug_assert!(avg_size >= AVERAGE_MIN);
        debug_assert!(avg_size <= AVERAGE_MAX);
        debug_assert!(max_size >= MAXIMUM_MIN);
        debug_assert!(max_size <= MAXIMUM_MAX);
        debug_assert!(min_size.is_multiple_of(2), "min_size must be even");
        debug_assert!(avg_size.is_multiple_of(2), "avg_size must be even");
        debug_assert!(max_size.is_multiple_of(2), "max_size must be even");
        let (mask_s, mask_l) = select_masks(avg_size, level);
        let (gear, gear_ls) = get_gear_with_seed(seed);
        Ok(Self {
            window,
            buffer: vec![0_u8; max_size],
            capacity: max_size,
            length: 0,
            source,
            eof: false,
            processed: 0,
            min_size,
            avg_size,
            max_size,
            mask_s,
            mask_l,
            mask_s_ls: mask_s << 1,
            mask_l_ls: mask_l << 1,
            gear,
            gear_ls,
        })
    }

    /// Fill the buffer with data from the source, returning the number of bytes
    /// read (zero if end of source has been reached).
    fn fill_buffer(&mut self) -> Result<usize, Error> {
        // this code originally copied from asuran crate
        if self.eof {
            Ok(0)
        } else {
            let mut all_bytes_read = 0;
            while !self.eof && self.length < self.capacity {
                let bytes_read = self.source.read(&mut self.buffer[self.length..])?;
                if bytes_read == 0 {
                    self.eof = true;
                } else {
                    self.length += bytes_read;
                    all_bytes_read += bytes_read;
                }
            }
            Ok(all_bytes_read)
        }
    }

    /// Drains a specified number of bytes from the buffer, then resizes the
    /// buffer back to `capacity` size in preparation for further reads.
    fn drain_bytes(&mut self, count: usize) -> Result<Vec<u8>, Error> {
        // this code originally copied from asuran crate
        if count > self.length {
            Err(Error::Other(format!(
                "drain_bytes() called with count larger than length: {} > {}",
                count, self.length
            )))
        } else {
            let mut data = Vec::with_capacity(count);
            data.extend_from_slice(&self.buffer[..count]);
            self.buffer.copy_within(count..self.length, 0);
            self.length -= count;
            Ok(data)
        }
    }

    /// Find the next chunk in the source. If the end of the source has been
    /// reached, returns `Error::Empty` as the error.
    fn read_chunk(&mut self) -> Result<FusedChunkData, Error> {
        self.fill_buffer()?;
        if self.length == 0 {
            Err(Error::Empty)
        } else {
            let (hash, count, blake_hash) = cut_gear_with_blake3(
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
                self.window,
            );
            if count == 0 {
                Err(Error::Empty)
            } else {
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
    }
}

impl<R: Read> Iterator for StreamCDC<R> {
    type Item = Result<FusedChunkData, Error>;

    fn next(&mut self) -> Option<Result<FusedChunkData, Error>> {
        let slice = self.read_chunk();
        if let Err(Error::Empty) = slice {
            None
        } else {
            Some(slice)
        }
    }
}

/// Находит следующую границу чанка и одновременно вычисляет его BLAKE3.
/// Таблицы Gear преобразуются в массивы фиксированного размера, чтобы в
/// горячем цикле сканирования не требовалась проверка границ индексов.
///
#[allow(clippy::too_many_arguments)]
fn cut_gear_with_blake3(
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
    window: usize,
) -> (u64, usize, blake3::Hash) {
    let gear: &[u64; 256] = gear.try_into().expect("GEAR table must have 256 entries");
    let gear_ls: &[u64; 256] = gear_ls
        .try_into()
        .expect("GEAR_LS table must have 256 entries");
    cut_gear_arr_blake3(
        source, min_size, avg_size, max_size, mask_s, mask_l, mask_s_ls, mask_l_ls, gear, gear_ls,
        window,
    )
}

/// Сканирует один чанк, сохраняя порядок проверки кандидатов исходного FastCDC.
#[allow(clippy::too_many_arguments)]
#[inline]
fn cut_gear_arr_blake3(
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
    window: usize,
) -> (u64, usize, blake3::Hash) {
    // Windows cover the chunk from byte zero, including the prefix FastCDC skips.
    let remaining = source.len().min(max_size);
    let src = &source[..remaining];
    let mut hasher = blake3::Hasher::new();
    if remaining <= min_size {
        for bytes in src.chunks(window) {
            hasher.update(bytes);
        }
        return (0, remaining, hasher.finalize());
    }
    let center = avg_size.min(remaining) & !1;
    let end = remaining & !1;
    let mut hash = 0;
    let mut fed = 0;
    while fed < remaining {
        let window_end = fed.saturating_add(window).min(remaining);
        let mut start = fed.max(min_size);
        let scan_end = window_end.min(end);
        while start < scan_end {
            let (region_end, shifted_mask, mask) = if start < center {
                (scan_end.min(center), mask_s_ls, mask_s)
            } else {
                (scan_end, mask_l_ls, mask_l)
            };
            if let Some(cut) = scan_region(
                src,
                start,
                region_end,
                &mut hash,
                shifted_mask,
                mask,
                gear,
                gear_ls,
            ) {
                hasher.update(&src[fed..cut]);
                return (hash, cut, hasher.finalize());
            }
            start = region_end;
        }
        hasher.update(&src[fed..window_end]);
        fed = window_end;
    }
    if remaining % 2 == 1 {
        hash = (hash << 1).wrapping_add(gear[src[remaining - 1] as usize]);
    }
    (hash, remaining, hasher.finalize())
}

#[allow(clippy::too_many_arguments)]
fn scan_region(
    source: &[u8],
    start: usize,
    end: usize,
    hash: &mut u64,
    mask_ls: u64,
    mask: u64,
    gear: &[u64; 256],
    gear_ls: &[u64; 256],
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
                return Some(cut);
            }
            current_hash = current_hash.wrapping_add($second);
            if current_hash & mask == 0 {
                *hash = current_hash;
                // Смещение +1 сохраняет принятую FastCDC исключительную
                // границу для кандидата на втором байте пары.
                let cut = offset + $delta + 1;
                return Some(cut);
            }
        }};
    }

    for bytes in blocks {
        // Загружаем Gear-значения для блока заранее, сохраняя порядок проверки
        // пар, используемый исходной реализацией FastCDC.
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

fn validate_window(window: usize) -> std::io::Result<()> {
    if window == 0 || !window.is_multiple_of(2) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "BLAKE3_BATCH must be a positive even window size",
        ));
    }
    Ok(())
}

/// Experimental mode: same streaming copies as fused-base, with bounded hash windows.
pub fn process_file_fused_windowed<R: Read>(reader: R) -> Result<(), Box<dyn std::error::Error>> {
    fn number(name: &str, default: usize) -> std::io::Result<usize> {
        match std::env::var(name) {
            Ok(value) => value.parse().map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid {name}: {value}"),
                )
            }),
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(error) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, error)),
        }
    }
    let chunker = StreamCDC::new(
        reader,
        number("MIN_CHUNK_SIZE", 16384)?,
        number("AVG_CHUNK_SIZE", 65536)?,
        number("MAX_CHUNK_SIZE", 262144)?,
        number("BLAKE3_BATCH", 8192)?,
    )?;
    let mut count = 0usize;
    let mut bytes = 0u64;
    for chunk in chunker {
        let chunk = chunk?;
        count += 1;
        bytes += chunk.length as u64;
        std::hint::black_box((chunk.offset, chunk.length, chunk.blake_hash));
    }
    std::hint::black_box((count, bytes));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fastcdc::v2020::StreamCDC as Reference;
    use std::io::{self, Cursor};

    fn data(length: usize) -> Vec<u8> {
        let mut state = 0x1234_5678_9abc_def0u64;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    fn compare(
        input: &[u8],
        min: usize,
        avg: usize,
        max: usize,
        seed: u64,
        level: Normalization,
        window: usize,
    ) {
        let expected =
            Reference::with_level_and_seed(Cursor::new(input), min, avg, max, level, seed)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        let actual =
            StreamCDC::with_level_and_seed(Cursor::new(input), min, avg, max, level, seed, window)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        let control = crate::fused_base::StreamCDC::with_level_and_seed(
            Cursor::new(input),
            min,
            avg,
            max,
            level,
            seed,
        )
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        assert_eq!(actual, control, "window={window}, seed={seed}");
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(&expected) {
            assert_eq!((a.offset, a.length, a.hash), (b.offset, b.length, b.hash));
            assert_eq!(a.data, b.data);
            assert_eq!(a.blake_hash, blake3::hash(&b.data));
        }
    }

    #[test]
    fn windows_preserve_reference_chunks_and_digests() {
        for window in [2, 64, 512, 1024, 2048, 4096, 8192, 16384, 32768] {
            for seed in [0, 1, u64::MAX] {
                for level in [
                    Normalization::Level0,
                    Normalization::Level1,
                    Normalization::Level2,
                    Normalization::Level3,
                ] {
                    compare(&data(70_003), 64, 256, 4096, seed, level, window);
                }
            }
            // The settings used in the board experiment, including chunks larger than a window.
            compare(
                &data(200_003),
                4096,
                10240,
                65536,
                0,
                Normalization::Level1,
                window,
            );
        }
    }

    #[test]
    fn tails_and_window_edges_match_reference() {
        for window in [64, 512, 1024, 8192] {
            for length in [
                0,
                1,
                63,
                64,
                65,
                255,
                256,
                257,
                1023,
                1024,
                1025,
                window - 1,
                window,
                window + 1,
                2 * window - 1,
                2 * window + 1,
            ] {
                compare(
                    &data(length),
                    64,
                    256,
                    1024,
                    0,
                    Normalization::Level1,
                    window,
                );
                compare(
                    &vec![0; length],
                    64,
                    256,
                    1024,
                    0,
                    Normalization::Level1,
                    window,
                );
            }
        }
    }

    #[test]
    fn exact_window_and_mask_boundaries_keep_exclusive_cut() {
        let source = data(2049);
        let gear = [1u64; 256];
        let gear_ls = [2u64; 256];
        for window in [2, 64, 128, 256, 512, 1024, 8192] {
            // Zero masks force the first candidate to cut, including at an exact window edge.
            for min in [64, 128, 256, 512] {
                let (_, cut, digest) = cut_gear_arr_blake3(
                    &source, min, 1024, 2048, 0, 0, 0, 0, &gear, &gear_ls, window,
                );
                assert_eq!(cut, min);
                assert_eq!(digest, blake3::hash(&source[..min]));
            }
            // No early match before center; the soft mask forces a cut at center.
            let (_, cut, digest) = cut_gear_arr_blake3(
                &source,
                64,
                256,
                2048,
                u64::MAX,
                0,
                u64::MAX,
                0,
                &gear,
                &gear_ls,
                window,
            );
            assert_eq!(cut, 256);
            assert_eq!(digest, blake3::hash(&source[..256]));
            // An always-nonzero odd Gear state prevents any match: force max_size.
            let (_, cut, digest) = cut_gear_arr_blake3(
                &source,
                64,
                256,
                2048,
                u64::MAX,
                u64::MAX,
                u64::MAX,
                u64::MAX,
                &gear,
                &gear_ls,
                window,
            );
            assert_eq!(cut, 2048);
            assert_eq!(digest, blake3::hash(&source[..2048]));
        }
    }

    struct ShortReads<'a>(&'a [u8]);
    impl Read for ShortReads<'_> {
        fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
            let count = 7.min(dst.len()).min(self.0.len());
            dst[..count].copy_from_slice(&self.0[..count]);
            self.0 = &self.0[count..];
            Ok(count)
        }
    }

    #[test]
    fn short_reads_preserve_chunks_and_eof() {
        let input = data(10_001);
        let expected = StreamCDC::new(Cursor::new(&input), 64, 256, 1024, 512)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut stream = StreamCDC::new(ShortReads(&input), 64, 256, 1024, 512).unwrap();
        let actual = stream.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(actual, expected);
        assert!(stream.next().is_none());
        assert!(stream.next().is_none());
    }

    #[test]
    fn read_errors_are_propagated() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("test read failure"))
            }
        }
        let error = StreamCDC::new(Broken, 64, 256, 1024, 512)
            .unwrap()
            .next()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("test read failure"));
    }

    #[test]
    fn invalid_windows_are_rejected() {
        for window in [0, 1, 3, 8193, usize::MAX] {
            assert!(StreamCDC::new(Cursor::new([]), 64, 256, 1024, window).is_err());
        }
    }
}
