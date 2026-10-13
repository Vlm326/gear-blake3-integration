use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs, io,
};

use plotters::{
    coord::Shift,
    prelude::*,
    style::text_anchor::{HPos, Pos, VPos},
};

use crate::analysys::parser::{Mode, ResultOfOneRun};

const OUTPUT_FILE: &str = "artifacts/charts/speedup.svg";
const WIDTH: u32 = 1920;
const LEGEND_WIDTH: u32 = 260;

type ChunkSettings = (usize, usize, usize);
type ConfigKey = (usize, ChunkSettings);

pub fn build_graph(
    fused_data: Vec<ResultOfOneRun>,
    baseline_data: Vec<ResultOfOneRun>,
) -> Result<(), Box<dyn Error>> {
    let speedups = collect_speedups(fused_data, baseline_data)?;
    if speedups.is_empty() {
        return Err(invalid_data("no benchmark results to plot"));
    }

    let batches = speedups
        .keys()
        .map(|(batch, _)| *batch)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let chunk_settings = speedups
        .keys()
        .map(|(_, settings)| *settings)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let max_cells = batches.len().max(chunk_settings.len());
    let height = 220u32.saturating_add((max_cells as u32).saturating_mul(48));
    fs::create_dir_all("artifacts/charts")?;
    let root = SVGBackend::new(OUTPUT_FILE, (WIDTH, height)).into_drawing_area();
    root.fill(&WHITE)?;
    let (chart_area, legend_area) = root.split_horizontally(WIDTH - LEGEND_WIDTH);

    let batch_count = i32::try_from(batches.len())?;
    let settings_count = i32::try_from(chunk_settings.len())?;
    let mut chart = ChartBuilder::on(&chart_area)
        .caption("Ускорение Fused относительно Baseline", ("sans-serif", 32))
        .margin(20)
        .x_label_area_size(72)
        .y_label_area_size(245)
        .build_cartesian_2d(0..batch_count * 2, 0..settings_count * 2)?;

    let batches_for_labels = batches.clone();
    let settings_for_labels = chunk_settings.clone();
    chart
        .configure_mesh()
        .disable_mesh()
        .x_desc("BLAKE3_BATCH (KiB)")
        .y_desc("FastCDC min / avg / max (KiB)")
        .axis_desc_style(("sans-serif", 19))
        .label_style(("sans-serif", 15))
        .x_labels(batches.len() * 2 + 1)
        .y_labels(chunk_settings.len() * 2 + 1)
        .x_label_formatter(&move |position| {
            if *position >= 0 && position % 2 == 1 {
                batches_for_labels
                    .get(((position - 1) / 2) as usize)
                    .map(|batch| format_size_kib(*batch))
                    .unwrap_or_default()
            } else {
                String::new()
            }
        })
        .y_label_formatter(&move |position| {
            if *position >= 0 && position % 2 == 1 {
                settings_for_labels
                    .get(((position - 1) / 2) as usize)
                    .map(|settings| {
                        format!(
                            "{} / {} / {}",
                            format_size_kib(settings.0),
                            format_size_kib(settings.1),
                            format_size_kib(settings.2)
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            }
        })
        .draw()?;

    let min_speedup = speedups.values().copied().fold(f64::INFINITY, f64::min);
    let max_speedup = speedups.values().copied().fold(f64::NEG_INFINITY, f64::max);
    let cell_font_size = if chunk_settings.len() > 24 || batches.len() > 24 {
        13
    } else {
        18
    };

    for (row, settings) in chunk_settings.iter().enumerate() {
        for (column, batch) in batches.iter().enumerate() {
            let x0 = column as i32 * 2;
            let y0 = row as i32 * 2;
            let cell = [(x0, y0), (x0 + 2, y0 + 2)];

            if let Some(speedup) = speedups.get(&(*batch, *settings)) {
                let color = speedup_color(*speedup, min_speedup, max_speedup);
                chart.draw_series(std::iter::once(Rectangle::new(
                    cell,
                    color.filled().stroke_width(2),
                )))?;
                chart.draw_series(std::iter::once(Text::new(
                    format!("{speedup:.2}×"),
                    (x0 + 1, y0 + 1),
                    ("sans-serif", cell_font_size)
                        .into_font()
                        .color(&BLACK)
                        .pos(Pos::new(HPos::Center, VPos::Center)),
                )))?;
            } else {
                chart.draw_series(std::iter::once(Rectangle::new(
                    cell,
                    RGBColor(235, 235, 235).filled().stroke_width(2),
                )))?;
                chart.draw_series(std::iter::once(Text::new(
                    "—",
                    (x0 + 1, y0 + 1),
                    ("sans-serif", cell_font_size)
                        .into_font()
                        .color(&BLACK)
                        .pos(Pos::new(HPos::Center, VPos::Center)),
                )))?;
            }
        }
    }

    draw_legend(&legend_area, min_speedup, max_speedup)?;
    drop(chart);
    root.present()?;
    Ok(())
}

fn collect_speedups(
    fused_data: Vec<ResultOfOneRun>,
    baseline_data: Vec<ResultOfOneRun>,
) -> Result<BTreeMap<ConfigKey, f64>, Box<dyn Error>> {
    let mut samples: BTreeMap<ConfigKey, (Vec<f64>, Vec<f64>)> = BTreeMap::new();

    for result in fused_data.into_iter().chain(baseline_data) {
        if !result.elapsed_seconds.is_finite() || result.elapsed_seconds <= 0.0 {
            return Err(invalid_data(format!(
                "elapsed_seconds must be finite and positive for batch={} and chunks={:?}",
                result.batch, result.chunks_settings
            )));
        }

        let entry = samples
            .entry((result.batch, result.chunks_settings))
            .or_default();
        match result.mode {
            Mode::Fused => entry.0.push(result.elapsed_seconds),
            Mode::Baseline => entry.1.push(result.elapsed_seconds),
        }
    }

    samples
        .into_iter()
        .map(|(key, (fused_times, baseline_times))| {
            if fused_times.is_empty() || baseline_times.is_empty() {
                return Err(invalid_data(format!(
                    "missing fused or baseline measurements for batch={} and chunks={:?}",
                    key.0, key.1
                )));
            }

            let fused_median = median(fused_times);
            let baseline_median = median(baseline_times);
            Ok((key, baseline_median / fused_median))
        })
        .collect()
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        values[middle - 1] / 2.0 + values[middle] / 2.0
    } else {
        values[middle]
    }
}

fn format_size_kib(bytes: usize) -> String {
    if bytes.is_multiple_of(1024) {
        format!("{}", bytes / 1024)
    } else {
        format!("{:.1}", bytes as f64 / 1024.0)
    }
}

fn speedup_color(value: f64, min: f64, max: f64) -> RGBColor {
    let slower = RGBColor(215, 48, 39);
    let equal = RGBColor(255, 255, 191);
    let faster = RGBColor(26, 152, 80);

    if value < 1.0 {
        let fraction = if min < 1.0 {
            ((value - min) / (1.0 - min)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        mix(slower, equal, fraction)
    } else {
        let fraction = if max > 1.0 {
            ((value - 1.0) / (max - 1.0)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        mix(equal, faster, fraction)
    }
}

fn mix(start: RGBColor, end: RGBColor, fraction: f64) -> RGBColor {
    let blend =
        |left: u8, right: u8| (left as f64 + (right as f64 - left as f64) * fraction).round() as u8;
    RGBColor(
        blend(start.0, end.0),
        blend(start.1, end.1),
        blend(start.2, end.2),
    )
}

fn draw_legend(
    area: &DrawingArea<SVGBackend<'_>, Shift>,
    min_speedup: f64,
    max_speedup: f64,
) -> Result<(), Box<dyn Error>> {
    let (_, height) = area.dim_in_pixel();
    let top = 150i32;
    let bottom = height as i32 - 150;
    let bar_left = 28i32;
    let bar_right = 78i32;
    let bar_height = (bottom - top).max(1);

    area.draw(&Text::new(
        "Ускорение",
        (bar_left, 90),
        ("sans-serif", 22).into_font(),
    ))?;

    for step in 0..bar_height {
        let fraction = step as f64 / bar_height as f64;
        let value = max_speedup - fraction * (max_speedup - min_speedup);
        let y = top + step;
        area.draw(&Rectangle::new(
            [(bar_left, y), (bar_right, y + 1)],
            speedup_color(value, min_speedup, max_speedup).filled(),
        ))?;
    }

    area.draw(&Text::new(
        format!("{max_speedup:.2}×"),
        (bar_right + 12, top + 10),
        ("sans-serif", 17).into_font(),
    ))?;
    area.draw(&Text::new(
        format!("{min_speedup:.2}×"),
        (bar_right + 12, bottom - 10),
        ("sans-serif", 17).into_font(),
    ))?;

    if min_speedup <= 1.0 && max_speedup >= 1.0 && max_speedup > min_speedup {
        let fraction = (max_speedup - 1.0) / (max_speedup - min_speedup);
        let y = top + (fraction * bar_height as f64).round() as i32;
        area.draw(&PathElement::new(
            vec![(bar_left - 5, y), (bar_right + 5, y)],
            BLACK.stroke_width(2),
        ))?;
        area.draw(&Text::new(
            "1.00×",
            (bar_right + 12, y),
            ("sans-serif", 17).into_font(),
        ))?;
    }

    area.draw(&Text::new(
        "1× = одинаковое время",
        (bar_left, bottom + 45),
        ("sans-serif", 15).into_font(),
    ))?;
    Ok(())
}

fn invalid_data(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}
