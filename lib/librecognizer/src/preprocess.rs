use crate::geom::{
    bbox_of, build_sections, point_dist, poly_len, rad_diff, resample, UBBox, UPoint, UNIT,
};
use crate::geometry::Stroke;

/// Denoise radii for [`simplify`] (outline §4.1): mouse input gets
/// stronger smoothing for hand shake. Queries are mouse-drawn (`true`);
/// KanjiVG templates are clean (`false`).
pub const TINY_MOUSE: f64 = 0.00725;
pub const TINY_MODEL: f64 = 0.00522;

/// [`dissect`] constants (outline §4.2).
pub const DISSECT_STEPS: usize = 3;
/// Equidistant resample points (`steps*60+1`).
pub const DISSECT_POINTS: usize = 181;
/// Corner candidate opens when the per-step direction change exceeds
/// `seg_len * STEEPNESS`.
pub const DISSECT_STEEPNESS: f64 = 1.46;
/// A candidate commits once accumulated change exceeds `len * LENMUL`.
pub const DISSECT_LENMUL: f64 = 0.037 / (DISSECT_POINTS as f64 / 201.0);
/// A candidate is abandoned (direction settled) while accumulated
/// change stays below `(ix-last) * seg_len * SEGMUL`.
pub const DISSECT_SEGMUL: f64 = 0.4;
/// Each corner interval is re-resampled to this many points.
pub const DISSECT_INTERVAL_POINTS: usize = DISSECT_STEPS + 1;
/// Defined in the outline (`0.8 * (chkcnt/divver)`) but with no stated
/// use in the scan procedure; kept so the constant set stays complete.
pub const DISSECT_SUMMUL: f64 = 0.8 * (DISSECT_POINTS as f64 / 201.0);

/// Denoise a unit-space point stream (outline §4.1:
/// `StrokeList::simplify`). Output is the *display* stroke; the
/// comparison stroke derives from it via [`dissect`].
///
/// - Step 1 joins very close points: while more than 2 points remain,
///   find the shortest segment *excluding the last*; stop once it
///   exceeds `tiny_dist * 3`. The merged point interpolates between
///   the pair with `balance = clamp(0.5 + (nextrad − thisrad)/(2π/3),
///   0, 1)` when the incoming kink is sharp (`< π/2·0.83`) or the
///   outgoing kink dominates — otherwise the plain midpoint.
/// - Step 2 drops near-collinear middle points to a fixpoint: erase
///   `i+1` while its perpendicular distance to `i→i+2` is below
///   `max_dist/2`, where longer neighboring segments tolerate larger
///   deviation.
/// - A degenerate stroke (≤1 point left) becomes a synthetic
///   0.25-diagonal segment so downstream code never divides by zero.
pub fn simplify(pts: &[UPoint], mouse_drawn: bool) -> Vec<UPoint> {
    let tiny = if mouse_drawn { TINY_MOUSE } else { TINY_MODEL };
    let mut out: Vec<UPoint> = pts.to_vec();
    // Step 1: join very close points.
    while out.len() > 2 {
        // Shortest segment, excluding the last.
        let mut m = 0usize;
        let mut best = f64::INFINITY;
        for i in 0..out.len() - 2 {
            let d = ((out[i + 1].0 - out[i].0).powi(2) + (out[i + 1].1 - out[i].1).powi(2)).sqrt();
            if d < best {
                best = d;
                m = i;
            }
        }
        if best > tiny * 3.0 {
            break;
        }
        // Kink at m (incoming) and at m+1 (outgoing), as signed turns.
        let kink = |a: UPoint, b: UPoint, c: UPoint| {
            rad_diff((b.1 - a.1).atan2(b.0 - a.0), (c.1 - b.1).atan2(c.0 - b.0))
        };
        let this_rad = if m == 0 {
            0.0
        } else {
            kink(out[m - 1], out[m], out[m + 1]).abs()
        };
        let next_rad = if m + 2 >= out.len() {
            0.0
        } else {
            kink(out[m], out[m + 1], out[m + 2]).abs()
        };
        // Sharp incoming kink, or the outgoing kink dominates: bias the
        // merge toward the corner; else take the midpoint.
        let balance = if this_rad < std::f64::consts::PI / 2.0 * 0.83 || next_rad > this_rad {
            (0.5 + (next_rad - this_rad) / (2.0 * std::f64::consts::PI / 3.0)).clamp(0.0, 1.0)
        } else {
            0.5
        };
        let merged = (
            out[m].0 + (out[m + 1].0 - out[m].0) * balance,
            out[m].1 + (out[m + 1].1 - out[m].1) * balance,
        );
        out[m] = merged;
        out.remove(m + 1);
        // Erase exact duplicates (keeps later length math honest).
        out.dedup();
    }
    // Step 2: drop near-collinear middle points, to a fixpoint.
    loop {
        let seglen: Vec<f64> = out
            .windows(2)
            .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
            .collect();
        let mut erased = false;
        let mut i = 0usize;
        while i + 2 < out.len() {
            let mut max_dist = tiny;
            if seglen[i] > tiny * 2.0 && seglen[i + 1] > tiny * 2.0 {
                max_dist = tiny * (0.5f64).max(seglen[i].min(seglen[i + 1]) / (tiny * 16.0));
            }
            if point_dist(out[i], out[i + 2], out[i + 1]).abs() < max_dist / 2.0 {
                out.remove(i + 1);
                erased = true;
            } else {
                i += 1;
            }
        }
        if !erased {
            break;
        }
    }
    // Degenerate input: synthetic diagonal so nothing divides by zero.
    if out.len() <= 1 {
        let base = out.into_iter().next().unwrap_or((0.0, 0.0));
        return vec![base, (base.0 + 0.25, base.1 + 0.25)];
    }
    out
}

