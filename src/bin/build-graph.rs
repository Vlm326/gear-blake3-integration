#[path = "../analysys/mod.rs"]
mod analysys;

use std::{env, error::Error, fs, io};

fn main() -> Result<(), Box<dyn Error>> {
    if env::args_os().len() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run --release --features reports --bin build-graph",
        )
        .into());
    }
    let graph_data_path = env::current_dir()?.join("artifacts/benchmarks/chunk_sweep");
    let fused_data = fs::read_to_string(graph_data_path.join("results.txt"))?;
    let baseline_data = fs::read_to_string(graph_data_path.join("results_baseline.txt"))?;
    let fused_results = analysys::parser::parse_run_results(&fused_data)?;
    let baseline_results = analysys::parser::parse_run_results(&baseline_data)?;
    analysys::graph_builder::build_graph(fused_results, baseline_results)
}
