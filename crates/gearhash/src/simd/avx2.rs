use core::arch::x86_64::*;

use crate::Table;

const CHUNK_SIZE: usize = 1024;
const STRIP_SIZE: usize = CHUNK_SIZE / 4;

#[target_feature(enable = "avx2")]
pub unsafe fn next_match(hash: &mut u64, table: &Table, buf: &[u8], mask: u64) -> Option<usize> {
    for (ic, chunk) in buf.chunks(CHUNK_SIZE).enumerate() {
        if chunk.len() != CHUNK_SIZE {
            return crate::scalar::next_match(hash, table, chunk, mask)
                .map(|off| off + ic * CHUNK_SIZE);
        }

        let mut h = _mm256_setzero_si256();

        for i in 0..64 {
            // SAFETY: `chunk` is exactly `CHUNK_SIZE` bytes long (checked above), and the
            // largest index read here is `STRIP_SIZE * 3 - 64 + i` < `CHUNK_SIZE` for `i < 64`.
            let (b1, b2, b3) = unsafe {
                (
                    *chunk.get_unchecked((STRIP_SIZE - 64) + i),
                    *chunk.get_unchecked((STRIP_SIZE * 2 - 64) + i),
                    *chunk.get_unchecked((STRIP_SIZE * 3 - 64) + i),
                )
            };

            let g = _mm256_set_epi64x(
                0,
                table[b1 as usize] as i64,
                table[b2 as usize] as i64,
                table[b3 as usize] as i64,
            );

            h = _mm256_add_epi64(_mm256_slli_epi64(h, 1), g);
        }

        h = _mm256_insert_epi64(h, *hash as i64, 3);

        let mut pre_off = usize::MAX;
        let mut pre_hash = 0u64;

        for i in 0..STRIP_SIZE {
            // SAFETY: `chunk` is exactly `CHUNK_SIZE` bytes long (checked above), and the
            // largest index read here is `STRIP_SIZE * 3 + i` < `CHUNK_SIZE` for `i < STRIP_SIZE`.
            let (b0, b1, b2, b3) = unsafe {
                (
                    *chunk.get_unchecked(i),
                    *chunk.get_unchecked(STRIP_SIZE + i),
                    *chunk.get_unchecked(STRIP_SIZE * 2 + i),
                    *chunk.get_unchecked(STRIP_SIZE * 3 + i),
                )
            };

            let g = _mm256_set_epi64x(
                table[b0 as usize] as i64,
                table[b1 as usize] as i64,
                table[b2 as usize] as i64,
                table[b3 as usize] as i64,
            );

            h = _mm256_add_epi64(_mm256_slli_epi64(h, 1), g);

            let m = _mm256_and_si256(h, _mm256_set1_epi64x(mask as i64));
            let c = _mm256_cmpeq_epi64(m, _mm256_setzero_si256());
            let z = _mm256_movemask_epi8(c) as u32;

            if z == 0 {
                continue;
            }

            if z & (1u32 << 24) != 0 {
                *hash = _mm256_extract_epi64(h, 3) as u64;
                return Some(ic * CHUNK_SIZE + i + 1);
            }

            // If we find a match in the second strip, fall back to the scalar implementation to
            // see if we can find an earlier match in the first strip.
            if z & (1u32 << 16) != 0 {
                let rest = &chunk[i + 1..STRIP_SIZE];
                *hash = _mm256_extract_epi64(h, 3) as u64;
                if let Some(off) = crate::scalar::next_match(hash, table, rest, mask) {
                    return Some(ic * CHUNK_SIZE + i + 1 + off);
                } else {
                    *hash = _mm256_extract_epi64(h, 2) as u64;
                    return Some(ic * CHUNK_SIZE + STRIP_SIZE + i + 1);
                }
            }

            if z & (1u32 << 8) != 0 {
                let off = STRIP_SIZE * 2 + i;
                if off < pre_off {
                    pre_off = off;
                    pre_hash = _mm256_extract_epi64(h, 1) as u64;
                }
            }

            if z & (1u32) != 0 {
                let off = STRIP_SIZE * 3 + i;
                if off < pre_off {
                    pre_off = off;
                    pre_hash = _mm256_extract_epi64(h, 0) as u64;
                }
            }
        }

        if pre_off != usize::MAX {
            *hash = pre_hash;
            return Some(ic * CHUNK_SIZE + pre_off + 1);
        }

        *hash = _mm256_extract_epi64(h, 0) as u64;
    }

    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn agrees_with_scalar() {
        if !is_x86_feature_detected!("avx2") {
            assert_ne!(
                std::env::var("GEARHASH_REQUIRE_SIMD").as_deref(),
                Ok("1"),
                "avx2 is unavailable but GEARHASH_REQUIRE_SIMD is set"
            );
            eprintln!("skipping: avx2 is not available on this CPU");
            return;
        }

        fn prop(seed: u64, mask: u64) -> bool {
            // SAFETY: `prop` is only reached once the `avx2` target feature has been
            // detected on this CPU.
            crate::simd::tests::agrees_with_scalar(seed, mask, |hash, table, buf, mask| unsafe {
                super::next_match(hash, table, buf, mask)
            })
        }

        quickcheck::QuickCheck::new().quickcheck(prop as fn(u64, u64) -> bool);
    }
}

