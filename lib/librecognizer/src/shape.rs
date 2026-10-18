use crate::geom::rad_diff;
use crate::preprocess::{Seg, Trajectory};

/// [`compare`] constants (outline §5).
/// Per-cell angle cost scale: `|angle-diff| * 800`.
pub const DTW_COST_SCALE: f64 = 800.0;
/// Aligned segments on sections shorter than `0.18 * len` *and*
/// similarly sized (`min/max > 0.8`) cost half — small matching
/// wiggles matter less.
pub const DTW_SHORT_FRAC: f64 = 0.18;
pub const DTW_SHORT_SIM: f64 = 0.8;
pub const DTW_SHORT_HALF: f64 = 0.5;
/// A first/last section shorter than `min(0.2, len*0.18)` with a long
/// neighbor (`> len*0.18`) is a suspected brush hook (§5.1).
pub const HOOK_ABS: f64 = 0.2;
pub const HOOK_FRAC: f64 = 0.18;
/// Separately-aligned hook ends that agree better than this mean cost
/// are not hooks after all — the hypothesis is discarded.
pub const HOOK_AGREE: f64 = 333.333;
/// Final normalization: `res/steps * 10 + wextra * 1000` (§5.3).
pub const DTW_NORM: f64 = 10.0;
pub const DTW_WEXTRA: f64 = 1000.0;

fn hook_suspect(stroke: &Trajectory, at_start: bool) -> Option<f64> {
    if stroke.sec_lens.len() < 2 {
        return None;
    }
    let (sec, neighbor) = if at_start {
        (stroke.sec_lens[0], stroke.sec_lens[1])
    } else {
        (
            stroke.sec_lens[stroke.sec_lens.len() - 1],
            stroke.sec_lens[stroke.sec_lens.len() - 2],
        )
    };
    let short = HOOK_ABS.min(stroke.len * HOOK_FRAC);
    if sec < short && neighbor > stroke.len * HOOK_FRAC {
        Some(sec)
    } else {
        None
    }
}

/// Segment index range covering the first/last `arc_len` of arc length.
fn end_range(segs: &[Seg], arc_len: f64, at_start: bool) -> (usize, usize) {
    if at_start {
        let mut acc = 0.0;
        let mut end = 0;
        while end < segs.len() && acc < arc_len {
            acc += segs[end].len;
            end += 1;
        }
        (0, end.max(1).min(segs.len()))
    } else {
        let mut acc = 0.0;
        let mut start = segs.len();
        while start > 0 && acc < arc_len {
            start -= 1;
            acc += segs[start].len;
        }
        (start.min(segs.len().saturating_sub(1)), segs.len())
    }
}

/// Plain segment-angle DTW over two segment slices. Cell cost is
/// `|angle-diff| * 800`, halved when both segments sit on short
/// (`<0.18·len`) similarly-sized (`min/max > 0.8`) sections; each cell
/// adds the min of left/top/diagonal (outline §5.2). Returns
/// `(accumulated cost, backtrack steps)`.
fn dtw_matrix(a: &[Seg], a_len: f64, b: &[Seg], b_len: f64) -> (f64, usize) {
    let (mw, mh) = (a.len(), b.len());
    if mw == 0 || mh == 0 {
        return (f64::INFINITY, 1);
    }
    let cost = |x: &Seg, xs: f64, y: &Seg, ys: f64| {
        let mut c = rad_diff(x.angle, y.angle).abs() * DTW_COST_SCALE;
        // Small matching wiggles matter less.
        if xs < DTW_SHORT_FRAC * a_len
            && ys < DTW_SHORT_FRAC * b_len
            && xs.min(ys) / xs.max(ys).max(1e-12) > DTW_SHORT_SIM
        {
            c *= DTW_SHORT_HALF;
        }
        c
    };
    // Section arc lengths, indexed by section id carried on segments.
    let sec_len = |segs: &[Seg], total: f64| -> Vec<f64> {
        let nsec = segs.iter().map(|s| s.section).max().unwrap_or(0) + 1;
        let mut lens = vec![0.0f64; nsec];
        for s in segs {
            lens[s.section] += s.len;
        }
        // Dissected arc only approximates the simplified total; rescale
        // so short-section thresholds stay calibrated.
        let sum: f64 = lens.iter().sum::<f64>().max(1e-12);
        for l in lens.iter_mut() {
            *l *= total / sum;
        }
        lens
    };
    let a_secs = sec_len(a, a_len);
    let b_secs = sec_len(b, b_len);
    let at = |segs: &[Seg], secs: &[f64], i: usize| {
        let s = segs[i];
        (s, secs[s.section.min(secs.len() - 1)])
    };
    let mut dp = vec![vec![f64::INFINITY; mh + 1]; mw + 1];
    dp[0][0] = 0.0;
    for i in 1..=mw {
        for j in 1..=mh {
            let (x, xs) = at(a, &a_secs, i - 1);
            let (y, ys) = at(b, &b_secs, j - 1);
            let c = cost(&x, xs, &y, ys);
            dp[i][j] = c + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1]);
        }
    }
    // Backtrack to (0,0) counting steps (per-step marginals are
    // diagnostics only in the outline — collected, not added back).
    let (mut i, mut j) = (mw, mh);
    let mut steps = 0usize;
    while i > 0 || j > 0 {
        steps += 1;
        let (di, dj) = if i > 0 && j > 0 {
            let (m, d, l) = (dp[i - 1][j - 1], dp[i - 1][j], dp[i][j - 1]);
            if d <= m && d <= l {
                (1, 0)
            } else if l <= m && l <= d {
                (0, 1)
            } else {
                (1, 1)
            }
        } else if i > 0 {
            (1, 0)
        } else {
            (0, 1)
        };
        i -= di;
        j -= dj;
    }
    (dp[mw][mh], steps.max(1))
}

