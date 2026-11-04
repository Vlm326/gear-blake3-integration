//! FastCDC-compatible cuts using the local gearhash SIMD scanner.
use fastcdc::v2020::{self, Normalization, get_gear_with_seed, select_masks};
use std::{
    hint::black_box,
    io::{self, Read},
};

#[derive(Clone, Copy)]
struct Config {
    min: usize,
    avg: usize,
    max: usize,
    window: usize,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl Config {
    fn new(min: usize, avg: usize, max: usize, batch: usize) -> io::Result<Self> {
        if !(v2020::MINIMUM_MIN..=v2020::MINIMUM_MAX).contains(&min)
            || !(v2020::AVERAGE_MIN..=v2020::AVERAGE_MAX).contains(&avg)
            || !(v2020::MAXIMUM_MIN..=v2020::MAXIMUM_MAX).contains(&max)
            || min > avg
            || avg > max
            || !min.is_multiple_of(2)
            || !avg.is_multiple_of(2)
            || !max.is_multiple_of(2)
        {
            return Err(invalid("use even FastCDC sizes with min <= avg <= max"));
        }
        if batch == 0 {
            return Err(invalid("BLAKE3_BATCH must be positive"));
        }
        let window = batch
            .checked_add(1023)
            .ok_or_else(|| invalid("BLAKE3_BATCH is too large"))?
            / 1024
            * 1024;
        Ok(Self {
            min,
            avg,
            max,
            window,
        })
    }

    fn from_env() -> io::Result<Self> {
        fn number(name: &str, default: usize) -> io::Result<usize> {
            match std::env::var(name) {
                Ok(value) => value.parse().map_err(|_| invalid(name)),
                Err(std::env::VarError::NotPresent) => Ok(default),
                Err(_) => Err(invalid(name)),
            }
        }
        Self::new(
            number("MIN_CHUNK_SIZE", 16384)?,
            number("AVG_CHUNK_SIZE", 65536)?,
            number("MAX_CHUNK_SIZE", 262144)?,
            number("BLAKE3_BATCH", 64)?,
        )
    }
}

fn simd_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        return std::arch::is_x86_feature_detected!("avx2");
    }
    #[cfg(target_arch = "aarch64")]
    {
        return std::arch::is_aarch64_feature_detected!("neon");
    }
    #[allow(unreachable_code)]
    false
}

struct Chunk {
    hash: u64,
    offset: u64,
    data: Vec<u8>,
    digest: blake3::Hash,
}

struct StreamCDC<R> {
    reader: R,
    buffer: Vec<u8>,
    length: usize,
    offset: u64,
    eof: bool,
    config: Config,
    table: gearhash::Table,
    masks: (u64, u64),
}