#[cfg(feature = "bench")]
#[bench]
fn throughput(b: &mut test::Bencher) {
    if is_x86_feature_detected!("avx2") {
        // SAFETY: the `avx2` target feature was just detected on this CPU.
        crate::bench::throughput(b, |hash, buf, mask| unsafe {
            next_match(hash, &crate::DEFAULT_TABLE, buf, mask)
        })
    }
}

// Each specialization uses immediate shifts, with one carried update per group.
macro_rules! unrolled {
    ($name:ident, $word:ty, $unroll:expr, $sequential:literal, $aggregate:literal; $($shift:literal),+) => {
        #[target_feature(enable = "avx2")]
        pub unsafe fn $name(hash: &mut u64, table: &Table, buf: &[u8], mask: u64) -> Option<usize> {
            for (ic, chunk) in buf.chunks(CHUNK_SIZE).enumerate() {
                if chunk.len() != CHUNK_SIZE {
                    return crate::scalar::next_match(hash, table, chunk, mask)
                        .map(|off| off + ic * CHUNK_SIZE);
                }

                let mut h = _mm256_setzero_si256();

                for i in 0..64 {
                    // SAFETY: `chunk` is exactly `CHUNK_SIZE` bytes long (checked above), and the
                    // largest index read here is `STRIP_SIZE * 3 - 64 + i` < `CHUNK_SIZE` for `i < 64`.
                    let (b1, b2, b3) = unsafe {
                        (
                            *chunk.get_unchecked((STRIP_SIZE - 64) + i),
                            *chunk.get_unchecked((STRIP_SIZE * 2 - 64) + i),
                            *chunk.get_unchecked((STRIP_SIZE * 3 - 64) + i),
                        )
                    };

                    let g = _mm256_set_epi64x(
                        0,
                        table[b1 as usize] as i64,
                        table[b2 as usize] as i64,
                        table[b3 as usize] as i64,
                    );

                    h = _mm256_add_epi64(_mm256_slli_epi64(h, 1), g);
                }

                h = _mm256_insert_epi64(h, *hash as i64, 3);

                let mut pre_off = usize::MAX;
                let mut pre_hash = 0u64;

                let maskv = _mm256_set1_epi64x(mask as i64);
                for i in (0..STRIP_SIZE).step_by($unroll) {
                    // SAFETY: full chunk; i + UNROLL <= STRIP_SIZE in every strip.
                    let mut words = unsafe {
                        let ptr = chunk.as_ptr();
                        [
                            ptr.add(i).cast::<$word>().read_unaligned().to_le(),
                            ptr.add(STRIP_SIZE + i).cast::<$word>().read_unaligned().to_le(),
                            ptr.add(STRIP_SIZE * 2 + i).cast::<$word>().read_unaligned().to_le(),
                            ptr.add(STRIP_SIZE * 3 + i).cast::<$word>().read_unaligned().to_le(),
                        ]
                    };
                    let mut a = [0u64; 4];
                    let mut states = [_mm256_setzero_si256(); $unroll];
                    let mut combined = _mm256_setzero_si256();
                    $(
                        for lane in 0..4 {
                            a[lane] = if $sequential { table[words[lane] as u8 as usize] }
                                else { (a[lane] << 1).wrapping_add(table[words[lane] as u8 as usize]) };
                            words[lane] = words[lane].wrapping_shr(8);
                        }
                        let mut p = _mm256_set_epi64x(a[0] as i64, a[1] as i64, a[2] as i64, a[3] as i64);
                        // Keep P opaque: LLVM otherwise splits it into additions on H.
                        // SAFETY: this empty identity asm preserves the initialized vector;
                        // AVX2 is enabled, and no memory or stack is accessed.
                        unsafe { core::arch::asm!("/* {p} */", p = inout(ymm_reg) p,
                            options(pure, nomem, nostack, preserves_flags)); }
                        let hk = if $sequential {
                            let prev = if $shift == 1 { h }
                                else { states[($shift as usize).saturating_sub(2)] };
                            _mm256_add_epi64(_mm256_slli_epi64::<1>(prev), p)
                        } else { _mm256_add_epi64(_mm256_slli_epi64::<$shift>(h), p) };
                        states[$shift - 1] = hk;
                        if $aggregate {
                            combined = _mm256_or_si256(combined, _mm256_cmpeq_epi64(
                                _mm256_and_si256(hk, maskv), _mm256_setzero_si256()));
                        }
                    )+
                    h = states[$unroll - 1];
                    if $aggregate && _mm256_testz_si256(combined, combined) != 0 { continue; }
                    for (k, &hk) in states.iter().enumerate() {
                        let i = i + k;
                        let h = hk;
                        let z = _mm256_movemask_epi8(_mm256_cmpeq_epi64(
                            _mm256_and_si256(h, maskv), _mm256_setzero_si256())) as u32;
                        if z == 0 {
                            continue;
                        }

                        if z & (1u32 << 24) != 0 {
                            *hash = _mm256_extract_epi64(h, 3) as u64;
                            return Some(ic * CHUNK_SIZE + i + 1);
                        }

                        // If we find a match in the second strip, fall back to the scalar implementation to
                        // see if we can find an earlier match in the first strip.
                        if z & (1u32 << 16) != 0 {
                            let rest = &chunk[i + 1..STRIP_SIZE];
                            *hash = _mm256_extract_epi64(h, 3) as u64;
                            if let Some(off) = crate::scalar::next_match(hash, table, rest, mask) {
                                return Some(ic * CHUNK_SIZE + i + 1 + off);
                            } else {
                                *hash = _mm256_extract_epi64(h, 2) as u64;
                                return Some(ic * CHUNK_SIZE + STRIP_SIZE + i + 1);
                            }
                        }

                        if z & (1u32 << 8) != 0 {
                            let off = STRIP_SIZE * 2 + i;
                            if off < pre_off {
                                pre_off = off;
                                pre_hash = _mm256_extract_epi64(h, 1) as u64;
                            }
                        }

                        if z & (1u32) != 0 {
                            let off = STRIP_SIZE * 3 + i;
                            if off < pre_off {
                                pre_off = off;
                                pre_hash = _mm256_extract_epi64(h, 0) as u64;
                            }
                        }
                    }
                }

                if pre_off != usize::MAX {
                    *hash = pre_hash;
                    return Some(ic * CHUNK_SIZE + pre_off + 1);
                }

                *hash = _mm256_extract_epi64(h, 0) as u64;
            }

            None
        }
    };
}
unrolled!(next_match_2, u16, 2, false, true; 1, 2);
unrolled!(next_match_4, u32, 4, false, true; 1, 2, 3, 4);
unrolled!(next_match_8, u64, 8, false, true; 1, 2, 3, 4, 5, 6, 7, 8);

