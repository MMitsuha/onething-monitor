use super::history::DeviceHistory;
use chrono::{DateTime, Local};
use plotters::prelude::*;
use std::io::Cursor;
use std::sync::Once;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

static FONT_INIT: Once = Once::new();

/// Load a system TTF font and register it with plotters' ab_glyph backend.
fn ensure_fonts() {
    FONT_INIT.call_once(|| {
        let font_paths = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ];

        for path in &font_paths {
            if let Ok(data) = std::fs::read(path) {
                let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
                let _ = plotters::style::register_font(
                    "sans-serif",
                    plotters::style::FontStyle::Normal,
                    leaked,
                );
                let _ = plotters::style::register_font(
                    "sans-serif",
                    plotters::style::FontStyle::Bold,
                    leaked,
                );
                return;
            }
        }
    });
}

/// Fixed color palette for lines (up to 16 lines).
const LINE_COLORS: &[RGBColor] = &[
    RGBColor(31, 119, 180),  // blue
    RGBColor(255, 127, 14),  // orange
    RGBColor(44, 160, 44),   // green
    RGBColor(214, 39, 40),   // red
    RGBColor(148, 103, 189), // purple
    RGBColor(140, 86, 75),   // brown
    RGBColor(227, 119, 194), // pink
    RGBColor(127, 127, 127), // gray
    RGBColor(188, 189, 34),  // olive
    RGBColor(23, 190, 207),  // cyan
    RGBColor(65, 68, 81),    // dark gray
    RGBColor(255, 187, 120), // light orange
    RGBColor(152, 223, 138), // light green
    RGBColor(255, 152, 150), // light red
    RGBColor(197, 176, 213), // light purple
    RGBColor(196, 156, 148), // light brown
];

fn color_for_line(idx: usize) -> RGBColor {
    LINE_COLORS[idx % LINE_COLORS.len()]
}

/// Render a chart PNG for a device. Returns PNG bytes.
/// Top panel: upload/download speed (MB/s) per line.
/// Bottom panel: packet loss (%) per line.
pub fn render_device_chart(
    device_sn: &str,
    history: &DeviceHistory,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    ensure_fonts();
    let mut buf = vec![0u8; (WIDTH * HEIGHT * 3) as usize];

    {
        let root = BitMapBackend::with_buffer(&mut buf, (WIDTH, HEIGHT)).into_drawing_area();
        root.fill(&WHITE)?;

        // Title
        let title = format!("{} ({})", history.remark, device_sn);
        root.titled(&title, ("sans-serif", 18).into_font())?;

        // Split into two vertical panels
        let panels = root.split_evenly((2, 1));

        // Collect sorted line keys for consistent ordering
        let mut line_keys: Vec<&String> = history.lines.keys().collect();
        line_keys.sort();

        // Find time range across all lines
        let (time_min, time_max) = find_time_range(history);
        let time_min = time_min.unwrap_or_else(Local::now);
        let time_max = time_max.unwrap_or_else(Local::now);

        // Ensure non-zero time range (at least 1 minute)
        let time_max = if time_max <= time_min {
            time_min + chrono::Duration::minutes(1)
        } else {
            time_max
        };

        // ── Top panel: speed ──
        render_speed_panel(&panels[0], &line_keys, history, time_min, time_max)?;

        // ── Bottom panel: packet loss ──
        render_loss_panel(&panels[1], &line_keys, history, time_min, time_max)?;

        root.present()?;
    }

    // Encode raw RGB buffer to PNG using the image crate
    let img = image::RgbImage::from_raw(WIDTH, HEIGHT, buf)
        .ok_or("Failed to create image from raw buffer")?;
    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)?;

    Ok(png_bytes)
}

fn find_time_range(history: &DeviceHistory) -> (Option<DateTime<Local>>, Option<DateTime<Local>>) {
    let mut min: Option<DateTime<Local>> = None;
    let mut max: Option<DateTime<Local>> = None;
    for buf in history.lines.values() {
        for s in buf {
            let t = s.timestamp;
            min = Some(min.map_or(t, |m: DateTime<Local>| m.min(t)));
            max = Some(max.map_or(t, |m: DateTime<Local>| m.max(t)));
        }
    }
    (min, max)
}

