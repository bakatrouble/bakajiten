/// Pen-space divisor: pen coords (0..255) → matcher unit space.
pub const UNIT: f64 = 255.0;

/// Corner/hook primitive: a new section starts when consecutive segment
/// angles differ by more than this (outline §3: `PI/2 * 0.8` = 72°).
pub const SECTION_SPLIT_DEG: f64 = 72.0;

pub type UPoint = (f64, f64);
/// Axis-aligned bbox as `(x0, y0, x1, y1)`, y-down like the source data.
pub type UBBox = (f64, f64, f64, f64);

/// Normalize an angle to `[0, 2π)` (outline §3: `Radian`).
pub fn rad_norm(mut a: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    a %= two_pi;
    if a < 0.0 {
        a += two_pi;
    }
    a
}

/// Shortest signed arc from `a` to `b`, in `[-π, π]` (outline §3:
/// `radDiff`). All DTW costs derive from this, so segment-angle
/// convention (y-down `atan2`) cancels out — only consistency matters.
pub fn rad_diff(a: f64, b: f64) -> f64 {
    let mut d = rad_norm(b) - rad_norm(a);
    let pi = std::f64::consts::PI;
    if d > pi {
        d -= 2.0 * pi;
    } else if d < -pi {
        d += 2.0 * pi;
    }
    d
}

/// Perpendicular distance from `q` to the line through `p1→p2`
/// (outline §3: `RecMath::pointDist` =
/// `sin(angle(p1→p2) − angle(p1→q)) * |p1−q|`). Signed; callers take
/// the absolute value.
pub fn point_dist(p1: UPoint, p2: UPoint, q: UPoint) -> f64 {
    let base = (p2.1 - p1.1).atan2(p2.0 - p1.0);
    let to_q = (q.1 - p1.1).atan2(q.0 - p1.0);
    let dist = ((q.0 - p1.0).powi(2) + (q.1 - p1.1).powi(2)).sqrt();
    (base - to_q).sin() * dist
}

/// Turning angle at `b` along `a→b→c`, in degrees, `0` = straight.
/// Zero-length legs yield `0` (no information, no turn).
pub fn turn_deg(a: UPoint, b: UPoint, c: UPoint) -> f64 {
    let (ax, ay) = (b.0 - a.0, b.1 - a.1);
    let (bx, by) = (c.0 - b.0, c.1 - b.1);
    let la = (ax * ax + ay * ay).sqrt();
    let lb = (bx * bx + by * by).sqrt();
    if la < 1e-12 || lb < 1e-12 {
        return 0.0;
    }
    ((ax * bx + ay * by) / (la * lb))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

/// Arc length of a polyline.
pub fn poly_len(pts: &[UPoint]) -> f64 {
    pts.windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum()
}

/// Bbox of points. Empty input → zero box at the origin.
pub fn bbox_of(pts: &[UPoint]) -> UBBox {
    let mut bb = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for &(x, y) in pts {
        bb.0 = bb.0.min(x);
        bb.1 = bb.1.min(y);
        bb.2 = bb.2.max(x);
        bb.3 = bb.3.max(y);
    }
    if bb.0.is_infinite() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        bb
    }
}