// Same word loads and four-byte loop for the factorial comparison.
unrolled!(next_match_control, u32, 4, true, false; 1, 2, 3, 4);
unrolled!(next_match_check, u32, 4, true, true; 1, 2, 3, 4);
unrolled!(next_match_unroll, u32, 4, false, false; 1, 2, 3, 4);

#[cfg(test)]
mod unroll_tests {
    use super::*;

    #[test]
    fn all_unrolls_match_scalar() {
        if !is_x86_feature_detected!("avx2") {
            assert_ne!(std::env::var("GEARHASH_REQUIRE_SIMD").as_deref(), Ok("1"));
            return;
        }
        for scan in [
            next_match,
            next_match_2,
            next_match_4,
            next_match_8,
            next_match_control,
            next_match_check,
            next_match_unroll,
        ] {
            for seed in 0..32u64 {
                for mask in [
                    0,
                    1,
                    0xff,
                    0x0000d90003530000,
                    u64::MAX,
                    seed.wrapping_mul(0x9e3779b97f4a7c15),
                ] {
                    assert!(crate::simd::tests::agrees_with_scalar(
                        seed,
                        mask,
                        |h, t, b, m| {
                            // SAFETY: AVX2 was detected above.
                            unsafe { scan(h, t, b, m) }
                        }
                    ));
                }
            }
            for table in [[0; 256], [1; 256], [u64::MAX; 256], crate::DEFAULT_TABLE] {
                for len in [
                    0, 1, 2, 3, 4, 7, 8, 9, 255, 256, 257, 1023, 1024, 1025, 3073,
                ] {
                    let bytes: Vec<_> = (0..len + 1).map(|i| (i * 71) as u8).collect();
                    for mask in [0, 1, 0xff, u64::MAX] {
                        let mut expected = 0xfedcba9876543210;
                        let mut actual = expected;
                        let off =
                            crate::scalar::next_match(&mut expected, &table, &bytes[1..], mask);
                        // SAFETY: AVX2 was detected above; buffer may be unaligned.
                        let result = unsafe { scan(&mut actual, &table, &bytes[1..], mask) };
                        assert_eq!((result, actual), (off, expected), "len={len} mask={mask}");
                    }
                }
            }
            // A table with only one even entry makes each marked byte a match for mask 1.
            // Competing candidates exercise strip priority and every unrolled position.
            let mut table = [1; 256];
            table[0] = 0;
            for strip in 0..4 {
                for k in 0..8 {
                    for competing in [false, true] {
                        let mut bytes = vec![1; 2049];
                        bytes[strip * STRIP_SIZE + 24 + k] = 0;
                        // A later byte in an earlier strip must beat an earlier SIMD iteration.
                        if competing && strip > 0 {
                            bytes[(strip - 1) * STRIP_SIZE + 99] = 0;
                        }
                        let mut expected = 7;
                        let mut actual = expected;
                        let off = crate::scalar::next_match(&mut expected, &table, &bytes, 1);
                        // SAFETY: AVX2 was detected above.
                        let result = unsafe { scan(&mut actual, &table, &bytes, 1) };
                        assert_eq!((result, actual), (off, expected));
                    }
                }
            }
        }
    }
}