fn render_speed_panel(
    area: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    _line_keys: &[&String],
    history: &DeviceHistory,
    time_min: DateTime<Local>,
    time_max: DateTime<Local>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Aggregate speed across all lines by timestamp.
    // All lines share the same timestamps per collection cycle.
    let mut totals: std::collections::BTreeMap<DateTime<Local>, (f64, f64)> =
        std::collections::BTreeMap::new();

    for buf in history.lines.values() {
        for s in buf {
            let entry = totals.entry(s.timestamp).or_insert((0.0, 0.0));
            entry.0 += s.upspeed_bytes.unwrap_or(0) as f64 / 1_000_000.0;
            entry.1 += s.downspeed_bytes.unwrap_or(0) as f64 / 1_000_000.0;
        }
    }

    let up_data: Vec<_> = totals.iter().map(|(t, (up, _))| (*t, *up)).collect();
    let down_data: Vec<_> = totals.iter().map(|(t, (_, down))| (*t, *down)).collect();

    let max_speed = totals
        .values()
        .fold(1.0f64, |m, (up, down)| m.max(*up).max(*down))
        * 1.1;

    let mut chart = ChartBuilder::on(area)
        .caption("Speed (MB/s)", ("sans-serif", 14))
        .margin(5)
        .x_label_area_size(30)
        .y_label_area_size(50)
        .build_cartesian_2d(time_min..time_max, 0f64..max_speed)?;

    chart
        .configure_mesh()
        .x_label_formatter(&|t| t.format("%H:%M").to_string())
        .y_label_formatter(&|v| format!("{:.1}", v))
        .draw()?;

    let up_color = RGBColor(44, 160, 44); // green
    let down_color = RGBColor(31, 119, 180); // blue

    if !up_data.is_empty() {
        chart
            .draw_series(LineSeries::new(up_data, up_color.stroke_width(2)))?
            .label("Upload")
            .legend(move |(x, y)| {
                PathElement::new(vec![(x, y), (x + 15, y)], up_color.stroke_width(2))
            });
    }

    if !down_data.is_empty() {
        chart
            .draw_series(LineSeries::new(down_data, down_color.stroke_width(2)))?
            .label("Download")
            .legend(move |(x, y)| {
                PathElement::new(vec![(x, y), (x + 15, y)], down_color.stroke_width(2))
            });
    }

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK.mix(0.3))
        .label_font(("sans-serif", 10))
        .draw()?;

    Ok(())
}

fn render_loss_panel(
    area: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    line_keys: &[&String],
    history: &DeviceHistory,
    time_min: DateTime<Local>,
    time_max: DateTime<Local>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Find max loss for Y range
    let mut max_loss: f64 = 5.0; // at least 5%
    for key in line_keys {
        if let Some(buf) = history.lines.get(*key) {
            for s in buf {
                if let Some(l) = s.lost {
                    max_loss = max_loss.max(l);
                }
            }
        }
    }
    max_loss = (max_loss * 1.1).min(100.0);

    let mut chart = ChartBuilder::on(area)
        .caption("Packet Loss (%)", ("sans-serif", 14))
        .margin(5)
        .x_label_area_size(30)
        .y_label_area_size(50)
        .build_cartesian_2d(time_min..time_max, 0f64..max_loss)?;

    chart
        .configure_mesh()
        .x_label_formatter(&|t| t.format("%H:%M").to_string())
        .y_label_formatter(&|v| format!("{:.1}", v))
        .draw()?;

    for (idx, key) in line_keys.iter().enumerate() {
        let color = color_for_line(idx);
        if let Some(buf) = history.lines.get(*key) {
            let loss_data: Vec<_> = buf
                .iter()
                .filter_map(|s| s.lost.map(|v| (s.timestamp, v)))
                .collect();
            if !loss_data.is_empty() {
                chart
                    .draw_series(LineSeries::new(loss_data, color.stroke_width(1)))?
                    .label(key.to_string())
                    .legend(move |(x, y)| {
                        PathElement::new(vec![(x, y), (x + 15, y)], color.stroke_width(2))
                    });
            }
        }
    }

    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperRight)
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK.mix(0.3))
        .label_font(("sans-serif", 10))
        .draw()?;

    Ok(())
}