impl<R: Read> StreamCDC<R> {
    fn new(reader: R, config: Config) -> io::Result<Self> {
        if !simd_available() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "gearhash-simd requires AVX2 on x86_64 or NEON on aarch64",
            ));
        }
        let (table, _) = get_gear_with_seed(0);
        Ok(Self {
            reader,
            buffer: vec![0; config.max],
            length: 0,
            offset: 0,
            eof: false,
            config,
            table: table.as_ref().try_into().expect("256 Gear entries"),
            masks: select_masks(config.avg, Normalization::Level1),
        })
    }

    fn read_chunk(&mut self) -> io::Result<Option<Chunk>> {
        while !self.eof && self.length < self.buffer.len() {
            match self.reader.read(&mut self.buffer[self.length..]) {
                Ok(0) => self.eof = true,
                Ok(n) => self.length += n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        if self.length == 0 {
            return Ok(None);
        }
        let (hash, count, digest) = self.cut();
        let chunk = Chunk {
            hash,
            offset: self.offset,
            data: self.buffer[..count].to_vec(),
            digest,
        };
        self.buffer.copy_within(count..self.length, 0);
        self.length -= count;
        self.offset += count as u64;
        Ok(Some(chunk))
    }

    fn cut(&self) -> (u64, usize, blake3::Hash) {
        let src = &self.buffer[..self.length];
        if src.len() <= self.config.min {
            return (0, src.len(), blake3::hash(src));
        }
        let mut gear = gearhash::Hasher::new(&self.table);
        let mut blake = blake3::Hasher::new();
        let mut fed = 0;
        let mut scan = self.config.min;
        let end = src.len() & !1;
        let center = self.config.avg.min(src.len()) & !1;
        for (limit, mask) in [(center, self.masks.0), (end, self.masks.1)] {
            while scan < limit {
                let stop = scan + self.config.window.min(limit - scan);
                if let Some(n) = gear.next_match(&src[scan..stop], mask) {
                    // gearhash includes the candidate byte; FastCDC cuts before it.
                    let cut = scan + n - 1;
                    let mut hash = gear.get_hash();
                    // FastCDC stores the first candidate of each pair shifted by one.
                    if n % 2 == 1 {
                        hash <<= 1;
                    }
                    blake.update(&src[fed..cut]);
                    return (hash, cut, blake.finalize());
                }
                scan = stop;
                blake.update(&src[fed..scan]);
                fed = scan;
            }
        }
        if src.len() % 2 == 1 {
            gear.update(&src[end..]);
        }
        blake.update(&src[fed..]);
        (gear.get_hash(), src.len(), blake.finalize())
    }
}

pub fn process_file_gearhash_simd<R: Read>(reader: R) -> io::Result<()> {
    let mut stream = StreamCDC::new(reader, Config::from_env()?)?;
    let mut count = 0usize;
    let mut bytes = 0u64;
    while let Some(chunk) = stream.read_chunk()? {
        count += 1;
        bytes += chunk.data.len() as u64;
        black_box((chunk.offset, chunk.data.len(), chunk.digest));
        black_box(chunk.hash);
    }
    black_box((count, bytes));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn check(data: &[u8], config: Config) {
        let expected: Vec<_> =
            v2020::StreamCDC::new(Cursor::new(data), config.min, config.avg, config.max)
                .map(Result::unwrap)
                .collect();
        let fused: Vec<_> = crate::fused_stream_cdc::StreamCDC::new(
            Cursor::new(data),
            config.min,
            config.avg,
            config.max,
        )
        .map(Result::unwrap)
        .collect();
        let mut actual = StreamCDC::new(Cursor::new(data), config).unwrap();
        for (reference, fused) in expected.iter().zip(&fused) {
            let chunk = actual.read_chunk().unwrap().unwrap();
            assert_eq!(
                (chunk.offset, chunk.data.len(), chunk.hash),
                (reference.offset, reference.length, reference.hash)
            );
            assert_eq!(chunk.data, reference.data);
            assert_eq!(chunk.digest, blake3::hash(&reference.data));
            assert_eq!(chunk.digest, fused.blake_hash);
            assert_eq!(chunk.data, fused.data);
        }
        assert_eq!(expected.len(), fused.len());
        assert!(actual.read_chunk().unwrap().is_none());
    }

    #[test]
    fn chunks_and_hashes_match_fastcdc_and_fused() {
        if !simd_available() {
            return;
        }
        let mut state = 12345u64;
        let random: Vec<u8> = (0..300_001)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        for batch in [1, 64, 1023, 1024, 1025, 8192] {
            for (min, avg, max) in [
                (64, 256, 4096),
                (4096, 10240, 65536),
                (16384, 65536, 262144),
            ] {
                let config = Config::new(min, avg, max, batch).unwrap();
                for len in [
                    0,
                    1,
                    63,
                    64,
                    65,
                    1023,
                    1024,
                    1025,
                    4095,
                    4096,
                    4097,
                    random.len(),
                ] {
                    check(&random[..len], config);
                }
                check(&vec![0; 70001], config);
                check(&vec![255; 70001], config);
            }
        }
    }

    #[test]
    fn validates_sizes_and_batch() {
        for sizes in [
            (0, 256, 4096),
            (65, 256, 4096),
            (512, 256, 4096),
            (64, 8192, 4096),
        ] {
            assert!(Config::new(sizes.0, sizes.1, sizes.2, 64).is_err());
        }
        assert!(Config::new(64, 256, 4096, 0).is_err());
        assert!(Config::new(64, 256, 4096, usize::MAX).is_err());
        assert_eq!(Config::new(64, 256, 4096, 64).unwrap().window, 1024);
    }

    #[test]
    fn handles_short_reads_and_propagates_errors() {
        if !simd_available() {
            return;
        }
        struct Short(Cursor<Vec<u8>>);
        impl Read for Short {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = buf.len().min(7);
                self.0.read(&mut buf[..n])
            }
        }
        let config = Config::new(64, 256, 4096, 64).unwrap();
        let mut stream = StreamCDC::new(Short(Cursor::new(vec![0; 10001])), config).unwrap();
        let mut total = 0;
        while let Some(chunk) = stream.read_chunk().unwrap() {
            total += chunk.data.len();
        }
        assert_eq!(total, 10001);
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        assert_eq!(
            StreamCDC::new(Broken, config)
                .unwrap()
                .read_chunk()
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Other
        );
    }
}
