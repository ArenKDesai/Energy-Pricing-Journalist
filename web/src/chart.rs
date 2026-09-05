//! Canvas2D line chart for a single HUB, with axes, high/low markers, the
//! latest value, and an optional dashed forecast overlay.
//!
//! Two price series are drawn rather than one. The settled hourly archive
//! (from `db/`) is drawn muted and thin; the live 5-minute feed is drawn full
//! weight on top. They are separate paths, so the settlement gap between the
//! newest settled hour and the first live sample shows as a break rather than
//! an invented straight line across it.

use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::model::Forecast;
use crate::types::{now_secs, Point, HISTORY_INTERVAL_SECONDS, INTERVAL_SECONDS};

const BG: &str = "#0d1117";
const GRID: &str = "#21262d";
const AXIS_TEXT: &str = "#8b949e";
const FORECAST: &str = "#ffaa00";
const DA_LINE: &str = "#6e86c9";
/// Neutral stand-in for the real-time gradient in the legend swatch.
const LEGEND_RT: &str = "#c9d1d9";

/// Opacity of the settled archive line, so live data reads as the foreground.
const ARCHIVE_ALPHA: f64 = 0.5;

struct Margin {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

const MARGIN: Margin = Margin {
    left: 64.0,
    right: 18.0,
    top: 18.0,
    bottom: 40.0,
};

/// Everything the chart draws for the selected hub.
pub struct ChartData<'a> {
    /// Settled hourly RT prices from the archive.
    pub rt_archive: &'a [Point],
    /// Live 5-minute RT prices accumulated this session (and from IndexedDB).
    pub rt_live: &'a [Point],
    /// Settled hourly DA prices, drawn only when the overlay is enabled.
    pub da_archive: Option<&'a [Point]>,
    /// Seconds of history to show.
    pub range_secs: i64,
}

/// Mint to amber to red for positive, mint to cyan to purple for negative,
/// where `t` is the value normalized into [-1, 1]. Mirrors the original
/// WebGPU shader.
fn price_color(t: f64) -> String {
    let a = t.abs().clamp(0.0, 1.0);
    let mix = |x: f64, y: f64, s: f64| x + (y - x) * s;
    let (r, g, b) = if t >= 0.0 {
        if a < 0.5 {
            let s = a * 2.0;
            (mix(0.667, 1.0, s), mix(0.941, 0.667, s), mix(0.757, 0.0, s))
        } else {
            let s = (a - 0.5) * 2.0;
            (1.0, mix(0.667, 0.1, s), mix(0.0, 0.1, s))
        }
    } else if a < 0.5 {
        let s = a * 2.0;
        (mix(0.667, 0.0, s), mix(0.941, 0.867, s), mix(0.757, 1.0, s))
    } else {
        let s = (a - 0.5) * 2.0;
        (mix(0.0, 0.533, s), mix(0.867, 0.0, s), 1.0)
    };
    format!(
        "rgb({},{},{})",
        (r * 255.0) as u8,
        (g * 255.0) as u8,
        (b * 255.0) as u8
    )
}

/// Format an epoch-seconds instant as a local-time axis label.
fn time_label(ts: i64, with_date: bool) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((ts * 1000) as f64));
    let hh = d.get_hours();
    let mm = d.get_minutes();
    if with_date {
        format!("{}/{} {:02}:{:02}", d.get_month() + 1, d.get_date(), hh, mm)
    } else {
        format!("{:02}:{:02}", hh, mm)
    }
}

/// Points falling inside the visible time window.
fn visible(pts: &[Point], t_min: i64, t_max: i64) -> Vec<Point> {
    pts.iter()
        .filter(|(t, _)| *t >= t_min && *t <= t_max)
        .copied()
        .collect()
}

/// One legend row: a short line sample plus its label.
struct LegendEntry<'a> {
    label: &'a str,
    color: &'a str,
    width: f64,
    alpha: f64,
    dashed: bool,
}

