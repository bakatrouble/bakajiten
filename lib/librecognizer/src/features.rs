//! Ink preprocessing + Bézier-curve feature encoding.
//!
//! Faithful port of the production pipeline as recovered by independent
//! reverse-engineering (validated 74/74 against the real SDK — notably,
//! the 2019 paper's feature order is WRONG; see below).
//!
//! Pipeline per stroke: thin (near-duplicate collapse) -> time rescale
//! (`t *= spatial_len/duration`, no origin shift) -> recursive split ->
//! merge -> refit -> 10-dim encode. Preprocessing: global `t -= t0`,
//! hallucinate all-20ms timestamps only on x/t length mismatch, ink-bbox
//! size normalization, 2-point pen-up bridges fitted as real strokes.
//!
//! Feature layout (native `CoeffsToFeaturesAnglesRatios`):
//! `(pen_down, dx, dy, a1, d1, a2, d2, dt, start_leg, end_leg)` with
//! pen_down = 1.0 for real strokes / 0.0 for pen-up bridges, signed angles,
//! and Bézier time legs (`dt = T3−T0`, `start = T1−T0`, `end = T2−T3`).

use crate::ink::Ink;

/// Curve-fit acceptance parameters (mirror `CurveSettings`, = recospec values).
#[derive(Debug, Clone)]
pub struct FeatureConfig {
    /// Corner-neighbour radius / stroke bbox diagonal.
    pub tol1: f64, // feature_params.1 = 0.05
    /// Max spatial residual / bbox diagonal.
    pub tol2: f64, // feature_params.2 = 0.02
    /// RMS spatial residual / bbox diagonal.
    pub tol3: f64, // feature_params.3 = 0.01
    /// Control-polygon / endpoint-distance ratio limit.
    pub max_arc_ratio: f64, // feature_params.4 = 3.0
    /// Minimum adjacent control-leg cosine.
    pub split_cos_threshold: f64, // feature_params.5 = -0.8
    /// Newton/fit iterations per curve.
    pub fit_iters: usize,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            tol1: 0.05,
            tol2: 0.02,
            tol3: 0.01,
            max_arc_ratio: 3.0,
            split_cos_threshold: -0.8,
            fit_iters: 8,
        }
    }
}

type Pt = [f64; 3]; // (x, y, t)

/// One stroke with a pen flag. Bridges (pen_up) are fitted like real strokes.
#[derive(Clone)]
struct Stroke {
    pts: Vec<Pt>,
    pen_up: bool,
}
// ---------------------------------------------------------------------------
// cubic fitting (paper eq. 3-6, native schedule)
// ---------------------------------------------------------------------------

/// Solve 4x4 linear system `A x = b` (Gaussian elimination, partial pivot).
fn solve4(a: [[f64; 4]; 4], b: [f64; 4]) -> [f64; 4] {
    let mut m = [[0.0f64; 5]; 4];
    for i in 0..4 {
        for j in 0..4 {
            m[i][j] = a[i][j];
        }
        m[i][4] = b[i];
    }
    for col in 0..4 {
        let mut piv = col;
        for r in col + 1..4 {
            if m[r][col].abs() > m[piv][col].abs() {
                piv = r;
            }
        }
        m.swap(col, piv);
        let d = m[col][col];
        if d.abs() < 1e-18 {
            continue;
        }
        for r in 0..4 {
            if r != col {
                let f = m[r][col] / d;
                for c in col..5 {
                    m[r][c] -= f * m[col][c];
                }
            }
        }
    }
    [0, 1, 2, 3].map(|i| {
        let d = m[i][i];
        if d.abs() < 1e-18 {
            0.0
        } else {
            m[i][4] / d
        }
    })
}

