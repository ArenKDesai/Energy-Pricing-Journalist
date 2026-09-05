//! Client-side ARIMA(p,1,q) + GARCH(1,1) short-term LMP forecast.
//!
//! The mean is modelled with an ARIMA on the price series: first-differenced
//! (d = 1) to handle the near-unit-root behaviour of LMPs, then ARMA(p, q) on
//! the differences with the orders chosen by AIC. ARMA parameters are fit by
//! conditional least squares (minimising the one-step residual sum of squares).
//!
//! The conditional variance of those residuals is modelled with GARCH(1,1),
//! fit by Gaussian maximum likelihood. Multi-step volatility is propagated
//! through the ARIMA MA(∞) weights to give an honest forecast band that widens
//! with horizon and with recent volatility.
//!
//! Everything here is plain `f64` arithmetic and runs in the browser via WASM —
//! both fits use the small Nelder–Mead simplex optimiser below.
//!
//! ## Two sampling grids, one model
//!
//! Two series are available: the live DataBroker feed at 5 minutes, and the
//! settled archive exported from DuckDB at 1 hour. They are never concatenated
//! or interpolated onto a common grid, because that would break both halves of
//! the model: the ARMA recursion indexes by position and so assumes even
//! spacing, and GARCH would be estimating 5-minute conditional volatility from
//! hourly moves — understating it badly, since intra-hour LMP variance is the
//! part hourly settlement averages away. Instead `forecast_hub` fits whichever
//! series is adequate on its own grid and labels which one it used.

use std::cmp::Ordering;

use crate::types::{Point, HISTORY_INTERVAL_SECONDS, INTERVAL_SECONDS};

/// 95% two-sided normal quantile, for the confidence band.
const Z95: f64 = 1.959_964;
/// Below this many samples we fall back to a random walk while history builds.
const MIN_SAMPLES: usize = 48;
/// Largest AR / MA order considered during selection.
const MAX_ORDER: usize = 2;
/// Live-feed horizon: 24 × 5 min = 2 hours.
const LIVE_HORIZON: usize = 24;
/// Archive horizon: 6 × 1 h = 6 hours. Hourly data is smoother and supports a
/// longer useful horizon than the 5-minute feed.
const ARCHIVE_HORIZON: usize = 6;

/// A forecast: aligned mean path and 95% band, plus a human-readable model tag.
#[derive(Clone, Default, PartialEq)]
pub struct Forecast {
    pub mean: Vec<(i64, f64)>,
    pub lower: Vec<(i64, f64)>,
    pub upper: Vec<(i64, f64)>,
    pub label: String,
}

/// Forecast one hub, preferring the live 5-minute feed once it has enough
/// samples to fit and falling back to the settled hourly archive before then.
///
/// Before the archive existed this fell back to a random walk for the first
/// four hours of every fresh browser profile. With history shipped as a static
/// asset the ARIMA-GARCH fit is available from the first frame.
pub fn forecast_hub(live: &[Point], archive: &[Point]) -> Forecast {
    if live.len() >= MIN_SAMPLES || archive.len() < MIN_SAMPLES {
        forecast_series(live, INTERVAL_SECONDS, LIVE_HORIZON, "5-min live")
    } else {
        forecast_series(archive, HISTORY_INTERVAL_SECONDS, ARCHIVE_HORIZON, "hourly archive")
    }
}

/// Describe a horizon in whole hours or minutes, for the chart tag.
fn horizon_label(interval: i64, horizon: usize) -> String {
    let secs = interval * horizon as i64;
    if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}m", secs / 60)
    }
}