// Window-size experiments preserve the original byte-at-a-time algorithm.
#[target_feature(enable = "avx2")]
pub unsafe fn next_match_window4<const STRIP: usize>(
    hash: &mut u64,
    table: &Table,
    buf: &[u8],
    mask: u64,
) -> Option<usize> {
    const {
        assert!(STRIP >= 64);
    }
    for (ic, chunk) in buf.chunks(STRIP * 4).enumerate() {
        if chunk.len() != (STRIP * 4) {
            return crate::scalar::next_match(hash, table, chunk, mask)
                .map(|off| off + ic * (STRIP * 4));
        }

        let mut h = _mm256_setzero_si256();

        for i in 0..64 {
            // SAFETY: `chunk` is exactly `(STRIP * 4)` bytes long (checked above), and the
            // largest index read here is `STRIP * 3 - 64 + i` < `(STRIP * 4)` for `i < 64`.
            let (b1, b2, b3) = unsafe {
                (
                    *chunk.get_unchecked((STRIP - 64) + i),
                    *chunk.get_unchecked((STRIP * 2 - 64) + i),
                    *chunk.get_unchecked((STRIP * 3 - 64) + i),
                )
            };

            let g = _mm256_set_epi64x(
                0,
                table[b1 as usize] as i64,
                table[b2 as usize] as i64,
                table[b3 as usize] as i64,
            );

            h = _mm256_add_epi64(_mm256_slli_epi64(h, 1), g);
        }

        h = _mm256_insert_epi64(h, *hash as i64, 3);

        let mut pre_off = usize::MAX;
        let mut pre_hash = 0u64;

        for i in 0..STRIP {
            // SAFETY: `chunk` is exactly `(STRIP * 4)` bytes long (checked above), and the
            // largest index read here is `STRIP * 3 + i` < `(STRIP * 4)` for `i < STRIP`.
            let (b0, b1, b2, b3) = unsafe {
                (
                    *chunk.get_unchecked(i),
                    *chunk.get_unchecked(STRIP + i),
                    *chunk.get_unchecked(STRIP * 2 + i),
                    *chunk.get_unchecked(STRIP * 3 + i),
                )
            };

            let g = _mm256_set_epi64x(
                table[b0 as usize] as i64,
                table[b1 as usize] as i64,
                table[b2 as usize] as i64,
                table[b3 as usize] as i64,
            );

            h = _mm256_add_epi64(_mm256_slli_epi64(h, 1), g);

            let m = _mm256_and_si256(h, _mm256_set1_epi64x(mask as i64));
            let c = _mm256_cmpeq_epi64(m, _mm256_setzero_si256());
            let z = _mm256_movemask_epi8(c) as u32;

            if z == 0 {
                continue;
            }

            if z & (1u32 << 24) != 0 {
                *hash = _mm256_extract_epi64(h, 3) as u64;
                return Some(ic * (STRIP * 4) + i + 1);
            }

            // If we find a match in the second strip, fall back to the scalar implementation to
            // see if we can find an earlier match in the first strip.
            if z & (1u32 << 16) != 0 {
                let rest = &chunk[i + 1..STRIP];
                *hash = _mm256_extract_epi64(h, 3) as u64;
                if let Some(off) = crate::scalar::next_match(hash, table, rest, mask) {
                    return Some(ic * (STRIP * 4) + i + 1 + off);
                } else {
                    *hash = _mm256_extract_epi64(h, 2) as u64;
                    return Some(ic * (STRIP * 4) + STRIP + i + 1);
                }
            }

            if z & (1u32 << 8) != 0 {
                let off = STRIP * 2 + i;
                if off < pre_off {
                    pre_off = off;
                    pre_hash = _mm256_extract_epi64(h, 1) as u64;
                }
            }

            if z & (1u32) != 0 {
                let off = STRIP * 3 + i;
                if off < pre_off {
                    pre_off = off;
                    pre_hash = _mm256_extract_epi64(h, 0) as u64;
                }
            }
        }

        if pre_off != usize::MAX {
            *hash = pre_hash;
            return Some(ic * (STRIP * 4) + pre_off + 1);
        }

        *hash = _mm256_extract_epi64(h, 0) as u64;
    }

    None
}

