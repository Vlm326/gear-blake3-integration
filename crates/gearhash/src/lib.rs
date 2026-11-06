//! The GEAR hashing function is a fast, rolling hash function that
//! is well suited for content defined chunking. In particular, it is
//! used as a building block for the [FastCDC](https://www.usenix.org/node/196197)
//! algorithm.
//!
//! The implementation provided in this crate consists of a simple, scalar
//! variant, as well as vectorised versions for the SSE4.2 and AVX2 instruction
//! sets on x86_64 and for NEON on aarch64, which are used automatically on
//! CPUs that support them.
//!
//! ## Example
//!
//! ```
//! fn find_all_chunks(buf: &[u8], mask: u64) -> Vec<&[u8]> {
//!     // set up initial state
//!     let mut chunks = vec![];
//!     let mut offset = 0;
//!
//!     // create new hasher
//!     let mut hasher = gearhash::Hasher::default();
//!
//!     // loop through all matches, and push the corresponding chunks
//!     while let Some(boundary) = hasher.next_match(&buf[offset..], mask) {
//!         chunks.push(&buf[offset..offset + boundary]);
//!         offset += boundary;
//!     }
//!
//!     // push final chunk
//!     chunks.push(&buf[offset..]);
//!     chunks
//! }
//! ```

#![deny(missing_docs, missing_debug_implementations)]
#![cfg_attr(fuzzing, allow(missing_docs))]
#![cfg_attr(feature = "bench", feature(test))]

#[cfg(feature = "bench")]
extern crate test;
#[cfg(feature = "bench")]
mod bench;

mod scalar;
mod simd;
mod table;

#[cfg(fuzzing)]
pub mod fuzzing;

pub use table::{DEFAULT_TABLE, Table};

/// Number of bytes per AVX2 strip iteration in the experimental scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Avx2Unroll {
    /// Original byte-at-a-time scanner.
    One,
    /// Two bytes per strip.
    Two,
    /// Four bytes per strip.
    Four,
    /// Eight bytes per strip.
    Eight,
}

/// Controlled four-byte experiments separating hash dependencies and boundary checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Avx2Experiment {
    /// Original byte-at-a-time scanner, as an external reference.
    Original,
    /// Sequential hash updates, per-byte boundary checks, four-byte loads.
    Control,
    /// Independent intermediate hashes, per-byte boundary checks.
    Unroll,
    /// Sequential hash updates, one aggregate test per four-byte group.
    Check,
    /// Independent intermediate hashes and one aggregate test per group.
    Combined,
}

/// Validated geometry of the experimental byte-at-a-time AVX2 scanner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Avx2Window {
    strip_size: usize,
    strips: usize,
}

impl Avx2Window {
    /// Create a geometry with 128/256/512/1024 bytes per strip and 2/4 strips.
    pub fn new(strip_size: usize, strips: usize) -> Option<Self> {
        if matches!(strip_size, 128 | 256 | 512 | 1024) && matches!(strips, 2 | 4) {
            Some(Self { strip_size, strips })
        } else {
            None
        }
    }
    /// Bytes per strip.
    pub fn strip_size(self) -> usize {
        self.strip_size
    }
    /// Number of independent strips.
    pub fn strips(self) -> usize {
        self.strips
    }
    /// Total bytes of a complete SIMD window.
    pub fn window_size(self) -> usize {
        self.strip_size * self.strips
    }
}

/// Gear hash state. Processes bytes to find chunk boundaries.
#[derive(Clone)]
pub struct Hasher<'t> {
    table: &'t Table,
    hash: u64,
}

impl<'t> Hasher<'t> {
    /// Create a new hasher with the given table.
    pub fn new(table: &'t Table) -> Self {
        Self { table, hash: 0 }
    }

    /// Update the hash state by processing all the bytes in the given slice.
    pub fn update(&mut self, buf: &[u8]) {
        for b in buf.iter() {
            self.hash = (self.hash << 1).wrapping_add(self.table[*b as usize]);
        }
    }

    /// Match the current hash state against the given mask.
    ///
    /// Returns true if `hash & mask == 0`, false otherwise.
    pub fn is_match(&self, mask: u64) -> bool {
        self.hash & mask == 0
    }