/// Forecast `horizon` steps ahead from a uniformly-sampled price series.
///
/// `interval` is the spacing of `series` in seconds; it stamps the forecast
/// timestamps and sets the horizon's real duration. `basis` names the series in
/// the chart tag so it is always visible which grid the fit came from.
pub fn forecast_series(
    series: &[Point],
    interval: i64,
    horizon: usize,
    basis: &str,
) -> Forecast {
    if series.len() < 2 || horizon == 0 || interval <= 0 {
        return Forecast::default();
    }

    let last_ts = series.last().unwrap().0;
    let y: Vec<f64> = series.iter().map(|p| p.1).collect();
    let n = y.len();

    if n < MIN_SAMPLES {
        return random_walk(&y, last_ts, interval, horizon, basis);
    }

    // --- ARIMA mean: difference, center, select ARMA order by AIC ---
    let w: Vec<f64> = (1..n).map(|i| y[i] - y[i - 1]).collect();
    let mu = mean(&w);
    let wc: Vec<f64> = w.iter().map(|x| x - mu).collect();
    let m = wc.len();

    let mut best: Option<(usize, usize, Vec<f64>, Vec<f64>, f64)> = None; // p,q,phi,theta,aic
    for p in 0..=MAX_ORDER {
        for q in 0..=MAX_ORDER {
            let (phi, theta, sse) = fit_arma(&wc, p, q);
            if !sse.is_finite() || sse <= 0.0 {
                continue;
            }
            let k = (p + q) as f64;
            let aic = m as f64 * (sse / m as f64).ln() + 2.0 * (k + 1.0);
            if best.as_ref().map_or(true, |b| aic < b.4) {
                best = Some((p, q, phi, theta, aic));
            }
        }
    }

    let (p, q, phi, theta, _) = match best {
        Some(b) => b,
        None => return random_walk(&y, last_ts, interval, horizon, basis),
    };

    // Residuals of the chosen model, then GARCH(1,1) on them.
    let e = arma_residuals(&wc, &phi, &theta);
    let (omega, alpha, beta) = garch_fit(&e);
    let sig2 = garch_var_series(&e, omega, alpha, beta);
    let sigma2_last = *sig2.last().unwrap();

    // --- Mean path: roll the ARMA recursion forward, then integrate (d=1) ---
    let total = m + horizon;
    let mut wc_full = vec![0.0; total];
    wc_full[..m].copy_from_slice(&wc);
    let mut e_full = vec![0.0; total];
    e_full[..m].copy_from_slice(&e);
    for hh in 0..horizon {
        let t = m + hh;
        let mut pred = 0.0;
        for i in 0..p {
            pred += phi[i] * wc_full[t - 1 - i];
        }
        for j in 0..q {
            if t > j {
                pred += theta[j] * e_full[t - 1 - j]; // future innovations are 0
            }
        }
        wc_full[t] = pred;
    }
    let last_y = y[n - 1];
    let mut level = vec![0.0; horizon];
    let mut cum = last_y;
    for (hh, lvl) in level.iter_mut().enumerate() {
        cum += wc_full[m + hh] + mu;
        *lvl = cum;
    }

    // --- Forecast variance: MA(∞) weights of the ARIMA in levels × GARCH var ---
    let psi = arima_psi_weights(&phi, &theta, q, horizon);
    let esig = garch_multistep_var(&e, sigma2_last, omega, alpha, beta, horizon);

    let mut mean_pts = Vec::with_capacity(horizon);
    let mut lower = Vec::with_capacity(horizon);
    let mut upper = Vec::with_capacity(horizon);
    for hh in 0..horizon {
        let hstep = hh + 1;
        let mut var = 0.0;
        for k in 1..=hstep {
            var += psi[hstep - k] * psi[hstep - k] * esig[k];
        }
        let sd = var.max(0.0).sqrt();
        let ts = last_ts + hstep as i64 * interval;
        mean_pts.push((ts, level[hh]));
        lower.push((ts, level[hh] - Z95 * sd));
        upper.push((ts, level[hh] + Z95 * sd));
    }

    Forecast {
        mean: mean_pts,
        lower,
        upper,
        label: format!(
            "ARIMA({p},1,{q})·GARCH(1,1) · {basis} · next {} · 95% band",
            horizon_label(interval, horizon)
        ),
    }
}

// ---------------------------------------------------------------------------
// ARMA
// ---------------------------------------------------------------------------

/// One-step (conditional) residuals of an ARMA(p,q) on a zero-mean series.
fn arma_residuals(wc: &[f64], phi: &[f64], theta: &[f64]) -> Vec<f64> {
    let m = wc.len();
    let (p, q) = (phi.len(), theta.len());
    let mut e = vec![0.0; m];
    for t in 0..m {
        let mut pred = 0.0;
        for i in 0..p {
            if t > i {
                pred += phi[i] * wc[t - 1 - i];
            }
        }
        for j in 0..q {
            if t > j {
                pred += theta[j] * e[t - 1 - j];
            }
        }
        e[t] = wc[t] - pred;
    }
    e
}

/// Fit ARMA(p,q) by conditional least squares. Returns (phi, theta, SSE).
fn fit_arma(wc: &[f64], p: usize, q: usize) -> (Vec<f64>, Vec<f64>, f64) {
    if p + q == 0 {
        let sse = wc.iter().map(|x| x * x).sum();
        return (Vec::new(), Vec::new(), sse);
    }
    let objective = |params: &[f64]| -> f64 {
        let phi = &params[0..p];
        let theta = &params[p..p + q];
        if !stationary(phi) || !invertible(theta) {
            return 1e12;
        }
        arma_residuals(wc, phi, theta).iter().map(|x| x * x).sum()
    };
    let (best, sse) = nelder_mead(&objective, vec![0.0; p + q], 0.2, 400);
    (best[0..p].to_vec(), best[p..p + q].to_vec(), sse)
}