#[target_feature(enable = "avx2")]
pub unsafe fn next_match_window2<const STRIP: usize>(
    hash: &mut u64,
    table: &Table,
    buf: &[u8],
    mask: u64,
) -> Option<usize> {
    const {
        assert!(STRIP >= 64);
    }
    for (ic, chunk) in buf.chunks(STRIP * 2).enumerate() {
        if chunk.len() != (STRIP * 2) {
            return crate::scalar::next_match(hash, table, chunk, mask)
                .map(|off| off + ic * (STRIP * 2));
        }

        let mut hx = 0u64;
        for i in 0..64 {
            // SAFETY: `chunk` is exactly `(STRIP * 2)` bytes long (checked above), and
            // `STRIP - 64 + i` < `(STRIP * 2)` for `i < 64`.
            let b = unsafe { *chunk.get_unchecked((STRIP - 64) + i) };
            hx = (hx << 1).wrapping_add(table[b as usize]);
        }

        let mut h = _mm_set_epi64x(*hash as i64, hx as i64);

        for i in 0..STRIP {
            // SAFETY: `chunk` is exactly `(STRIP * 2)` bytes long (checked above), and the
            // largest index read here is `STRIP + i` < `(STRIP * 2)` for `i < STRIP`.
            let (b0, b1) = unsafe { (*chunk.get_unchecked(i), *chunk.get_unchecked(STRIP + i)) };

            let g = _mm_set_epi64x(table[b0 as usize] as i64, table[b1 as usize] as i64);

            h = _mm_add_epi64(_mm_slli_epi64(h, 1), g);

            let m = _mm_and_si128(h, _mm_set1_epi64x(mask as i64));
            let c = _mm_cmpeq_epi64(m, _mm_setzero_si128());
            let z = _mm_movemask_epi8(c) as u32;

            if z == 0 {
                continue;
            }

            if z & (1u32 << 8) != 0 {
                *hash = _mm_extract_epi64(h, 1) as u64;
                return Some(ic * (STRIP * 2) + i + 1);
            }

            // If we find a match in the second strip, fall back to the scalar implementation to
            // see if we can find an earlier match in the first strip.
            if z & 1u32 != 0 {
                let rest = &chunk[i + 1..STRIP];
                *hash = _mm_extract_epi64(h, 1) as u64;
                if let Some(off) = crate::scalar::next_match(hash, table, rest, mask) {
                    return Some(ic * (STRIP * 2) + i + 1 + off);
                } else {
                    *hash = _mm_extract_epi64(h, 0) as u64;
                    return Some(ic * (STRIP * 2) + STRIP + i + 1);
                }
            }
        }

        *hash = _mm_extract_epi64(h, 0) as u64;
    }

    None
}