/// Draw the legend in the top-right of the plot area.
///
/// Real-time is stroked with a price-dependent gradient rather than one colour,
/// so its swatch is drawn neutral and the entry is distinguished by weight —
/// which is exactly how the two real-time series read on the chart itself.
fn draw_legend(ctx: &CanvasRenderingContext2d, entries: &[LegendEntry], px1: f64, py0: f64) {
    if entries.is_empty() {
        return;
    }
    ctx.set_font("11px Arial");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");

    let sample = 18.0;
    let gap = 6.0;
    let row = 15.0;
    let widest = entries
        .iter()
        .map(|e| ctx.measure_text(e.label).map(|m| m.width()).unwrap_or(70.0))
        .fold(0.0f64, f64::max);
    let box_w = sample + gap + widest + 16.0;
    let box_h = entries.len() as f64 * row + 10.0;
    let x = px1 - box_w - 6.0;
    let y = py0 + 6.0;

    ctx.set_fill_style_str("rgba(13,17,23,0.72)");
    ctx.fill_rect(x, y, box_w, box_h);
    ctx.set_stroke_style_str(GRID);
    ctx.set_line_width(1.0);
    ctx.stroke_rect(x, y, box_w, box_h);

    for (i, e) in entries.iter().enumerate() {
        let cy = y + 5.0 + row * i as f64 + row / 2.0;
        if e.dashed {
            let dash = js_sys::Array::of2(
                &wasm_bindgen::JsValue::from_f64(3.0),
                &wasm_bindgen::JsValue::from_f64(3.0),
            );
            let _ = ctx.set_line_dash(&dash);
        }
        ctx.set_global_alpha(e.alpha);
        ctx.set_stroke_style_str(e.color);
        ctx.set_line_width(e.width);
        ctx.begin_path();
        ctx.move_to(x + 8.0, cy);
        ctx.line_to(x + 8.0 + sample, cy);
        ctx.stroke();
        ctx.set_global_alpha(1.0);
        let _ = ctx.set_line_dash(&js_sys::Array::new());

        ctx.set_fill_style_str(AXIS_TEXT);
        let _ = ctx.fill_text(e.label, x + 8.0 + sample + gap, cy);
    }
}

