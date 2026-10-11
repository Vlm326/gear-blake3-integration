use std::{collections::HashMap, error::Error, fmt, io, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Baseline,
    Fused,
}

#[derive(Debug, PartialEq)]
pub struct ResultOfOneRun {
    pub mode: Mode,
    pub batch: usize,
    pub chunks_settings: (usize, usize, usize),
    pub cycles: u64,
    pub instructions: u64,
    pub cache_references: u64,
    pub cache_misses: u64,
    pub elapsed_seconds: f64,
    pub elapsed_spread_seconds: f64,
    pub elapsed_spread_percent: f64,
    pub counter_spread_percent: HashMap<String, f64>,
}

fn invalid_data(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

fn parse_number<T>(value: &str, field: &str) -> Result<T, Box<dyn Error>>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    value
        .trim()
        .parse()
        .map_err(|error| invalid_data(format!("invalid {field}: {error}")))
}

fn clean_line(line: &str) -> &str {
    let line = line.trim();
    let line = line.strip_prefix("/*").unwrap_or(line);
    let line = line.strip_suffix("*/").unwrap_or(line);
    line.trim()
}

fn parse_header(header: &str) -> Result<(Mode, usize, (usize, usize, usize)), Box<dyn Error>> {
    let mut mode = None;
    let mut batch = None;
    let mut chunks = None;

    for part in header.split(';').map(str::trim) {
        if let Some(value) = part.strip_prefix("Mode:") {
            mode = Some(match value.trim() {
                "baseline" => Mode::Baseline,
                "fused" => Mode::Fused,
                other => return Err(invalid_data(format!("unknown mode {other:?}"))),
            });
        } else if let Some(value) = part.strip_prefix("batch=") {
            let value = value
                .trim()
                .strip_suffix("bytes")
                .unwrap_or(value.trim())
                .trim();
            batch = Some(parse_number(value, "batch")?);
        } else if let Some(value) = part.strip_prefix("chunks min/avg/max=") {
            let value = value
                .trim()
                .strip_suffix("bytes")
                .unwrap_or(value.trim())
                .trim();
            let values: Vec<_> = value.split('/').map(str::trim).collect();
            if values.len() != 3 {
                return Err(invalid_data(format!(
                    "expected three chunk sizes, got {value:?}"
                )));
            }
            chunks = Some((
                parse_number(values[0], "minimum chunk size")?,
                parse_number(values[1], "average chunk size")?,
                parse_number(values[2], "maximum chunk size")?,
            ));
        }
    }

    Ok((
        mode.ok_or_else(|| invalid_data("missing mode in header"))?,
        batch.ok_or_else(|| invalid_data("missing batch in header"))?,
        chunks.ok_or_else(|| invalid_data("missing chunk sizes in header"))?,
    ))
}

fn parse_counter_spread(value: &str) -> Result<HashMap<String, f64>, Box<dyn Error>> {
    let value = value.trim();
    let value = value
        .strip_prefix('{')
        .and_then(|value| value.strip_suffix('}'))
        .ok_or_else(|| invalid_data("counter_spread_percent must be enclosed in braces"))?;

    let mut counters = HashMap::new();
    for pair in value
        .split(',')
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
    {
        let (key, value) = pair
            .split_once(':')
            .ok_or_else(|| invalid_data(format!("invalid counter spread entry {pair:?}")))?;
        let key = key.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() {
            return Err(invalid_data("counter spread key cannot be empty"));
        }
        counters.insert(
            key.to_owned(),
            parse_number(value, "counter spread percentage")?,
        );
    }

    Ok(counters)
}