/// Least-squares power-basis fit. N<4: fit degree N-1, zero-pad (a 2-point
/// segment is an exact straight line). N==1: singleton at the point.
fn fit_omega(pts: &[Pt], s: &[f64]) -> [[f64; 3]; 4] {
    let n = pts.len();
    assert_eq!(n, s.len());
    if n == 1 {
        let mut o = [[0.0; 3]; 4];
        o[0][0] = pts[0][0];
        o[0][1] = pts[0][1];
        return o; // time polynomial stays zero (native singleton path)
    }
    let mut mtm = [[0.0f64; 4]; 4];
    let mut mtz = [[0.0f64; 3]; 4];
    for (p, &si) in pts.iter().zip(s) {
        let v = [1.0, si, si * si, si * si * si];
        for i in 0..4 {
            for j in 0..4 {
                mtm[i][j] += v[i] * v[j];
            }
            for k in 0..3 {
                mtz[i][k] += v[i] * p[k];
            }
        }
    }
    let mut omega = [[0.0f64; 3]; 4];
    if n >= 4 {
        for k in 0..3 {
            let col = solve4(mtm, [mtz[0][k], mtz[1][k], mtz[2][k], mtz[3][k]]);
            for i in 0..4 {
                omega[i][k] = col[i];
            }
        }
        return omega;
    }
    // Underdetermined: highest exactly-determined degree, zero-padded.
    let mut sub_mtm = [[0.0f64; 4]; 4];
    let mut sub_mtz = [[0.0f64; 3]; 4];
    for (p, &si) in pts.iter().zip(s) {
        let mut v = [0.0; 4];
        for i in 0..n {
            v[i] = si.powi(i as i32);
        }
        for i in 0..n {
            for j in 0..n {
                sub_mtm[i][j] += v[i] * v[j];
            }
            for k in 0..3 {
                sub_mtz[i][k] += v[i] * p[k];
            }
        }
    }
    for k in 0..3 {
        let mut b = [0.0; 4];
        for i in 0..n {
            b[i] = sub_mtz[i][k];
        }
        let mut a = [[0.0; 4]; 4];
        for i in 0..n {
            for j in 0..n {
                a[i][j] = sub_mtm[i][j];
            }
            a[i][i] += 1e-12;
        }
        let col = solve4(a, b);
        for i in 0..n {
            omega[i][k] = col[i];
        }
    }
    omega
}

fn eval_poly(o: &[f64; 4], s: f64) -> f64 {
    o[0] + s * (o[1] + s * (o[2] + s * o[3]))
}
fn eval_deriv(o: &[f64; 4], s: f64) -> f64 {
    o[1] + s * (2.0 * o[2] + s * 3.0 * o[3])
}
fn eval_deriv2(o: &[f64; 4], s: f64) -> f64 {
    2.0 * o[2] + s * 6.0 * o[3]
}

/// Sum of squared errors (eq.3, includes time).
fn sse_of(pts: &[Pt], s: &[f64], omega: &[[f64; 3]; 4]) -> f64 {
    let mut e = 0.0;
    for (p, &si) in pts.iter().zip(s) {
        for k in 0..3 {
            let d = eval_poly(&[omega[0][k], omega[1][k], omega[2][k], omega[3][k]], si) - p[k];
            e += d * d;
        }
    }
    e
}

/// Chord-length s in [0,1]; uniform fallback for zero spatial extent.
fn initial_s(pts: &[Pt]) -> Vec<f64> {
    let n = pts.len();
    let mut s = vec![0.0; n];
    if n > 1 {
        let mut dist = vec![0.0; n];
        for i in 1..n {
            let dx = pts[i][0] - pts[i - 1][0];
            let dy = pts[i][1] - pts[i - 1][1];
            dist[i] = dist[i - 1] + (dx * dx + dy * dy).sqrt();
        }
        let total = dist[n - 1];
        if total > 1e-12 {
            for i in 0..n {
                s[i] = dist[i] / total;
            }
        } else {
            for i in 0..n {
                s[i] = i as f64 / (n - 1).max(1) as f64;
            }
        }
    }
    s
}