/// Corner-preserving reduction to the comparison point list (outline
/// §4.2: `StrokeList::dissect`). The source is resampled to 181
/// equidistant points; a running-direction scan opens a corner
/// candidate on sharp per-step direction changes, commits it once
/// enough change accumulates, or abandons it once the direction
/// settles; every corner interval is re-resampled to 4 points. Result:
/// roughly `3·corners + 1` points — position- and speed-invariant,
/// angle-sequence only.
pub fn dissect(pts: &[UPoint]) -> Vec<UPoint> {
    if pts.len() < 2 {
        return resample(pts, DISSECT_INTERVAL_POINTS);
    }
    let pp = resample(pts, DISSECT_POINTS);
    let total = poly_len(&pp).max(1e-12);
    let seg = total / (DISSECT_POINTS as f64 - 1.0);
    // Consecutive difference vectors of the resample.
    let diff: Vec<UPoint> = pp
        .windows(2)
        .map(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1))
        .collect();
    // Running direction, seeded with the first step. Each step's
    // deviation from it accumulates; a sustained deviation proves a
    // corner, a transient one is wobble. [RECON: the outline names the
    // per-step quantity `di` between "consecutive difference vectors"
    // but the commit/abandon bookkeeping only closes if `dsum`
    // persists after the turn — i.e. deviation from the *running*
    // direction, which is re-seeded at every commit below.]
    let mut dir = diff[0];
    let mut corners = vec![0usize];
    let mut open: Option<usize> = None; // index the candidate opened at
    let mut dsum = 0.0f64;
    for (ix, dstep) in diff.iter().enumerate().skip(1) {
        // Deviation of the current step from the running direction.
        let di = (dstep.0 - dir.0).abs() + (dstep.1 - dir.1).abs();
        dsum += di;
        if open.is_none() && di > seg * DISSECT_STEEPNESS {
            open = Some(ix);
        }
        if let Some(last) = open {
            if dsum > total * DISSECT_LENMUL {
                // Committed: the corner is where the turn opened, and
                // the running direction becomes the new heading.
                if last != *corners.last().unwrap() {
                    corners.push(last);
                }
                dir = diff[ix];
                open = None;
                dsum = 0.0;
            } else if dsum < (ix - last) as f64 * seg * DISSECT_SEGMUL {
                // Direction settled back: not a corner after all.
                open = None;
                dsum = 0.0;
            }
        }
    }
    if *corners.last().unwrap() != DISSECT_POINTS - 1 {
        corners.push(DISSECT_POINTS - 1);
    }
    // Re-resample every corner interval to steps+1 points, joining
    // shared endpoints. [RECON: the outline says "re-resample every
    // corner interval"; shared endpoints are the only reading that
    // yields the stated ~3·corners+1 length.]
    let mut out = Vec::new();
    for w in corners.windows(2) {
        let part = resample(&pp[w[0]..=w[1]], DISSECT_INTERVAL_POINTS);
        if out.is_empty() {
            out.extend(part);
        } else {
            out.extend(part.into_iter().skip(1));
        }
    }
    out
}