/// MA(∞) weights ψ₀…ψ_{H-1} of the ARIMA model written in levels, i.e. of
/// Θ(B) / (φ(B)(1-B)). For a random walk these are all 1 (variance ∝ horizon).
fn arima_psi_weights(phi: &[f64], theta: &[f64], q: usize, horizon: usize) -> Vec<f64> {
    // Expanded AR polynomial C(B) = (1 - φ₁B - …)(1 - B).
    let mut c_phi = vec![1.0];
    for &v in phi {
        c_phi.push(-v);
    }
    let c_diff = [1.0, -1.0];
    let mut c = vec![0.0; c_phi.len() + 1];
    for (i, &ci) in c_phi.iter().enumerate() {
        for (j, &dj) in c_diff.iter().enumerate() {
            c[i + j] += ci * dj;
        }
    }
    // Φ_i in the recursion's "1 - Φ₁B - …" convention.
    let big_p = c.len() - 1;
    let phi_exp: Vec<f64> = (0..=big_p).map(|i| if i == 0 { 0.0 } else { -c[i] }).collect();

    let mut psi = vec![0.0; horizon.max(1)];
    psi[0] = 1.0;
    for k in 1..horizon {
        let mut val = if k <= q { theta[k - 1] } else { 0.0 };
        for i in 1..=big_p {
            if k >= i {
                val += phi_exp[i] * psi[k - i];
            }
        }
        psi[k] = val;
    }
    psi
}

// ---------------------------------------------------------------------------
// GARCH(1,1)
// ---------------------------------------------------------------------------

/// Conditional variance series σ²ₜ for given parameters (σ²₀ = sample var).
fn garch_var_series(e: &[f64], omega: f64, alpha: f64, beta: f64) -> Vec<f64> {
    let m = e.len();
    let mut s = vec![0.0; m];
    s[0] = variance(e).max(1e-8);
    for t in 1..m {
        s[t] = omega + alpha * e[t - 1] * e[t - 1] + beta * s[t - 1];
    }
    s
}

/// Fit GARCH(1,1) by Gaussian MLE. Returns (ω, α, β). Falls back to a constant
/// variance (α = β = 0) if the optimiser fails to find a valid fit.
fn garch_fit(e: &[f64]) -> (f64, f64, f64) {
    let var = variance(e).max(1e-8);
    let objective = |x: &[f64]| -> f64 {
        // Unconstrained reparam: ω>0, α,β≥0, α+β<1.
        let omega = x[0].exp();
        let persistence = sigmoid(x[1]); // α+β ∈ (0,1)
        let share = sigmoid(x[2]); // α's share of persistence
        let alpha = persistence * share;
        let beta = persistence * (1.0 - share);
        let mut s = var;
        let mut nll = 0.0;
        for t in 1..e.len() {
            s = omega + alpha * e[t - 1] * e[t - 1] + beta * s;
            if s <= 0.0 || !s.is_finite() {
                return 1e12;
            }
            nll += 0.5 * (s.ln() + e[t] * e[t] / s);
        }
        if nll.is_finite() {
            nll
        } else {
            1e12
        }
    };
    let x0 = vec![(var * 0.1).ln(), 1.0, -1.5];
    let (xb, fb) = nelder_mead(&objective, x0, 0.5, 400);
    if !fb.is_finite() || fb >= 1e12 {
        return (var, 0.0, 0.0);
    }
    let persistence = sigmoid(xb[1]);
    let share = sigmoid(xb[2]);
    (xb[0].exp(), persistence * share, persistence * (1.0 - share))
}

/// Expected variance of the innovation k steps ahead, k = 1..=horizon.
/// `esig[0]` is unused; `esig[k]` = E[σ²_{t+k}].
fn garch_multistep_var(
    e: &[f64],
    sigma2_last: f64,
    omega: f64,
    alpha: f64,
    beta: f64,
    horizon: usize,
) -> Vec<f64> {
    let persistence = alpha + beta;
    let last_e2 = e.last().map_or(0.0, |x| x * x);
    let mut esig = vec![0.0; horizon + 1];
    if horizon >= 1 {
        esig[1] = omega + alpha * last_e2 + beta * sigma2_last;
    }
    for k in 2..=horizon {
        esig[k] = omega + persistence * esig[k - 1];
    }
    esig
}

// ---------------------------------------------------------------------------
// Fallback + helpers
// ---------------------------------------------------------------------------

