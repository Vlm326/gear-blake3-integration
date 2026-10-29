//! Ablation modes: FastCDC with baseline allocations, or BLAKE3 over saved ranges.
use fastcdc::v2020::{self, StreamCDC};
use std::{
    env,
    hint::black_box,
    io::{self, Cursor, Read},
    ops::Range,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkSizes {
    min: usize,
    avg: usize,
    max: usize,
}

impl ChunkSizes {
    pub fn from_env() -> io::Result<Self> {
        fn read(name: &str, default: usize) -> io::Result<usize> {
            match env::var(name) {
                Ok(value) => value
                    .parse()
                    .map_err(|_| invalid(format!("invalid {name}"))),
                Err(env::VarError::NotPresent) => Ok(default),
                Err(_) => Err(invalid(format!("invalid {name}"))),
            }
        }
        let sizes = Self {
            min: read("MIN_CHUNK_SIZE", 16384)?,
            avg: read("AVG_CHUNK_SIZE", 65536)?,
            max: read("MAX_CHUNK_SIZE", 262144)?,
        };
        sizes.validate()?;
        Ok(sizes)
    }

    fn validate(self) -> io::Result<()> {
        if !(v2020::MINIMUM_MIN..=v2020::MINIMUM_MAX).contains(&self.min)
            || !(v2020::AVERAGE_MIN..=v2020::AVERAGE_MAX).contains(&self.avg)
            || !(v2020::MAXIMUM_MIN..=v2020::MAXIMUM_MAX).contains(&self.max)
            || self.min > self.avg
            || self.avg > self.max
            || !self.min.is_multiple_of(2)
            || !self.avg.is_multiple_of(2)
            || !self.max.is_multiple_of(2)
        {
            return Err(invalid(
                "invalid chunk sizes: use even FastCDC sizes with min <= avg <= max",
            ));
        }
        Ok(())
    }
}

pub fn process_chunking_only<R: Read>(reader: R) -> io::Result<()> {
    // Keep configuration lookup and collect/allocations comparable to baseline.
    let sizes = ChunkSizes::from_env()?;
    let chunks =
        StreamCDC::new(reader, sizes.min, sizes.avg, sizes.max).collect::<Result<Vec<_>, _>>()?;
    black_box(&chunks);
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChunkMap {
    file_size: usize,
    sizes: ChunkSizes,
    ranges: Vec<Range<usize>>,
}

impl ChunkMap {
    pub fn prepare(data: &[u8], sizes: ChunkSizes) -> io::Result<Self> {
        sizes.validate()?;
        let ranges = StreamCDC::new(Cursor::new(data), sizes.min, sizes.avg, sizes.max)
            .map(|chunk| {
                let chunk = chunk?;
                let start = usize::try_from(chunk.offset)
                    .map_err(|_| invalid("chunk offset exceeds address space"))?;
                Ok(start..start + chunk.length)
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self {
            file_size: data.len(),
            sizes,
            ranges,
        })
    }

    pub fn encode(&self) -> String {
        use std::fmt::Write;
        let mut text = format!(
            "# hash_combination chunk-map v1\nfile_size\t{}\nchunk_sizes\t{}\t{}\t{}\n",
            self.file_size, self.sizes.min, self.sizes.avg, self.sizes.max
        );
        for range in &self.ranges {
            writeln!(text, "{}\t{}", range.start, range.len()).unwrap();
        }
        text
    }

    pub fn decode(text: &str, input_size: usize) -> io::Result<Self> {
        fn numbers(line: Option<&str>, prefix: &str, count: usize) -> io::Result<Vec<usize>> {
            let fields = line
                .and_then(|line| line.strip_prefix(prefix))
                .ok_or_else(|| invalid("invalid chunk-map header or row"))?
                .split_whitespace()
                .map(|v| {
                    v.parse::<usize>()
                        .map_err(|_| invalid("invalid chunk-map number"))
                })
                .collect::<io::Result<Vec<_>>>()?;
            if fields.len() != count {
                return Err(invalid("wrong number of chunk-map fields"));
            }
            Ok(fields)
        }
        let mut lines = text.lines();
        if lines.next() != Some("# hash_combination chunk-map v1") {
            return Err(invalid("unsupported chunk-map format"));
        }
        let file_size = numbers(lines.next(), "file_size\t", 1)?[0];
        if file_size != input_size {
            return Err(invalid(
                "chunk-map file size does not match input; regenerate map",
            ));
        }
        let fields = numbers(lines.next(), "chunk_sizes\t", 3)?;
        let sizes = ChunkSizes {
            min: fields[0],
            avg: fields[1],
            max: fields[2],
        };
        sizes.validate()?;
        let mut ranges = Vec::new();
        let mut expected_start = 0;
        for line in lines {
            let fields = numbers(Some(line), "", 2)?;
            let (start, length) = (fields[0], fields[1]);
            let end = start
                .checked_add(length)
                .ok_or_else(|| invalid("chunk range overflow"))?;
            if start != expected_start || length == 0 || end > file_size {
                return Err(invalid(
                    "chunk ranges must be nonempty, contiguous and within input",
                ));
            }
            ranges.push(start..end);
            expected_start = end;
        }
        if expected_start != file_size {
            return Err(invalid("chunk map does not cover the complete input"));
        }
        Ok(Self {
            file_size,
            sizes,
            ranges,
        })
    }

    /// Call only with the input validated when loading the map. No scanning or copying.
    pub fn process_blake3_only(&self, data: &[u8]) {
        for range in &self.ranges {
            let digest = blake3::hash(black_box(&data[range.clone()]));
            black_box((range.start, range.len(), digest));
        }
        black_box((self.ranges.len(), self.file_size));
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: ChunkSizes = ChunkSizes {
        min: 64,
        avg: 256,
        max: 1024,
    };

    #[test]
    fn saved_ranges_preserve_reference_chunks_and_digests() {
        for data in [
            Vec::new(),
            vec![0; 63],
            vec![0; 64],
            vec![0; 65],
            vec![0; 2049],
            (0usize..20_000)
                .map(|i| (i.wrapping_mul(37) ^ (i >> 3)) as u8)
                .collect(),
        ] {
            let prepared = ChunkMap::prepare(&data, SIZES).unwrap();
            let loaded = ChunkMap::decode(&prepared.encode(), data.len()).unwrap();
            assert_eq!(prepared, loaded);
            let reference = StreamCDC::new(Cursor::new(&data), SIZES.min, SIZES.avg, SIZES.max)
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(loaded.ranges.len(), reference.len());
            for (range, chunk) in loaded.ranges.iter().zip(reference) {
                assert_eq!(range.start as u64, chunk.offset);
                assert_eq!(range.len(), chunk.length);
                assert_eq!(&data[range.clone()], chunk.data);
                assert_eq!(
                    blake3::hash(&data[range.clone()]),
                    blake3::hash(&chunk.data)
                );
            }
            loaded.process_blake3_only(&data);
        }
    }

    #[test]
    fn rejects_malformed_and_incomplete_maps() {
        let header = "# hash_combination chunk-map v1\nfile_size\t10\nchunk_sizes\t64\t256\t1024\n";
        for rows in [
            "",
            "0 0\n",
            "1 9\n",
            "0 11\n",
            "0 4\n",
            "0 5\n4 6\n",
            "0 4\n5 5\n",
            "x 10\n",
            "0 10 extra\n",
            "0 10\n10 1\n",
            "0 18446744073709551615\n",
        ] {
            assert!(
                ChunkMap::decode(&format!("{header}{rows}"), 10).is_err(),
                "{rows:?}"
            );
        }
        assert!(ChunkMap::decode("garbage", 10).is_err());
        assert!(ChunkMap::decode(&format!("{header}0 10\n"), 11).is_err());
        assert!(
            ChunkMap::decode(&format!("{header}0 10\n").replace("64\t256", "65\t256"), 10).is_err()
        );
    }

    #[test]
    fn read_failures_are_propagated() {
        struct BrokenReader;
        impl Read for BrokenReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        assert!(process_chunking_only(BrokenReader).is_err());
    }
}