/// Fit one cubic: alternate least squares with Newton reprojection (eq.6,
/// x/y only). A Newton step that does not strictly descend is rejected
/// (keeps the previous fit) rather than letting s wander into degeneracy.
fn fit_curve(pts: &[Pt], iters: usize) -> ([[f64; 3]; 4], f64, Vec<f64>) {
    let n = pts.len();
    let mut s = initial_s(pts);
    let mut omega = fit_omega(pts, &s);
    let mut sse = sse_of(pts, &s, &omega);
    for _ in 0..iters {
        let ox = [omega[0][0], omega[1][0], omega[2][0], omega[3][0]];
        let oy = [omega[0][1], omega[1][1], omega[2][1], omega[3][1]];
        let mut s_new = s.clone();
        for i in 0..n {
            let si = s[i];
            let fx = eval_poly(&ox, si) - pts[i][0];
            let fy = eval_poly(&oy, si) - pts[i][1];
            let dx = eval_deriv(&ox, si);
            let dy = eval_deriv(&oy, si);
            let num = dx * fx + dy * fy;
            let den = eval_deriv2(&ox, si) * fx - dx * dx
                + eval_deriv2(&oy, si) * fy
                - dy * dy;
            if den.abs() > 1e-12 {
                s_new[i] = (si - num / den).clamp(0.0, 1.0);
            }
        }
        let omega_new = fit_omega(pts, &s_new);
        let sse_new = sse_of(pts, &s_new, &omega_new);
        if sse_new > sse - 1e-12 {
            break;
        }
        s = s_new;
        omega = omega_new;
        sse = sse_new;
        if std::env::var("TEGAKI_DEBUG").is_ok() {
            eprintln!("  iter sse={sse:.5}");
        }
    }
    if std::env::var("TEGAKI_DEBUG2").is_ok() {
        eprintln!(
            "fit n={n} sse={sse:.4} tspan=[{:.3},{:.3}] gam=[{:.2},{:.2},{:.2},{:.2}]",
            pts.first().map(|p| p[2]).unwrap_or(0.0),
            pts.last().map(|p| p[2]).unwrap_or(0.0),
            omega[0][2],
            omega[1][2],
            omega[2][2],
            omega[3][2]
        );
    }
    (omega, sse, s)
}
/// Bbox diagonal of point set (spatial only).
fn bbox_diag(pts: &[Pt]) -> f64 {
    if pts.is_empty() {
        return 0.0;
    }
    let (mut mnx, mut mxx, mut mny, mut mxy) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in pts {
        mnx = mnx.min(p[0]);
        mxx = mxx.max(p[0]);
        mny = mny.min(p[1]);
        mxy = mxy.max(p[1]);
    }
    ((mxx - mnx).powi(2) + (mxy - mny).powi(2)).sqrt()
}

/// Max-cardinality minimum-spacing chain (thinning). Keeps a max-size subset
/// where consecutive kept points exceed `e` in BOTH cumulative length and
/// Euclidean distance. `e` scales with the whole-stroke bbox diagonal.
fn thin_points(pts: &[Pt]) -> Vec<Pt> {
    let n = pts.len();
    if n < 2 {
        return pts.to_vec();
    }
    let mut dist = vec![0.0; n];
    for i in 1..n {
        dist[i] = dist[i - 1]
            + ((pts[i][0] - pts[i - 1][0]).powi(2) + (pts[i][1] - pts[i - 1][1]).powi(2)).sqrt();
    }
    let e = bbox_diag(pts) * 0.0003452669770922512;
    let e2 = e * e;
    let mut length = vec![1usize; n];
    let mut pred: Vec<usize> = (0..n).collect();
    let mut prefixmax = vec![1usize; n];
    let mut best = 0usize;
    for i in 1..n {
        pred[i] = i;
        let mut j = i;
        loop {
            if j == 0 {
                break;
            }
            j -= 1;
            if length[i] > prefixmax[j] + 1 {
                break;
            }
            let candidate = length[j] + 1;
            if candidate >= length[i]
                && dist[i] - dist[j] > e
                && {
                    let dx = pts[i][0] - pts[j][0];
                    let dy = pts[i][1] - pts[j][1];
                    dx * dx + dy * dy > e2
                }
            {
                length[i] = candidate;
                pred[i] = j;
                if length[i] >= length[best] {
                    best = i;
                }
            }
        }
        prefixmax[i] = length[best];
    }
    let mut idx = Vec::with_capacity(length[best]);
    let mut cur = best;
    for _ in 0..length[best] {
        idx.push(cur);
        cur = pred[cur];
    }
    idx.reverse();
    idx.into_iter().map(|i| pts[i]).collect()
}

/// Per-stroke fitter time rescale (after thinning): `t *= L/duration` with L
/// = spatial length over retained points. No origin subtraction; nonpositive
/// duration zeroes the time column.
fn rescale_time(pts: &mut [Pt]) {
    if pts.is_empty() {
        return;
    }
    let duration = pts[pts.len() - 1][2] - pts[0][2];
    if !(duration > 0.0) {
        for p in pts.iter_mut() {
            p[2] = 0.0;
        }
        return;
    }
    let mut len = 0.0;
    for w in pts.windows(2) {
        len += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
    }
    let k = len / duration;
    for p in pts.iter_mut() {
        p[2] *= k;
    }
}