/// Render the chart.
pub fn render(canvas: &HtmlCanvasElement, data: &ChartData, forecast: &Forecast) {
    let ctx = match canvas.get_context("2d") {
        Ok(Some(c)) => c.unchecked_into::<CanvasRenderingContext2d>(),
        _ => return,
    };

    let dpr = web_sys::window()
        .and_then(|w| w.device_pixel_ratio().into())
        .unwrap_or(1.0)
        .max(1.0);

    let css_w = canvas.client_width() as f64;
    let css_h = canvas.client_height() as f64;
    if css_w <= 0.0 || css_h <= 0.0 {
        return;
    }
    canvas.set_width((css_w * dpr) as u32);
    canvas.set_height((css_h * dpr) as u32);
    let _ = ctx.scale(dpr, dpr);

    // Background.
    ctx.set_fill_style_str(BG);
    ctx.fill_rect(0.0, 0.0, css_w, css_h);

    // Plot area.
    let px0 = MARGIN.left;
    let px1 = css_w - MARGIN.right;
    let py0 = MARGIN.top;
    let py1 = css_h - MARGIN.bottom;
    if px1 <= px0 || py1 <= py0 {
        return;
    }

    // X domain: the selected range up to now, extended to cover the forecast,
    // and never narrower than an hour so a sparse chart still reads sensibly.
    let now = now_secs();
    let t_max = now.max(forecast.mean.last().map(|p| p.0).unwrap_or(now));
    let earliest = data
        .rt_archive
        .first()
        .map(|p| p.0)
        .into_iter()
        .chain(data.rt_live.first().map(|p| p.0))
        .min();
    let mut t_min = now - data.range_secs;
    if let Some(e) = earliest {
        t_min = t_min.max(e);
    }
    if t_max - t_min < 3600 {
        t_min = t_max - 3600;
    }

    let arch = visible(data.rt_archive, t_min, t_max);
    let live = visible(data.rt_live, t_min, t_max);
    let da: Vec<Point> = data
        .da_archive
        .map(|d| visible(d, t_min, t_max))
        .unwrap_or_default();

    if arch.is_empty() && live.is_empty() {
        return;
    }

    // Y domain over everything visible, padded 8%.
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (_, v) in arch
        .iter()
        .chain(live.iter())
        .chain(da.iter())
        .chain(forecast.lower.iter())
        .chain(forecast.upper.iter())
    {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    if !lo.is_finite() || !hi.is_finite() {
        return;
    }
    if (hi - lo).abs() < 1.0 {
        lo -= 1.0;
        hi += 1.0;
    }
    let pad = (hi - lo) * 0.08;
    lo -= pad;
    hi += pad;

    let tx = |t: i64| px0 + (t - t_min) as f64 / (t_max - t_min).max(1) as f64 * (px1 - px0);
    let ty = |v: f64| py1 - (v - lo) / (hi - lo) * (py1 - py0);
    let norm = hi.abs().max(lo.abs()).max(1.0);

    // --- Grid + axis labels ---
    ctx.set_font("12px Arial");
    ctx.set_line_width(1.0);

    // Y ticks.
    ctx.set_text_align("right");
    ctx.set_text_baseline("middle");
    // More decimals when the visible range is narrow, so labels stay distinct.
    let span = hi - lo;
    let decimals = if span < 2.0 {
        2
    } else if span < 20.0 {
        1
    } else {
        0
    };
    let y_ticks = 5;
    for i in 0..=y_ticks {
        let v = lo + span * i as f64 / y_ticks as f64;
        let y = ty(v);
        ctx.set_stroke_style_str(GRID);
        ctx.begin_path();
        ctx.move_to(px0, y);
        ctx.line_to(px1, y);
        ctx.stroke();
        ctx.set_fill_style_str(AXIS_TEXT);
        let _ = ctx.fill_text(&format!("${:.*}", decimals, v), px0 - 8.0, y);
    }

    // X ticks (~6), date shown when the calendar day changes.
    ctx.set_text_align("center");
    ctx.set_text_baseline("top");
    let x_ticks = 6;
    let mut last_day = -1i32;
    for i in 0..=x_ticks {
        let t = t_min + (t_max - t_min) * i / x_ticks;
        let x = tx(t);
        ctx.set_stroke_style_str(GRID);
        ctx.begin_path();
        ctx.move_to(x, py0);
        ctx.line_to(x, py1);
        ctx.stroke();
        let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((t * 1000) as f64));
        let day = d.get_date() as i32;
        let show_date = day != last_day;
        last_day = day;
        ctx.set_fill_style_str(AXIS_TEXT);
        let _ = ctx.fill_text(&time_label(t, show_date), x, py1 + 6.0);
    }

    // --- Price lines ---
    // Segments wider than `max_gap` are skipped, so a period the browser was
    // closed for, or the gap between settled and live data, stays a visible gap
    // instead of an invented straight line.
    ctx.set_line_join("round");
    let plot = |pts: &[Point], width: f64, alpha: f64, max_gap: i64, solid: Option<&str>| {
        if pts.len() < 2 {
            return;
        }
        ctx.set_global_alpha(alpha);
        ctx.set_line_width(width);
        for w in pts.windows(2) {
            let (t0, v0) = w[0];
            let (t1, v1) = w[1];
            if t1 - t0 > max_gap {
                continue;
            }
            match solid {
                Some(c) => ctx.set_stroke_style_str(c),
                None => ctx.set_stroke_style_str(&price_color((v0 + v1) / 2.0 / norm)),
            }
            ctx.begin_path();
            ctx.move_to(tx(t0), ty(v0));
            ctx.line_to(tx(t1), ty(v1));
            ctx.stroke();
        }
        ctx.set_global_alpha(1.0);
    };

    // Day-ahead first, so it sits behind real-time.
    if !da.is_empty() {
        let dash = js_sys::Array::of2(
            &wasm_bindgen::JsValue::from_f64(3.0),
            &wasm_bindgen::JsValue::from_f64(3.0),
        );
        let _ = ctx.set_line_dash(&dash);
        plot(&da, 1.2, 0.75, HISTORY_INTERVAL_SECONDS * 2, Some(DA_LINE));
        let _ = ctx.set_line_dash(&js_sys::Array::new());
    }

    plot(&arch, 1.1, ARCHIVE_ALPHA, HISTORY_INTERVAL_SECONDS * 2, None);
    plot(&live, 1.8, 1.0, INTERVAL_SECONDS * 3, None);

    // --- High / low markers over everything visible ---
    let all: Vec<Point> = arch.iter().chain(live.iter()).copied().collect();
    let (mut min_p, mut max_p) = (all[0], all[0]);
    for &p in &all {
        if p.1 < min_p.1 {
            min_p = p;
        }
        if p.1 > max_p.1 {
            max_p = p;
        }
    }
    let marker = |p: Point, label: &str, above: bool| {
        let (x, y) = (tx(p.0), ty(p.1));
        ctx.set_fill_style_str(&price_color(p.1 / norm));
        ctx.begin_path();
        let _ = ctx.arc(x, y, 3.5, 0.0, std::f64::consts::TAU);
        ctx.fill();
        ctx.set_fill_style_str("#c9d1d9");
        ctx.set_text_align("center");
        ctx.set_text_baseline(if above { "bottom" } else { "top" });
        let _ = ctx.fill_text(label, x, if above { y - 6.0 } else { y + 6.0 });
    };
    marker(max_p, &format!("${:.2}", max_p.1), true);
    marker(min_p, &format!("${:.2}", min_p.1), false);

    // --- Latest actual point: live if we have it, else the newest settled hour.
    let anchor = match live.last().copied().or_else(|| arch.last().copied()) {
        Some(a) => a,
        None => return,
    };
    let (lx, lyv) = (tx(anchor.0), ty(anchor.1));
    ctx.set_fill_style_str("#aaf0c1");
    ctx.begin_path();
    let _ = ctx.arc(lx, lyv, 4.0, 0.0, std::f64::consts::TAU);
    ctx.fill();

    // --- Forecast overlay: 95% band ribbon + dashed mean path ---
    if !forecast.mean.is_empty() {
        // Translucent band: upper edge forward, lower edge back, then fill.
        ctx.set_fill_style_str("rgba(255,170,0,0.13)");
        ctx.begin_path();
        ctx.move_to(lx, lyv);
        for &(t, v) in &forecast.upper {
            ctx.line_to(tx(t), ty(v));
        }
        for &(t, v) in forecast.lower.iter().rev() {
            ctx.line_to(tx(t), ty(v));
        }
        ctx.close_path();
        ctx.fill();

        // Mean path, dashed, continuing from the last actual sample.
        let dash = js_sys::Array::of2(
            &wasm_bindgen::JsValue::from_f64(5.0),
            &wasm_bindgen::JsValue::from_f64(4.0),
        );
        let _ = ctx.set_line_dash(&dash);
        ctx.set_stroke_style_str(FORECAST);
        ctx.set_line_width(1.8);
        ctx.begin_path();
        ctx.move_to(lx, lyv);
        for &(t, v) in &forecast.mean {
            ctx.line_to(tx(t), ty(v));
        }
        ctx.stroke();
        let _ = ctx.set_line_dash(&js_sys::Array::new());
    }

    // --- Legend, listing only what was actually drawn ---
    let mut entries: Vec<LegendEntry> = Vec::new();
    if !live.is_empty() {
        entries.push(LegendEntry {
            label: "real-time · live 5-min",
            color: LEGEND_RT,
            width: 1.8,
            alpha: 1.0,
            dashed: false,
        });
    }
    if !arch.is_empty() {
        entries.push(LegendEntry {
            label: "real-time · settled hourly",
            color: LEGEND_RT,
            width: 1.1,
            alpha: ARCHIVE_ALPHA,
            dashed: false,
        });
    }
    if !da.is_empty() {
        entries.push(LegendEntry {
            label: "day-ahead · settled hourly",
            color: DA_LINE,
            width: 1.2,
            alpha: 0.75,
            dashed: true,
        });
    }
    if !forecast.mean.is_empty() {
        entries.push(LegendEntry {
            label: "forecast · 95% band",
            color: FORECAST,
            width: 1.8,
            alpha: 1.0,
            dashed: true,
        });
    }
    draw_legend(&ctx, &entries, px1, py0);
}