#[cfg(test)]
mod window_tests {
    use crate::{Avx2Window, DEFAULT_TABLE, Hasher};

    #[test]
    fn geometries_preserve_boundaries_and_hashes() {
        if !is_x86_feature_detected!("avx2") {
            assert_ne!(std::env::var("GEARHASH_REQUIRE_SIMD").as_deref(), Ok("1"));
            return;
        }
        for size in [128, 256, 512, 1024] {
            for strips in [2, 4] {
                let geometry = Avx2Window::new(size, strips).unwrap();
                for seed in 0..8 {
                    for mask in [0, 1, 0xff, 0x0000d90003530000, u64::MAX] {
                        assert!(crate::simd::tests::agrees_with_scalar(
                            seed,
                            mask,
                            |hash, table, buf, mask| {
                                let mut h = Hasher::new(table);
                                h.set_hash(*hash);
                                let result = h.next_match_with_avx2_window(buf, mask, geometry);
                                *hash = h.get_hash();
                                result
                            }
                        ));
                    }
                }
                let window = geometry.window_size();
                let mut state = 12345u64;
                let bytes: Vec<_> = (0..window * 3 + 2)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        state as u8
                    })
                    .collect();
                for len in [
                    0,
                    1,
                    63,
                    64,
                    65,
                    size - 1,
                    size,
                    size + 1,
                    window - 1,
                    window,
                    window + 1,
                    window * 3 + 1,
                ] {
                    for table in [DEFAULT_TABLE, [1; 256], [u64::MAX; 256], [0; 256]] {
                        for mask in [0, 1, 0xff, u64::MAX] {
                            let mut expected = u64::MAX;
                            let mut h = Hasher::new(&table);
                            h.set_hash(expected);
                            let buf = &bytes[1..1 + len];
                            let off = crate::scalar::next_match(&mut expected, &table, buf, mask);
                            assert_eq!(
                                (
                                    h.next_match_with_avx2_window(buf, mask, geometry),
                                    h.get_hash()
                                ),
                                (off, expected),
                                "{geometry:?} len={len}"
                            );
                        }
                    }
                }
                // A later byte in an earlier strip must beat an earlier byte in a later strip.
                let mut table = [1; 256];
                table[0] = 0;
                for target in 0..strips {
                    for position in [0, 1, size - 1] {
                        for competing in [false, true] {
                            let mut buf = vec![1; window * 2];
                            buf[target * size + position] = 0;
                            if competing && target > 0 {
                                buf[(target - 1) * size + size - 1] = 0;
                            }
                            let mut expected = 7;
                            let mut h = Hasher::new(&table);
                            h.set_hash(expected);
                            let off = crate::scalar::next_match(&mut expected, &table, &buf, 1);
                            assert_eq!(
                                (
                                    h.next_match_with_avx2_window(&buf, 1, geometry),
                                    h.get_hash()
                                ),
                                (off, expected)
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn geometry_rejects_unsupported_values() {
        for size in [0, 64, 129, 2048] {
            assert!(Avx2Window::new(size, 4).is_none());
        }
        for strips in [0, 1, 3, 8] {
            assert!(Avx2Window::new(256, strips).is_none());
        }
    }
}