/// Acceptance gates: max/RMS *spatial* residual vs whole-stroke bbox
/// diagonal, plus (N>=4) the control-polygon gate. Returns
/// (residual_fail, bend_fail).
fn fit_failures(
    pts: &[Pt],
    omega: &[[f64; 3]; 4],
    s: &[f64],
    cfg: &FeatureConfig,
    diag: f64,
) -> (bool, bool) {
    let mut max_e = 0.0f64;
    let mut sum = 0.0f64;
    for (p, &si) in pts.iter().zip(s) {
        let dx = eval_poly(&[omega[0][0], omega[1][0], omega[2][0], omega[3][0]], si) - p[0];
        let dy = eval_poly(&[omega[0][1], omega[1][1], omega[2][1], omega[3][1]], si) - p[1];
        let e = dx * dx + dy * dy;
        max_e = max_e.max(e);
        sum += e;
    }
    let max_err = max_e.sqrt();
    let rms_err = (sum / pts.len().max(1) as f64).sqrt();
    let wrong = max_err > cfg.tol2 * diag || rms_err > cfg.tol3 * diag;
    if pts.len() < 4 {
        return (wrong, false);
    }
    let cp = control_points(omega);
    let (p0, p1, p2, p3) = (cp[0], cp[1], cp[2], cp[3]);
    let (vx, vy) = (p1[0] - p0[0], p1[1] - p0[1]);
    let (mx, my) = (p2[0] - p1[0], p2[1] - p1[1]);
    let (wx, wy) = (p2[0] - p3[0], p2[1] - p3[1]);
    let (lv, lm, lw) = (
        (vx * vx + vy * vy).sqrt(),
        (mx * mx + my * my).sqrt(),
        (wx * wx + wy * wy).sqrt(),
    );
    let chord = ((p3[0] - p0[0]).powi(2) + (p3[1] - p0[1]).powi(2)).sqrt();
    let ratio = (lv + lm + lw) / chord; // NaN/Inf if chord == 0
    let a = (vx * mx + vy * my) / lv; // may be +-inf/NaN on degenerate legs
    let b = -(wx * mx + wy * my) / lw;
    // x86 MINSS semantics: pick `a` unless b is strictly less (NaN -> a).
    let minimum = if b < a { b } else { a };
    let score = minimum / lm;
    let bend = !(chord > 0.0 && ratio <= cfg.max_arc_ratio && score >= cfg.split_cos_threshold);
    (wrong, bend)
}

/// Earliest argmin corner with neighbours at radius `tol1*diag` on both sides.
fn split_at_min_angle(pts: &[Pt], radius: f64) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None; // (score, idx), strictly decreasing
    for i in 1..pts.len().saturating_sub(1) {
        let mut l = i - 1;
        while l > 0
            && ((pts[i][0] - pts[l][0]).powi(2) + (pts[i][1] - pts[l][1]).powi(2)).sqrt() < radius
        {
            l -= 1;
        }
        let mut r = i + 1;
        while r + 1 < pts.len()
            && ((pts[r][0] - pts[i][0]).powi(2) + (pts[r][1] - pts[i][1]).powi(2)).sqrt() < radius
        {
            r += 1;
        }
        // Exhausting either side skips the candidate.
        let dl = ((pts[i][0] - pts[l][0]).powi(2) + (pts[i][1] - pts[l][1]).powi(2)).sqrt();
        let dr = ((pts[r][0] - pts[i][0]).powi(2) + (pts[r][1] - pts[i][1]).powi(2)).sqrt();
        if dl < radius || dr < radius {
            continue;
        }
        let (ax, ay) = (pts[i][0] - pts[l][0], pts[i][1] - pts[l][1]);
        let (bx, by) = (pts[r][0] - pts[i][0], pts[r][1] - pts[i][1]);
        let denom = (ax * ax + ay * ay).sqrt() * (bx * bx + by * by).sqrt();
        if denom <= 0.0 {
            continue;
        }
        let score = (ax * bx + ay * by) / denom;
        if score < best.map(|(s, _)| s).unwrap_or(1.0) {
            best = Some((score, i));
        }
    }
    best.map(|(_, i)| i)
}