/// Parses one result block, optionally wrapped in a `/* ... */` comment.
pub fn parse_run_result(input: &str) -> Result<ResultOfOneRun, Box<dyn Error>> {
    let mut lines = input
        .lines()
        .map(clean_line)
        .filter(|line| !line.is_empty());
    let header = lines.next().ok_or_else(|| invalid_data("input is empty"))?;
    let (mode, batch, chunks_settings) = parse_header(header)?;

    let mut cycles = None;
    let mut instructions = None;
    let mut cache_references = None;
    let mut cache_misses = None;
    let mut elapsed_seconds = None;
    let mut elapsed_spread_seconds = None;
    let mut elapsed_spread_percent = None;
    let mut counter_spread_percent = None;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(invalid_data(format!("invalid result line {line:?}")));
        };
        let value = value.trim();

        match name.trim() {
            "cycles" => cycles = Some(parse_number(value, "cycles")?),
            "instructions" => instructions = Some(parse_number(value, "instructions")?),
            "cache_references" => cache_references = Some(parse_number(value, "cache_references")?),
            "cache_misses" => cache_misses = Some(parse_number(value, "cache_misses")?),
            "elapsed_seconds" => elapsed_seconds = Some(parse_number(value, "elapsed_seconds")?),
            "elapsed_spread_seconds" => {
                elapsed_spread_seconds = Some(parse_number(value, "elapsed_spread_seconds")?)
            }
            "elapsed_spread_percent" => {
                elapsed_spread_percent = Some(parse_number(value, "elapsed_spread_percent")?)
            }
            "counter_spread_percent" => counter_spread_percent = Some(parse_counter_spread(value)?),
            _ => {} // Ignore extra metrics so the format can be extended.
        }
    }

    Ok(ResultOfOneRun {
        mode,
        batch,
        chunks_settings,
        cycles: cycles.ok_or_else(|| invalid_data("missing cycles"))?,
        instructions: instructions.ok_or_else(|| invalid_data("missing instructions"))?,
        cache_references: cache_references
            .ok_or_else(|| invalid_data("missing cache_references"))?,
        cache_misses: cache_misses.ok_or_else(|| invalid_data("missing cache_misses"))?,
        elapsed_seconds: elapsed_seconds.ok_or_else(|| invalid_data("missing elapsed_seconds"))?,
        elapsed_spread_seconds: elapsed_spread_seconds
            .ok_or_else(|| invalid_data("missing elapsed_spread_seconds"))?,
        elapsed_spread_percent: elapsed_spread_percent
            .ok_or_else(|| invalid_data("missing elapsed_spread_percent"))?,
        counter_spread_percent: counter_spread_percent
            .ok_or_else(|| invalid_data("missing counter_spread_percent"))?,
    })
}

pub fn parse_run_results(input: &str) -> Result<Vec<ResultOfOneRun>, Box<dyn Error>> {
    let mut results = Vec::new();
    let mut current_block = String::new();

    for line in input.lines() {
        let line = clean_line(line);
        if line.is_empty() {
            if !current_block.is_empty() {
                results.push(parse_run_result(&current_block)?);
                current_block.clear();
            }
        } else {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    if !current_block.is_empty() {
        results.push(parse_run_result(&current_block)?);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::{Mode, parse_run_result};

    const SAMPLE: &str = "/*Mode: fused; batch=8192 bytes; chunks min/avg/max=16384/65536/262144 bytes\n\
        cycles: 2394591837\n\
        instructions: 8233380777\n\
        cache_references: 278320333\n\
        cache_misses: 13720847\n\
        elapsed_seconds: 1.27122\n\
        elapsed_spread_seconds: 0.00273\n\
        elapsed_spread_percent: 0.21\n\
        counter_spread_percent: {'cycles': 0.2, 'instructions': 0.0, 'cache-references': 0.78, 'cache-misses': 2.74}\n\
        */";

    #[test]
    fn parses_commented_sample() {
        let result = parse_run_result(SAMPLE).unwrap();

        assert_eq!(result.mode, Mode::Fused);
        assert_eq!(result.batch, 8192);
        assert_eq!(result.chunks_settings, (16384, 65536, 262144));
        assert_eq!(result.cycles, 2_394_591_837);
        assert_eq!(result.elapsed_seconds, 1.27122);
        assert_eq!(result.counter_spread_percent["cache-references"], 0.78);
        assert_eq!(result.counter_spread_percent["cache-misses"], 2.74);
    }

    #[test]
    fn rejects_missing_metrics() {
        let error = parse_run_result("Mode: baseline; batch=1 bytes; chunks min/avg/max=1/2/3")
            .unwrap_err();
        assert!(error.to_string().contains("missing cycles"));
    }
}