/// Union of two bboxes.
pub fn bbox_union(a: UBBox, b: UBBox) -> UBBox {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

/// Resample a polyline to exactly `n` equidistant points by arc length
/// (outline §3: `splitPoints`). Degenerate input repeats its single
/// point. Local `f64` copy of the geometry routine: the matcher works
/// in `f64` unit space throughout, and converting back and forth to
/// `f32` would quantize the very angles being compared.
pub fn resample(pts: &[UPoint], n: usize) -> Vec<UPoint> {
    if pts.is_empty() || n == 0 {
        return Vec::new();
    }
    if pts.len() == 1 || poly_len(pts) < 1e-12 {
        return vec![pts[0]; n];
    }
    let total = poly_len(pts);
    let step = total / (n as f64 - 1.0).max(1.0);
    let mut cum = vec![0.0f64];
    for w in pts.windows(2) {
        let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        cum.push(cum.last().unwrap() + d);
    }
    let mut out = Vec::with_capacity(n);
    let mut seg = 0usize;
    for i in 0..n {
        let target = i as f64 * step;
        while seg + 1 < cum.len() - 1 && cum[seg + 1] < target {
            seg += 1;
        }
        let (l0, l1) = (cum[seg], cum[seg + 1]);
        let t = if l1 - l0 < 1e-12 {
            0.0
        } else {
            ((target - l0) / (l1 - l0)).clamp(0.0, 1.0)
        };
        let (a, b) = (pts[seg], pts[seg + 1]);
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
    out
}

/// Split a point stream into sections: a new section starts wherever
/// the turn from the previous segment exceeds [`SECTION_SPLIT_DEG`]
/// (outline §3: `|angle[i] − angle[i−1]| > PI/2 * 0.8`). Returns
/// per-point section ids plus per-section arc lengths. Exact
/// duplicates are dropped first (outline §3: `add()`), so zero-length
/// segments never produce phantom turns.
pub fn build_sections(pts: &[UPoint]) -> (Vec<usize>, Vec<f64>) {
    let mut clean: Vec<UPoint> = Vec::with_capacity(pts.len());
    for &p in pts {
        if clean.last().is_none_or(|&q| q != p) {
            clean.push(p);
        }
    }
    if clean.len() < 2 {
        return (vec![0; clean.len()], vec![poly_len(&clean)]);
    }
    let mut ids = vec![0usize; clean.len()];
    let mut cur = 0usize;
    let mut prev_ang = (clean[1].1 - clean[0].1).atan2(clean[1].0 - clean[0].0);
    for i in 2..clean.len() {
        let ang = (clean[i].1 - clean[i - 1].1).atan2(clean[i].0 - clean[i - 1].0);
        if rad_diff(prev_ang, ang).abs().to_degrees() > SECTION_SPLIT_DEG {
            cur += 1;
        }
        ids[i] = cur;
        prev_ang = ang;
    }
    let nsec = cur + 1;
    let mut lens = vec![0.0f64; nsec];
    for w in clean.windows(2).enumerate() {
        let d = ((w.1[1].0 - w.1[0].0).powi(2) + (w.1[1].1 - w.1[0].1).powi(2)).sqrt();
        lens[ids[w.0 + 1]] += d;
    }
    (ids, lens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upoint_vec(pts: &[(f64, f64)]) -> Vec<UPoint> {
        pts.to_vec()
    }

    #[test]
    fn radian_helpers_wrap_correctly() {
        let pi = std::f64::consts::PI;
        assert!((rad_norm(3.0 * pi) - pi).abs() < 1e-12);
        assert!((rad_norm(-pi / 2.0) - 3.0 * pi / 2.0).abs() < 1e-12);
        // Shortest arc: 350° → 10° is +20°, not −340°.
        let a = 350.0f64.to_radians();
        let b = 10.0f64.to_radians();
        assert!((rad_diff(a, b) - 20.0f64.to_radians()).abs() < 1e-12);
        assert!((rad_diff(b, a) + 20.0f64.to_radians()).abs() < 1e-12);
    }

    #[test]
    fn sections_split_past_72_degrees() {
        // L: east then south = 90° turn → two sections.
        let pts = upoint_vec(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]);
        let (ids, lens) = build_sections(&pts);
        assert_eq!(ids, vec![0, 0, 1]);
        assert_eq!(lens.len(), 2);
        // Straight run: one section.
        let pts = upoint_vec(&[(0.0, 0.0), (0.5, 0.0), (1.0, 0.0)]);
        let (ids, _) = build_sections(&pts);
        assert!(ids.iter().all(|&i| i == 0));
    }
}