/// Max-curvature split: 100-sample interior grid, nearest input parameter.
fn split_at_max_curvature(pts: &[Pt], omega: &[[f64; 3]; 4], s: &[f64]) -> usize {
    let n = pts.len();
    let (a, b) = (s[1], s[n - 2]);
    let mut best_ss = a;
    let mut best_k = f64::NEG_INFINITY;
    let ox = [omega[0][0], omega[1][0], omega[2][0], omega[3][0]];
    let oy = [omega[0][1], omega[1][1], omega[2][1], omega[3][1]];
    for j in 0..100 {
        let f = j as f64 / 99.0;
        let ss = if a.abs() <= b.abs() { a + (b - a) * f } else { b + (a - b) * (1.0 - f) };
        let ss = if j == 99 { b } else { ss };
        let dx = eval_deriv(&ox, ss);
        let dy = eval_deriv(&oy, ss);
        let ddx = eval_deriv2(&ox, ss);
        let ddy = eval_deriv2(&oy, ss);
        let num = (dx * ddy - dy * ddx).abs();
        let sp2 = dx * dx + dy * dy;
        let den = (sp2 * sp2 * sp2).sqrt();
        let kappa = if den > 0.0 { num / den } else { 0.0 };
        if kappa > best_k {
            best_k = kappa;
            best_ss = ss;
        }
    }
    // Earliest nearest input parameter among interior points.
    let mut best_i = 1;
    let mut best_d = f64::INFINITY;
    for (i, &si) in s.iter().enumerate().take(n - 1).skip(1) {
        let d = (si - best_ss).abs();
        if d < best_d {
            best_d = d;
            best_i = i;
        }
    }
    best_i.clamp(1, n - 2)
}

/// Recursively split point set; carries the ORIGINAL stroke scale.
/// N<4 segments are accepted as-is (exact fit, no geometric gate).
fn split_points(pts: &[Pt], cfg: &FeatureConfig, diag: f64) -> Vec<Vec<Pt>> {
    if pts.len() < 4 {
        return vec![pts.to_vec()];
    }
    let (omega, _, s) = fit_curve(pts, cfg.fit_iters);
    let (wrong, bend) = fit_failures(pts, &omega, &s, cfg, diag);
    if !(wrong || bend) {
        return vec![pts.to_vec()];
    }
    let mut idx = if wrong {
        split_at_min_angle(pts, cfg.tol1 * diag)
    } else {
        None
    };
    if idx.is_none() {
        if !bend {
            return vec![pts.to_vec()];
        }
        idx = Some(split_at_max_curvature(pts, &omega, &s));
    }
    let k = idx.unwrap().clamp(1, pts.len() - 2);
    let mut out = split_points(&pts[..=k], cfg, diag);
    out.extend(split_points(&pts[k..], cfg, diag));
    out
}

/// Merge adjacent segments when the combined fit passes the gates.
/// Operates on point sets (drops the duplicated joint), repeats to fixpoint.
fn merge_segments(segs: Vec<Vec<Pt>>, cfg: &FeatureConfig, diag: f64) -> Vec<Vec<Pt>> {
    let mut segs = segs;
    if segs.len() < 2 {
        return segs;
    }
    loop {
        let mut changed = false;
        let mut out: Vec<Vec<Pt>> = Vec::with_capacity(segs.len());
        let mut i = 0;
        while i < segs.len() {
            if i + 1 < segs.len() {
                let mut combined = segs[i].clone();
                combined.extend_from_slice(&segs[i + 1][1..]);
                let (omega, _, s) = fit_curve(&combined, cfg.fit_iters);
                let (wrong, bend) = fit_failures(&combined, &omega, &s, cfg, diag);
                if !wrong && !bend {
                    out.push(combined);
                    i += 2;
                    changed = true;
                    continue;
                }
            }
            out.push(segs[i].clone());
            i += 1;
        }
        segs = out;
        if !changed {
            break;
        }
    }
    segs
}

/// Power basis -> Bézier control points (x/y).
fn control_points(omega: &[[f64; 3]; 4]) -> [[f64; 2]; 4] {
    let (ax, ay) = ([omega[0][0], omega[1][0], omega[2][0], omega[3][0]], [omega[0][1], omega[1][1], omega[2][1], omega[3][1]]);
    let p0 = [ax[0], ay[0]];
    let p1 = [ax[0] + ax[1] / 3.0, ay[0] + ay[1] / 3.0];
    let p2 = [2.0 * p1[0] - p0[0] + ax[2] / 3.0, 2.0 * p1[1] - p0[1] + ay[2] / 3.0];
    let p3 = [ax[0] + ax[1] + ax[2] + ax[3], ay[0] + ay[1] + ay[2] + ay[3]];
    [p0, p1, p2, p3]
}