/// Random walk with drift — used only when neither series has enough samples
/// to fit ARIMA, which now means the archive asset failed to load as well.
fn random_walk(
    y: &[f64],
    last_ts: i64,
    interval: i64,
    horizon: usize,
    basis: &str,
) -> Forecast {
    let n = y.len();
    let diffs: Vec<f64> = (1..n).map(|i| y[i] - y[i - 1]).collect();
    let drift = if diffs.is_empty() { 0.0 } else { mean(&diffs) };
    let var = variance(&diffs);
    let last = y[n - 1];

    let mut mean_pts = Vec::with_capacity(horizon);
    let mut lower = Vec::with_capacity(horizon);
    let mut upper = Vec::with_capacity(horizon);
    for k in 1..=horizon {
        let ts = last_ts + k as i64 * interval;
        let yhat = last + drift * k as f64;
        let sd = (var * k as f64).max(0.0).sqrt();
        mean_pts.push((ts, yhat));
        lower.push((ts, yhat - Z95 * sd));
        upper.push((ts, yhat + Z95 * sd));
    }
    Forecast {
        mean: mean_pts,
        lower,
        upper,
        label: format!(
            "random walk + drift · {basis} · collecting history ({n}/{MIN_SAMPLES})"
        ),
    }
}

fn mean(x: &[f64]) -> f64 {
    if x.is_empty() {
        0.0
    } else {
        x.iter().sum::<f64>() / x.len() as f64
    }
}

fn variance(x: &[f64]) -> f64 {
    if x.len() < 2 {
        return 0.0;
    }
    let mu = mean(x);
    x.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / (x.len() - 1) as f64
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// AR stationarity for orders ≤ 2 (the unit-root triangle); soft-true above.
fn stationary(phi: &[f64]) -> bool {
    match phi.len() {
        0 => true,
        1 => phi[0].abs() < 0.999,
        2 => phi[1].abs() < 0.999 && phi[0] + phi[1] < 0.999 && phi[1] - phi[0] < 0.999,
        _ => true,
    }
}

/// MA invertibility ⇔ stationarity of the polynomial with negated coefficients.
fn invertible(theta: &[f64]) -> bool {
    let neg: Vec<f64> = theta.iter().map(|t| -t).collect();
    stationary(&neg)
}

/// Minimal Nelder–Mead simplex minimiser. Returns (argmin, min value).
fn nelder_mead<F: Fn(&[f64]) -> f64>(
    f: &F,
    x0: Vec<f64>,
    step: f64,
    iters: usize,
) -> (Vec<f64>, f64) {
    let n = x0.len();
    if n == 0 {
        return (x0.clone(), f(&x0));
    }
    let (refl, expand, contract, shrink) = (1.0, 2.0, 0.5, 0.5);

    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(n + 1);
    simplex.push(x0.clone());
    for i in 0..n {
        let mut v = x0.clone();
        v[i] += step;
        simplex.push(v);
    }
    let mut fv: Vec<f64> = simplex.iter().map(|p| f(p)).collect();

    for _ in 0..iters {
        let mut order: Vec<usize> = (0..=n).collect();
        order.sort_by(|&a, &b| fv[a].partial_cmp(&fv[b]).unwrap_or(Ordering::Equal));
        let (best, second, worst) = (order[0], order[n - 1], order[n]);

        let mut centroid = vec![0.0; n];
        for &k in order.iter().take(n) {
            for j in 0..n {
                centroid[j] += simplex[k][j];
            }
        }
        for c in &mut centroid {
            *c /= n as f64;
        }

        let reflected: Vec<f64> = (0..n)
            .map(|j| centroid[j] + refl * (centroid[j] - simplex[worst][j]))
            .collect();
        let fr = f(&reflected);

        if fr < fv[best] {
            let expanded: Vec<f64> = (0..n)
                .map(|j| centroid[j] + expand * (reflected[j] - centroid[j]))
                .collect();
            let fe = f(&expanded);
            if fe < fr {
                simplex[worst] = expanded;
                fv[worst] = fe;
            } else {
                simplex[worst] = reflected;
                fv[worst] = fr;
            }
        } else if fr < fv[second] {
            simplex[worst] = reflected;
            fv[worst] = fr;
        } else {
            let contracted: Vec<f64> = (0..n)
                .map(|j| centroid[j] + contract * (simplex[worst][j] - centroid[j]))
                .collect();
            let fc = f(&contracted);
            if fc < fv[worst] {
                simplex[worst] = contracted;
                fv[worst] = fc;
            } else {
                let bx = simplex[best].clone();
                for &k in order.iter().skip(1) {
                    for j in 0..n {
                        simplex[k][j] = bx[j] + shrink * (simplex[k][j] - bx[j]);
                    }
                    fv[k] = f(&simplex[k]);
                }
            }
        }
    }

    let mut bi = 0;
    for i in 1..=n {
        if fv[i] < fv[bi] {
            bi = i;
        }
    }
    (simplex[bi].clone(), fv[bi])
}
