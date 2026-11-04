use fastcdc::v2020::{
    AVERAGE_MAX, AVERAGE_MIN, Error, MAXIMUM_MAX, MAXIMUM_MIN, MINIMUM_MAX, MINIMUM_MIN,
    Normalization, get_gear_with_seed, select_masks,
};
use std::{borrow::Cow, io::Read};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct FusedChunkData {
    /// The gear hash value as of the end of the chunk.
    pub hash: u64,
    /// Starting byte position within the source.
    pub offset: u64,
    /// Length of the chunk in bytes.
    pub length: usize,
    /// Source bytes contained in this chunk.
    pub data: Vec<u8>,
    /// Дайджест BLAKE3, вычисленный совмещённым сканером.
    pub blake_hash: blake3::Hash,
}

pub struct StreamCDC<R: Read> {
    /// Buffer of data from source for finding cut points.
    buffer: Vec<u8>,
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
    pub fn new(source: R, min_size: usize, avg_size: usize, max_size: usize) -> Self {
        StreamCDC::with_level(source, min_size, avg_size, max_size, Normalization::Level1)
    }

    ///
    /// Create a new [`StreamCDC`] with the given normalization level.
    ///
    pub fn with_level(
        source: R,
        min_size: usize,
        avg_size: usize,
        max_size: usize,
        level: Normalization,
    ) -> Self {
        StreamCDC::with_level_and_seed(source, min_size, avg_size, max_size, level, 0)
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
    ) -> Self {
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
        Self {
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
        }
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
                    blake_hash: blake_hash,
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
) -> (u64, usize, blake3::Hash) {
    let gear: &[u64; 256] = gear.try_into().expect("GEAR table must have 256 entries");
    let gear_ls: &[u64; 256] = gear_ls
        .try_into()
        .expect("GEAR_LS table must have 256 entries");
    cut_gear_arr_blake3(
        source, min_size, avg_size, max_size, mask_s, mask_l, mask_s_ls, mask_l_ls, gear, gear_ls,
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
) -> (u64, usize, blake3::Hash) {
    let blake3_batch: usize = std::env::var("BLAKE3_BATCH")
        .unwrap_or_else(|_| "64".to_string())
        .parse()
        .unwrap();
    // Хешер и позиция уже переданных байтов нужны, чтобы порционные обновления
    // BLAKE3 шли строго последовательно и не включали байты следующего чанка.
    let mut blake_hasher = blake3::Hasher::new();
    let mut blake_fed_until = 0usize;

    let mut remaining = source.len();
    if remaining <= min_size {
        // Для короткого остатка граница принудительно ставится в конце данных;
        // весь остаток сразу становится отдельным чанком.
        blake_hasher.update(source);
        let digest = blake_hasher.finalize();
        return (0, remaining, digest);
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
    let mut hash: u64 = 0;
    let center = center & !1;
    let end = remaining & !1;
    let mut scan_start = min_size & !1;

    // До среднего размера FastCDC применяет строгую маску mask_s.
    if scan_start < center {
        if let Some(offset) = scan_region(
            src, scan_start, center, &mut hash, mask_s_ls, mask_s, gear, gear_ls,
        ) {
            let digest =
                finish_blake_hasher_at(src, offset, &mut blake_hasher, &mut blake_fed_until);
            return (hash, offset, digest);
        }
        blake3_fed_until(
            src,
            &mut blake_hasher,
            &mut blake_fed_until,
            center,
            blake3_batch,
        );
        scan_start = center;
    }

    // После среднего размера используется более мягкая маска mask_l; диапазон
    // сканирования продолжается до конца доступного окна или max_size.
    if scan_start < end
        && let Some(offset) = scan_region(
            src, scan_start, end, &mut hash, mask_l_ls, mask_l, gear, gear_ls,
        )
    {
        let digest = finish_blake_hasher_at(src, offset, &mut blake_hasher, &mut blake_fed_until);
        return (hash, offset, digest);
    }
    blake3_fed_until(
        src,
        &mut blake_hasher,
        &mut blake_fed_until,
        end,
        blake3_batch,
    );

    // Если Gear не нашёл кандидата, FastCDC принудительно завершает чанк на
    // максимальной доступной длине. Нечётный последний байт участвует в Gear-
    // отпечатке, но не проверяется как отдельный кандидат границы.
    if remaining % 2 == 1 {
        hash = (hash << 1).wrapping_add(gear[src[remaining - 1] as usize]);
    }
    let digest = finish_blake_hasher_at(src, remaining, &mut blake_hasher, &mut blake_fed_until);
    (hash, remaining, digest)
}

/// Передаёт в BLAKE3 ещё не обработанный хвост и завершает дайджест ровно на
/// границе чанка, найденной Gear.
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

/// Проверяет четыре последовательных кандидата одним AVX2-сравнением.
/// Состояния на первом байте каждой пары удвоены, как в скалярном FastCDC.
#[allow(clippy::too_many_arguments)]
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scan_region_avx2(
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
    let (blocks, _) = source[start..end].as_chunks::<4>();
    let mut current_hash = *hash;

    // set_epi64x принимает дорожки от старшей к младшей.
    let masks = _mm256_set_epi64x(mask as i64, mask_ls as i64, mask as i64, mask_ls as i64);
    let zero = _mm256_setzero_si256();

    for bytes in blocks {
        let g0 = gear_ls[bytes[0] as usize];
        let g1 = gear[bytes[1] as usize];
        let g2 = gear_ls[bytes[2] as usize];
        let g3 = gear[bytes[3] as usize];

        // Формулы кандидатов раскрыты относительно одного входного hash:
        // h0 =  4h + g0
        // h1 =  4h + g0 + g1
        // h2 = 16h + 4(g0 + g1) + g2
        // h3 = 16h + 4(g0 + g1) + g2 + g3
        // Вклады двух пар вычисляются независимо; после base4_pair дорожки
        // h2 и h3 также получают свои последние слагаемые независимо.
        let pair01 = g0.wrapping_add(g1);
        let pair23 = g2.wrapping_add(g3);
        let base2 = current_hash << 2;
        let base4_pair = (current_hash << 4).wrapping_add(pair01 << 2);

        // Дорожки от младшей к старшей: [h0, h1, h2, h3].
        let bases = _mm256_set_epi64x(
            base4_pair as i64,
            base4_pair as i64,
            base2 as i64,
            base2 as i64,
        );
        let contributions = _mm256_set_epi64x(pair23 as i64, g2 as i64, pair01 as i64, g0 as i64);
        let states = _mm256_add_epi64(bases, contributions);

        let masked = _mm256_and_si256(states, masks);
        let equal_zero = _mm256_cmpeq_epi64(masked, zero);
        let matches = _mm256_movemask_pd(_mm256_castsi256_pd(equal_zero)) as u32;

        if matches != 0 {
            let lane = matches.trailing_zeros() as usize;
            *hash = match lane {
                0 => _mm256_extract_epi64::<0>(states) as u64,
                1 => _mm256_extract_epi64::<1>(states) as u64,
                2 => _mm256_extract_epi64::<2>(states) as u64,
                3 => _mm256_extract_epi64::<3>(states) as u64,
                _ => unreachable!(),
            };
            return Some(offset + lane);
        }

        // Gear всё равно связывает соседние блоки через состояние после h3.
        current_hash = base4_pair.wrapping_add(pair23);
        offset += 4;
    }

    *hash = current_hash;
    if offset < end {
        return scan_region_scalar(source, offset, end, hash, mask_ls, mask, gear, gear_ls);
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn scan_region_scalar(
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
    #[cfg(target_arch = "x86_64")]
    if is_x86_feature_detected!("avx2") {
        // SAFETY: поддержка AVX2 проверена перед вызовом.
        return unsafe { scan_region_avx2(source, start, end, hash, mask_ls, mask, gear, gear_ls) };
    }
    scan_region_scalar(source, start, end, hash, mask_ls, mask, gear, gear_ls)
}

#[inline]
fn blake3_fed_until(
    source: &[u8],
    blake_hasher: &mut blake3::Hasher,
    blake_fed_until: &mut usize,
    cut: usize,
    batch: usize,
) {
    if cut - *blake_fed_until >= batch {
        blake_hasher.update(&source[*blake_fed_until..cut]);
        *blake_fed_until = cut;
    }
}

#[cfg(test)]
mod tests {
    use super::StreamCDC as FusedStreamCDC;
    #[cfg(target_arch = "x86_64")]
    use super::{scan_region_avx2, scan_region_scalar};
    use fastcdc::v2020::StreamCDC as ReferenceStreamCDC;
    use fastcdc::v2020::{Normalization, get_gear_with_seed, select_masks};
    use std::io::Cursor;

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn avx2_returns_first_matching_lane_and_its_hash() {
        if !is_x86_feature_detected!("avx2") {
            eprintln!("skipping AVX2 test: unsupported CPU");
            return;
        }

        let source = [255, 255, 0, 1, 2, 3, 255];
        // With G[b] = 1 and masks 1/2, none of the four candidates matches.
        // Setting G[lane] = 0 makes that candidate the first match.
        for expected_lane in 0..4 {
            let mut gear = [1u64; 256];
            gear[expected_lane] = 0;
            let gear_ls = gear.map(|value| value << 1);
            let mut expected_hash = 0;
            let mut actual_hash = 0;
            let expected =
                scan_region_scalar(&source, 2, 6, &mut expected_hash, 2, 1, &gear, &gear_ls);
            // SAFETY: AVX2 was detected above.
            let actual =
                unsafe { scan_region_avx2(&source, 2, 6, &mut actual_hash, 2, 1, &gear, &gear_ls) };
            assert_eq!(expected, Some(2 + expected_lane));
            assert_eq!(actual, expected);
            assert_eq!(actual_hash, expected_hash);
        }

        // All candidates match: choose the first, including its shifted state.
        let gear = [1u64; 256];
        let gear_ls = gear.map(|value| value << 1);
        let mut hash = 7;
        // SAFETY: AVX2 was detected above.
        let cut = unsafe { scan_region_avx2(&source, 2, 6, &mut hash, 0, 0, &gear, &gear_ls) };
        assert_eq!(cut, Some(2));
        assert_eq!(hash, (7 << 2) + 2);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn avx2_matches_scalar_across_regions_masks_and_initial_states() {
        if !is_x86_feature_detected!("avx2") {
            eprintln!("skipping AVX2 test: unsupported CPU");
            return;
        }

        let mut state = 0x1234_5678_9abc_def0u64;
        let source: Vec<u8> = (0..132)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        let (strict, relaxed) = select_masks(256, Normalization::Level1);

        for seed in [0, 42, u64::MAX] {
            let (gear, gear_ls) = get_gear_with_seed(seed);
            let gear: &[u64; 256] = gear.as_ref().try_into().unwrap();
            let gear_ls: &[u64; 256] = gear_ls.as_ref().try_into().unwrap();
            for mask in [0, 1, 0xff, strict, relaxed, u64::MAX] {
                for initial_hash in [0, 1, state, u64::MAX] {
                    for start in 0..4 {
                        // Cover full blocks, leftover pairs and ignored odd tails.
                        for length in 0..=128 {
                            let end = start + length;
                            let mut expected_hash = initial_hash;
                            let mut actual_hash = initial_hash;
                            let expected = scan_region_scalar(
                                &source,
                                start,
                                end,
                                &mut expected_hash,
                                mask << 1,
                                mask,
                                gear,
                                gear_ls,
                            );
                            // SAFETY: AVX2 was detected above.
                            let actual = unsafe {
                                scan_region_avx2(
                                    &source,
                                    start,
                                    end,
                                    &mut actual_hash,
                                    mask << 1,
                                    mask,
                                    gear,
                                    gear_ls,
                                )
                            };
                            assert_eq!(
                                (actual, actual_hash),
                                (expected, expected_hash),
                                "seed={seed}, mask={mask:#x}, initial={initial_hash:#x}, start={start}, length={length}",
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fused_stream_matches_reference_chunks_and_digests() {
        // Векторы проверяют короткий остаток, нечётный хвост, принудительный
        // max_size и обычные разрезы, найденные масками Gear.
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
            let actual = FusedStreamCDC::new(Cursor::new(input.as_slice()), 64, 256, 1024)
                .map(Result::unwrap)
                .collect::<Vec<_>>();

            assert_eq!(actual.len(), expected.len());
            for (fused, reference) in actual.iter().zip(&expected) {
                assert_eq!(fused.hash, reference.hash, "Gear hash differs");
                assert_eq!(fused.offset, reference.offset, "chunk offset differs");
                assert_eq!(fused.length, reference.length, "chunk length differs");
                assert_eq!(fused.data, reference.data, "chunk bytes differ");
                assert_eq!(
                    fused.blake_hash,
                    blake3::hash(&reference.data),
                    "BLAKE3 digest differs"
                );
            }
        }
    }
}