/// Encode one curve as the native 10-vector:
/// `(pen_down, dx, dy, a1, d1, a2, d2, dt, start_leg, end_leg)`.
/// `a1` = signed D→V angle, `a2` = signed −D→W angle (atan2 unconditional);
/// `d1/d2` are control-arm / endpoint ratios (0 when L == 0 exactly);
/// time legs come straight from the power-basis γ (no cancellation).
fn encode_curve(omega: &[[f64; 3]; 4], pen_down: bool) -> [f32; 10] {
    let [p0, p1, p2, p3] = control_points(omega);
    let (ex, ey) = (p3[0] - p0[0], p3[1] - p0[1]);
    let len = (ex * ex + ey * ey).sqrt();
    let (vx, vy) = (p1[0] - p0[0], p1[1] - p0[1]);
    let (wx, wy) = (p2[0] - p3[0], p2[1] - p3[1]);
    let (d1, d2) = if len > 0.0 {
        ((vx * vx + vy * vy).sqrt() / len, (wx * wx + wy * wy).sqrt() / len)
    } else {
        (0.0, 0.0)
    };
    let a1 = (ex * vy - ey * vx).atan2(ex * vx + ey * vy);
    let a2 = (ey * wx - ex * wy).atan2(-(ex * wx + ey * wy));
    let (g1, g2, g3) = (omega[1][2], omega[2][2], omega[3][2]);
    let dt = g1 + g2 + g3;
    let start_leg = g1 / 3.0;
    let end_leg = -g1 / 3.0 - 2.0 * g2 / 3.0 - g3;
    [
        if pen_down { 1.0 } else { 0.0 },
        ex as f32,
        ey as f32,
        a1 as f32,
        d1 as f32,
        a2 as f32,
        d2 as f32,
        dt as f32,
        start_leg as f32,
        end_leg as f32,
    ]
}

/// Full pipeline: ink -> time-major feature sequence.
/// Returns (features, curve count). Gaps are real fitted 2-point strokes.
pub fn extract(ink: &Ink, _area: Option<(f32, f32)>, cfg: &FeatureConfig) -> (Vec<[f32; 10]>, usize) {
    // 1-2. Collect strokes; global t origin; hallucinate if lengths mismatch.
    // (Ink always carries t per point; the GUI/CLI synthetic-20ms fallback in
    // Ink::from_raw covers missing timestamps, so lengths always match here.)
    let mut strokes: Vec<Stroke> = Vec::new();
    for s in &ink.strokes {
        if s.points.is_empty() {
            continue;
        }
        strokes.push(Stroke {
            pts: s.points.iter().map(|p| [p.x as f64, p.y as f64, p.t as f64]).collect(),
            pen_up: false,
        });
    }
    if strokes.is_empty() {
        return (vec![], 0);
    }
    normalize_time_strokes(&mut strokes);
    // (x/t length mismatch cannot happen here: Ink always carries t per point.
    // The GUI/CLI synthetic-20ms fallback in Ink::from_raw covers missing t.)
    // 3. Size normalize (no writing guide): ink bbox.
    normalize_size_strokes(&mut strokes);
    // 4. Pen-up bridges (fitted as real strokes downstream).
    let mut seq: Vec<Stroke> = Vec::with_capacity(strokes.len() * 2);
    for (i, s) in strokes.iter().enumerate() {
        seq.push(s.clone());
        if i + 1 < strokes.len() {
            let a = *s.pts.last().unwrap();
            let b = strokes[i + 1].pts[0];
            seq.push(Stroke { pts: vec![a, b], pen_up: true });
        }
    }
    // 5. Per stroke: thin -> rescale time -> split -> merge -> refit -> encode.
    let mut feats = Vec::new();
    let mut n_curves = 0;
    for stroke in &seq {
        if stroke.pts.is_empty() {
            continue;
        }
        let diag = bbox_diag(&stroke.pts);
        let mut pts = thin_points(&stroke.pts);
        rescale_time(&mut pts);
        let segs = split_points(&pts, cfg, diag);
        let merged = merge_segments(segs, cfg, diag);
        for seg in &merged {
            let (omega, _, _) = fit_curve(seg, cfg.fit_iters);
            feats.push(encode_curve(&omega, !stroke.pen_up));
            n_curves += 1;
            if std::env::var("TEGAKI_DEBUG3").is_ok() {
                let cp = control_points(&omega);
                eprintln!(
                    "curve n={} p0=({:.3},{:.3}) p1=({:.3},{:.3}) p2=({:.3},{:.3}) p3=({:.3},{:.3})",
                    seg.len(),
                    cp[0][0],
                    cp[0][1],
                    cp[1][0],
                    cp[1][1],
                    cp[2][0],
                    cp[2][1],
                    cp[3][0],
                    cp[3][1]
                );
            }
        }
    }
    (feats, n_curves)
}