/// One segment of a comparison stroke: direction, arc length, and the
/// corner-section it belongs to (for the short-section discount).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub angle: f64,
    pub len: f64,
    pub section: usize,
}

/// A stroke ready for comparison: dissected points plus per-segment
/// angles, lengths, and section tags, all in unit space (outline §3:
/// `Stroke { list, len, ang, sectcnt, dim }`).
#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    /// Dissected comparison points (`≈3·corners+1`).
    pub pts: Vec<UPoint>,
    /// Segments between them (the DTW sequence).
    pub segs: Vec<Seg>,
    /// Total arc length of the *simplified* stroke (hook thresholds
    /// are relative to this, matching the outline's `len`).
    pub len: f64,
    /// First-point → last-point angle.
    pub angle: f64,
    /// Arc length per corner-section (hook suppression).
    pub sec_lens: Vec<f64>,
    pub bbox: UBBox,
}

impl Trajectory {
    pub fn seg_count(&self) -> usize {
        self.segs.len()
    }
}

/// Build a comparison stroke from raw unit-space points: widen
/// singletons, denoise, section, dissect (outline §§3–4).
///
/// `mouse_drawn` selects the denoise radius (queries are hand-drawn;
/// templates are clean). Empty input yields `None` — unlike the
/// outline's editor path, our pipeline can simply skip empty strokes.
pub fn trajectory_from_points(raw: &[UPoint], mouse_drawn: bool) -> Option<Trajectory> {
    stages_from_points(raw, mouse_drawn).map(|(_, t)| t)
}

/// Denoised + dissected inspection stages for one raw unit-space
/// stroke: the `simplify` output (what dissection consumes) plus the
/// comparison stroke built from it. Drives the UI three-layer preview
/// (raw → denoised → dissected). `None` exactly when
/// [`trajectory_from_points`] returns `None`.
pub fn stages_from_points(raw: &[UPoint], mouse_drawn: bool) -> Option<(Vec<UPoint>, Trajectory)> {
    if raw.is_empty() {
        return None;
    }
    // Single-point taps are widened to a tiny 2-point stroke so
    // downstream code always has at least one segment (outline §1:
    // `+0.0005` in x/y).
    let widened: Vec<UPoint> = if raw.len() == 1 {
        vec![raw[0], (raw[0].0 + 0.0005, raw[0].1 + 0.0005)]
    } else {
        raw.to_vec()
    };
    let simple = simplify(&widened, mouse_drawn);
    let len = poly_len(&simple).max(1e-12);
    // Sections over the simplified polyline (corner primitive).
    let (point_secs, sec_lens) = build_sections(&simple);
    // Arc-length map: simplified cumulative length at each vertex…
    let mut cum = vec![0.0f64];
    for w in simple.windows(2) {
        let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        cum.push(cum.last().unwrap() + d);
    }
    let section_at = |pos: f64| -> usize {
        // …locates the section owning an arc position.
        let mut idx = 0usize;
        while idx + 1 < cum.len() - 1 && cum[idx + 1] < pos {
            idx += 1;
        }
        point_secs[idx.min(point_secs.len() - 1)]
    };
    let pts = dissect(&simple);
    if pts.len() < 2 {
        return None;
    }
    // Tag each dissected segment with its section via arc position.
    let mut segs = Vec::with_capacity(pts.len() - 1);
    let mut run = 0.0f64;
    for w in pts.windows(2) {
        let d = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        let mid = run + d / 2.0;
        // Dissected resampling preserves total length up to float
        // noise, so map its arc positions onto the simplified stroke.
        let pos = (mid / poly_len(&pts).max(1e-12) * len).min(len);
        segs.push(Seg {
            angle: (w[1].1 - w[0].1).atan2(w[1].0 - w[0].0),
            len: d,
            section: section_at(pos),
        });
        run += d;
    }
    let angle = (pts.last().unwrap().1 - pts[0].1).atan2(pts.last().unwrap().0 - pts[0].0);
    let bbox = bbox_of(&pts);
    Some((
        simple,
        Trajectory {
            pts,
            segs,
            len,
            angle,
            sec_lens,
            bbox,
        },
    ))
}

