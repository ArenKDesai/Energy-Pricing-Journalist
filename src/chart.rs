//! Canvas2D line chart for a single HUB's LMP series, with axes, high/low
//! markers, the latest value, and an optional dashed forecast overlay.

use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::model::Forecast;
use crate::types::{now_secs, Snapshot, INTERVAL_SECONDS, WINDOW_SECONDS};

const BG: &str = "#0d1117";
const GRID: &str = "#21262d";
const AXIS_TEXT: &str = "#8b949e";
const FORECAST: &str = "#ffaa00";

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

/// A point in data space: (epoch seconds, LMP).
type Point = (i64, f64);

/// Mint → amber → red for positive, mint → cyan → purple for negative, where
/// `t` is the value normalized into [-1, 1]. Mirrors the original WebGPU shader.
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

/// Render the chart. `forecast` carries the ARIMA-GARCH mean path and 95% band,
/// drawn as a dashed line and translucent ribbon continuing from the last
/// actual sample.
pub fn render(
    canvas: &HtmlCanvasElement,
    snapshots: &[Snapshot],
    hub: &str,
    forecast: &Forecast,
) {
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

    // Collect points for the selected hub.
    let pts: Vec<Point> = snapshots
        .iter()
        .filter_map(|s| s.lmp(hub).map(|v| (s.ts, v)))
        .collect();
    if pts.is_empty() {
        return;
    }

    // Plot area.
    let px0 = MARGIN.left;
    let px1 = css_w - MARGIN.right;
    let py0 = MARGIN.top;
    let py1 = css_h - MARGIN.bottom;
    if px1 <= px0 || py1 <= py0 {
        return;
    }

    // X domain: the last 3 days up to now, but never narrower than 1h so a
    // freshly-seeded chart still reads sensibly.
    let now = now_secs();
    let earliest = pts.first().map(|p| p.0).unwrap_or(now);
    let t_max = now.max(forecast.mean.last().map(|p| p.0).unwrap_or(now));
    let mut t_min = (now - WINDOW_SECONDS).max(earliest);
    if t_max - t_min < INTERVAL_SECONDS * 12 {
        t_min = t_max - INTERVAL_SECONDS * 12;
    }

    // Y domain over actual + forecast values, padded 8%.
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for (_, v) in pts
        .iter()
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

    // --- Price line, colored per segment by normalized value ---
    ctx.set_line_width(1.8);
    ctx.set_line_join("round");
    for w in pts.windows(2) {
        let (t0, v0) = w[0];
        let (t1, v1) = w[1];
        ctx.set_stroke_style_str(&price_color((v0 + v1) / 2.0 / norm));
        ctx.begin_path();
        ctx.move_to(tx(t0), ty(v0));
        ctx.line_to(tx(t1), ty(v1));
        ctx.stroke();
    }

    // --- High / low markers ---
    let (mut min_p, mut max_p) = (pts[0], pts[0]);
    for &p in &pts {
        if p.1 < min_p.1 {
            min_p = p;
        }
        if p.1 > max_p.1 {
            max_p = p;
        }
    }
    let marker = |ctx: &CanvasRenderingContext2d, p: Point, label: &str, above: bool| {
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
    marker(&ctx, max_p, &format!("${:.2}", max_p.1), true);
    marker(&ctx, min_p, &format!("${:.2}", min_p.1), false);

    // --- Latest actual point, highlighted ---
    let last = *pts.last().unwrap();
    let (lx, lyv) = (tx(last.0), ty(last.1));
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
}