fn normalize_time_strokes(strokes: &mut [Stroke]) {
    let t0 = strokes.iter().flat_map(|s| s.pts.first().map(|p| p[2])).next();
    let Some(t0) = t0 else { return };
    for s in strokes.iter_mut() {
        for p in s.pts.iter_mut() {
            p[2] -= t0;
        }
    }
}

fn normalize_size_strokes(strokes: &mut [Stroke]) {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for s in strokes.iter() {
        for p in &s.pts {
            xs.push(p[0]);
            ys.push(p[1]);
        }
    }
    if xs.is_empty() {
        return;
    }
    let (mut xmin, mut xmax) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
    for (&x, &y) in xs.iter().zip(&ys) {
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let _ = xmin;
    let mut h = (ymax - ymin).max((xmax - xmin) / 100.0);
    if h < 2f64.powi(-23) {
        h = 1.0;
    }
    let x0 = strokes.iter().flat_map(|s| s.pts.first()).next().map(|p| p[0]).unwrap_or(0.0);
    for s in strokes.iter_mut() {
        for p in s.pts.iter_mut() {
            p[0] = (p[0] - x0) / h;
            p[1] = (p[1] - ymin) / h;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::{Ink, Point, Stroke as InkStroke};

    fn line_ink() -> Ink {
        Ink {
            strokes: vec![InkStroke {
                points: (0..11)
                    .map(|i| Point { x: i as f32 * 10.0, y: 5.0, t: i as f32 * 10.0 })
                    .collect(),
            }],
        }
    }

    #[test]
    fn straight_line_features() {
        // Straight horizontal line: pen_down=1, dx>0, dy~0, d1=d2=1/3, angles~0.
        let (feats, n) = extract(&line_ink(), None, &FeatureConfig::default());
        assert!(!feats.is_empty());
        assert!(n > 0);
        let f = &feats[0];
        assert!(f.iter().all(|v| v.is_finite()), "all features finite: {f:?}");
        assert_eq!(f[0], 1.0, "pen_down flag first");
        assert!(f[1] > 0.0, "dx positive");
        assert!(f[2].abs() < 0.05, "dy ~ 0, got {}", f[2]);
        assert!((f[4] - 1.0 / 3.0).abs() < 0.05, "d1 ~ 1/3, got {}", f[4]);
        assert!((f[6] - 1.0 / 3.0).abs() < 0.05, "d2 ~ 1/3, got {}", f[6]);
        assert!(f[3].abs() < 0.05 && f[5].abs() < 0.05, "angles ~ 0: {:?}", &f[3..6]);
    }

    #[test]
    fn empty_ink_no_features() {
        let (feats, _) = extract(&Ink::default(), None, &FeatureConfig::default());
        assert!(feats.is_empty());
    }

    #[test]
    fn gap_curve_has_pen_down_zero() {
        let ink = Ink {
            strokes: vec![
                InkStroke { points: vec![Point { x: 0.0, y: 0.0, t: 0.0 }, Point { x: 1.0, y: 0.0, t: 10.0 }] },
                InkStroke { points: vec![Point { x: 5.0, y: 5.0, t: 30.0 }, Point { x: 6.0, y: 5.0, t: 40.0 }] },
            ],
        };
        let (feats, _) = extract(&ink, None, &FeatureConfig::default());
        assert!(feats.iter().any(|f| f[0] == 0.0), "a pen-up bridge row is emitted");
        assert!(feats.iter().any(|f| f[0] == 1.0), "pen-down rows exist");
    }
}