/// Convert pen-space strokes to unit-space comparison strokes.
/// Queries pass `mouse_drawn = true`, templates `false`.
pub fn trajectories_from_strokes(strokes: &[Stroke], mouse_drawn: bool) -> Vec<Trajectory> {
    stages_from_strokes(strokes, mouse_drawn)
        .into_iter()
        .map(|(_, t)| t)
        .collect()
}

/// Per-stroke inspection stages aligned with the input: `(denoised
/// unit-space polyline, comparison trajectory)` per non-empty stroke.
/// Same filtering as [`trajectories_from_strokes`], so the output
/// zips 1:1 with it. Drives the UI three-layer preview.
pub fn stages_from_strokes(
    strokes: &[Stroke],
    mouse_drawn: bool,
) -> Vec<(Vec<UPoint>, Trajectory)> {
    strokes
        .iter()
        .filter(|s| !s.0.is_empty())
        .filter_map(|s| {
            // u8 grid: pen coords round to whole units (0..255)
            // before the pipeline, so pack-time and query-time
            // trajectories agree exactly with the stored bytes.
            let raw: Vec<UPoint> =
                s.0.iter()
                    .map(|p| (p.x.round() as f64 / UNIT, p.y.round() as f64 / UNIT))
                    .collect();
            stages_from_points(&raw, mouse_drawn)
        })
        .collect()
}

#[cfg(test)]
mod preprocess_tests {
    use super::*;
    use crate::geom::UPoint;

    fn upoint_vec(pts: &[(f64, f64)]) -> Vec<UPoint> {
        pts.to_vec()
    }

    #[test]
    fn stages_agree_with_trajectory() {
        // The inspection stages expose the same trajectory
        // `trajectory_from_points` builds, plus the denoised polyline
        // it was dissected from.
        let raw: Vec<UPoint> = (0..=40)
            .map(|i| (i as f64 / 40.0, 0.001 * ((i * 7) % 5) as f64))
            .collect();
        let (simple, via_stages) = stages_from_points(&raw, true).unwrap();
        let via_direct = trajectory_from_points(&raw, true).unwrap();
        assert_eq!(via_stages.pts, via_direct.pts);
        assert_eq!(via_stages.segs.len(), via_direct.segs.len());
        // Denoising never invents points; dissection yields ≥1 segment.
        assert!(!simple.is_empty() && simple.len() <= raw.len());
        assert_eq!(via_stages.segs.len() + 1, via_stages.pts.len());
        assert!(stages_from_points(&[], true).is_none());
    }

    #[test]
    fn dissect_counts_follow_corners() {
        // Straight: one interval → steps+1 points.
        let line: Vec<UPoint> = (0..=50).map(|i| (i as f64 / 50.0, 0.0)).collect();
        assert_eq!(dissect(&line).len(), DISSECT_INTERVAL_POINTS);
        // L: two intervals sharing the corner → 2*4-1 points.
        let mut ell = Vec::new();
        for i in 0..=25 {
            ell.push((i as f64 / 50.0, 0.0));
        }
        for i in 1..=25 {
            ell.push((0.5, i as f64 / 50.0));
        }
        assert_eq!(dissect(&ell).len(), 2 * DISSECT_INTERVAL_POINTS - 1);
    }

    #[test]
    fn simplify_collapses_wobble_and_guards_degenerate() {
        // Near-straight noisy run collapses toward its endpoints.
        let mut pts = Vec::new();
        for i in 0..=40 {
            let t = i as f64 / 40.0;
            pts.push((t, 0.001 * ((i * 7) % 5) as f64));
        }
        let s = simplify(&pts, true);
        assert!(s.len() < pts.len());
        assert!((s[0].0 - 0.0).abs() < 1e-9 && (s.last().unwrap().0 - 1.0).abs() < 1e-9);
        // A sharp corner survives.
        let l = upoint_vec(&[(0.0, 0.0), (0.5, 0.0), (0.5, 0.5)]);
        let s = simplify(&l, true);
        assert_eq!(s.len(), 3);
        // Degenerate input becomes the synthetic diagonal, never empty.
        assert_eq!(simplify(&[], true).len(), 2);
        assert_eq!(simplify(&[(0.3, 0.4)], true).len(), 2);
    }
}