/// Shape distance between two comparison strokes (outline §5:
/// `Stroke::compare`). `tmpl` plays the outline's `other` (n-side),
/// `query` its `this` (m-side); the roles are symmetric except in
/// hook-end priority.
///
/// 1. Hook suppression (§5.1): the first suspect end (template-start,
///    template-end, query-start, query-end — [RECON: the outline calls
///    these combinations "mutually exclusive" without giving priority,
///    so first-suspect in this order wins]) is aligned separately; if
///    it agrees (mean < 333.333) the hypothesis is discarded, else the
///    region leaves the main matrix and its arc length feeds `wextra`.
/// 2. Main DTW (§5.2) over the surviving segments.
/// 3. Normalization (§5.3): `res/steps * 10 + wextra * 1000`. Callers
///    halve the result; only relative ordering matters.
pub fn compare(tmpl: &Trajectory, query: &Trajectory) -> f64 {
    // 1. Hook suppression.
    let mut n_lo = 0usize;
    let mut n_hi = tmpl.seg_count();
    let mut m_lo = 0usize;
    let mut m_hi = query.seg_count();
    let mut wextra = 0.0f64;
    // Candidate ends in outline order; first suspect wins.
    let suspects: [(bool, bool); 4] = [
        (true, true),   // template start
        (true, false),  // template end
        (false, true),  // query start
        (false, false), // query end
    ];
    for (is_tmpl, at_start) in suspects {
        let (stroke, other) = if is_tmpl {
            (tmpl, query)
        } else {
            (query, tmpl)
        };
        let Some(hook_len) = hook_suspect(stroke, at_start) else {
            continue;
        };
        // Suspect region on this side, same-arc end region on the
        // other. [RECON: the outline aligns "the suspect end segments"
        // against the other stroke without sizing the far side; equal
        // arc lengths is the natural reading. Note the far-side range
        // may cover a much shorter stroke entirely — the exclusion
        // below clamps ranges to non-empty, so this never erases a
        // side even then.]
        let (a0, a1) = end_range(&stroke.segs, hook_len, at_start);
        let (b0, b1) = end_range(&other.segs, hook_len, at_start);
        let a = &stroke.segs[a0..a1];
        let b = &other.segs[b0..b1];
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let (res, steps) = dtw_matrix(a, stroke.len, b, other.len);
        if res / steps as f64 > HOOK_AGREE {
            // Genuine hook: exclude both end regions from the main
            // matrix; their arc lengths penalize the mismatch later.
            let alen: f64 = a.iter().map(|s| s.len).sum();
            let blen: f64 = b.iter().map(|s| s.len).sum();
            wextra = (alen + blen) / 20.0;
            if is_tmpl {
                if at_start {
                    n_lo = a1;
                } else {
                    n_hi = a0;
                }
                if at_start {
                    m_lo = b1;
                } else {
                    m_hi = b0;
                }
            } else if at_start {
                m_lo = a1;
                n_lo = b1;
            } else {
                m_hi = a0;
                n_hi = b0;
            }
        }
        // Either way (agreed or excluded), the hypothesis is resolved:
        // mutually exclusive, one end only.
        break;
    }
    // Never erase a side: the far-side range is sized by the
    // suspect's arc length, which can exceed a much shorter stroke's
    // total arc (found live: a 0.121 hook wiped all 3 segments of a
    // 0.154 stroke → empty DTW → INFINITY → NaN downstream).
    // Clamping only ever fires on would-be-empty ranges; every other
    // pairing scores exactly as before.
    n_hi = n_hi.max(1).min(tmpl.seg_count());
    n_lo = n_lo.min(n_hi.saturating_sub(1));
    m_hi = m_hi.max(1).min(query.seg_count());
    m_lo = m_lo.min(m_hi.saturating_sub(1));
    // 2+3. Main matrix over survivors, then normalize.
    let n = &tmpl.segs[n_lo.min(n_hi)..n_hi];
    let m = &query.segs[m_lo.min(m_hi)..m_hi];
    let (res, steps) = dtw_matrix(n, tmpl.len, m, query.len);
    if !res.is_finite() {
        return f64::INFINITY;
    }
    res / steps as f64 * DTW_NORM + wextra * DTW_WEXTRA
}