    /// Processes the given byte slice until a match is found for the given mask.
    ///
    /// If a match is found before the end of the byte slice, it returns the number
    /// of bytes processed. If no match has been found, it returns `None`.
    pub fn next_match(&mut self, buf: &[u8], mask: u64) -> Option<usize> {
        simd::next_match(&mut self.hash, self.table, buf, mask)
    }

    /// Scan with the selected AVX2 unroll, falling back to automatic dispatch without AVX2.
    pub fn next_match_with_avx2_unroll(
        &mut self,
        buf: &[u8],
        mask: u64,
        unroll: Avx2Unroll,
    ) -> Option<usize> {
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 was detected above.
            return unsafe {
                match unroll {
                    Avx2Unroll::One => {
                        simd::avx2::next_match(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Unroll::Two => {
                        simd::avx2::next_match_2(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Unroll::Four => {
                        simd::avx2::next_match_4(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Unroll::Eight => {
                        simd::avx2::next_match_8(&mut self.hash, self.table, buf, mask)
                    }
                }
            };
        }
        let _ = unroll;
        self.next_match(buf, mask)
    }

    /// Run a controlled AVX2 experiment; falls back to automatic dispatch without AVX2.
    pub fn next_match_with_avx2_experiment(
        &mut self,
        buf: &[u8],
        mask: u64,
        experiment: Avx2Experiment,
    ) -> Option<usize> {
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 was detected above.
            return unsafe {
                match experiment {
                    Avx2Experiment::Original => {
                        simd::avx2::next_match(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Experiment::Control => {
                        simd::avx2::next_match_control(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Experiment::Unroll => {
                        simd::avx2::next_match_unroll(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Experiment::Check => {
                        simd::avx2::next_match_check(&mut self.hash, self.table, buf, mask)
                    }
                    Avx2Experiment::Combined => {
                        simd::avx2::next_match_4(&mut self.hash, self.table, buf, mask)
                    }
                }
            };
        }
        let _ = experiment;
        self.next_match(buf, mask)
    }

    /// Scan with the selected strip geometry, falling back to automatic dispatch without AVX2.
    pub fn next_match_with_avx2_window(
        &mut self,
        buf: &[u8],
        mask: u64,
        window: Avx2Window,
    ) -> Option<usize> {
        #[cfg(target_arch = "x86_64")]
        if is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 was detected above; geometry was validated at construction.
            return unsafe {
                match (window.strip_size, window.strips) {
                    (128, 2) => {
                        simd::avx2::next_match_window2::<128>(&mut self.hash, self.table, buf, mask)
                    }
                    (256, 2) => {
                        simd::avx2::next_match_window2::<256>(&mut self.hash, self.table, buf, mask)
                    }
                    (512, 2) => {
                        simd::avx2::next_match_window2::<512>(&mut self.hash, self.table, buf, mask)
                    }
                    (1024, 2) => simd::avx2::next_match_window2::<1024>(
                        &mut self.hash,
                        self.table,
                        buf,
                        mask,
                    ),
                    (128, 4) => {
                        simd::avx2::next_match_window4::<128>(&mut self.hash, self.table, buf, mask)
                    }
                    (256, 4) => {
                        simd::avx2::next_match_window4::<256>(&mut self.hash, self.table, buf, mask)
                    }
                    (512, 4) => {
                        simd::avx2::next_match_window4::<512>(&mut self.hash, self.table, buf, mask)
                    }
                    (1024, 4) => simd::avx2::next_match_window4::<1024>(
                        &mut self.hash,
                        self.table,
                        buf,
                        mask,
                    ),
                    _ => unreachable!("validated window"),
                }
            };
        }
        let _ = window;
        self.next_match(buf, mask)
    }

    /// Retrieve the current hash value.
    pub fn get_hash(&self) -> u64 {
        self.hash
    }

    /// Set the hash value to the given integer.
    pub fn set_hash(&mut self, hash: u64) {
        self.hash = hash
    }
}

impl Default for Hasher<'static> {
    fn default() -> Self {
        Hasher::new(&DEFAULT_TABLE)
    }
}

impl<'t> std::fmt::Debug for Hasher<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Hasher").field("hash", &self.hash).finish()
    }
}