#[cfg(test)]
mod compare_tests {
    use super::*;
    use crate::{geom::UPoint, preprocess::trajectory_from_points};

    fn line_stroke() -> Trajectory {
        let pts: Vec<UPoint> = (0..=20).map(|i| (i as f64 / 20.0, 0.0)).collect();
        trajectory_from_points(&pts, false).unwrap()
    }

    fn vline_stroke() -> Trajectory {
        let pts: Vec<UPoint> = (0..=20).map(|i| (0.0, i as f64 / 20.0)).collect();
        trajectory_from_points(&pts, false).unwrap()
    }

    #[test]
    fn identical_strokes_score_near_zero() {
        let a = line_stroke();
        let d = compare(&a, &a);
        assert!(d < 1e-6, "{d}");
    }

    #[test]
    fn orthogonal_strokes_score_far_apart() {
        let h = line_stroke();
        let v = vline_stroke();
        let same = compare(&h, &h);
        let diff = compare(&h, &v);
        assert!(diff > same + 100.0, "same={same} diff={diff}");
    }

    #[test]
    fn hook_exclusion_never_empties_a_side() {
        // Live failure (bank pair 73/103, both directions): a
        // 0.121-arc hook suspect sized the far-side range on a
        // 0.154-arc stroke, wiping all 3 of its segments → empty DTW →
        // INFINITY → NaN downstream. A short straight stroke
        // against a hooked longer one must stay finite both ways.
        //
        // Sharp corners survive simplification verbatim, so an exact L
        // pins the hook arc (0.12) deterministically above the short
        // stroke's survivable share (total 0.15 minus one ~0.05 seg).
        let short: Vec<UPoint> = vec![(0.0, 0.10), (0.15, 0.10)];
        let hooked: Vec<UPoint> = vec![(0.0, 0.0), (0.6, 0.0), (0.6, 0.12)];
        let a = trajectory_from_points(&short, false).unwrap();
        let b = trajectory_from_points(&hooked, false).unwrap();
        assert!(!a.segs.is_empty() && !b.segs.is_empty());
        let c1 = compare(&a, &b);
        let c2 = compare(&b, &a);
        assert!(c1.is_finite(), "{c1}");
        assert!(c2.is_finite(), "{c2}");
    }

    #[test]
    fn small_end_hook_costs_less_than_wrong_shape() {
        // Same stem with a tiny end hook vs a different shape: the
        // hook penalty must beat a wholesale mismatch.
        let mut hooked = Vec::new();
        for i in 0..=18 {
            hooked.push((i as f64 / 20.0, 0.0));
        }
        hooked.push((0.95, 0.05));
        hooked.push((0.9, 0.03));
        let hooked = trajectory_from_points(&hooked, true).unwrap();
        let h = line_stroke();
        let v = vline_stroke();
        let hook_cost = compare(&h, &hooked);
        let wrong_cost = compare(&h, &v);
        assert!(
            hook_cost < wrong_cost,
            "hook={hook_cost} wrong={wrong_cost}"
        );
        assert!(hook_cost.is_finite());
    }
}
