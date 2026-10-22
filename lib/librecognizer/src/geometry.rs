//! 2D stroke geometry: points and polylines in pen space (0..255).
//!
//! The matcher converts these to unit space on the way in
//! (`trajectories_from_strokes`); everything else here is plain
//! container code.

use serde::{Deserialize, Serialize};

/// A 2D point in pen space (0..255).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// One pen stroke: an ordered polyline.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct Stroke(pub Vec<Point>);

impl Stroke {
    pub fn new(points: Vec<Point>) -> Self {
        Self(points)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Fractional reveal of a polyline by arc length (desktop stroke
/// player): constant drawing speed regardless of point density,
/// with an interpolated tip instead of whole-point pops. Mirrors
/// the Android/web players step for step (`target > seg` consume,
/// else interpolate and stop; zero-length segments pass through).
pub fn stroke_slice(points: &[Point], fraction: f32) -> Vec<Point> {
    let Some(&first) = points.first() else {
        return Vec::new();
    };
    if fraction <= 0.0 {
        return Vec::new();
    }
    let mut segs = Vec::with_capacity(points.len().saturating_sub(1));
    let mut total = 0.0f64;
    for w in points.windows(2) {
        let len = f64::hypot((w[1].x - w[0].x) as f64, (w[1].y - w[0].y) as f64);
        segs.push(len);
        total += len;
    }
    if total <= 0.0 {
        return vec![first];
    }
    let mut target = f64::min(fraction as f64, 1.0) * total;
    let mut out = vec![first];
    for (i, w) in points.windows(2).enumerate() {
        let seg = segs[i];
        if target > seg {
            target -= seg;
            out.push(w[1]);
        } else {
            let t = if seg > 0.0 { target / seg } else { 0.0 };
            out.push(Point::new(
                (w[0].x as f64 + t * (w[1].x as f64 - w[0].x as f64)) as f32,
                (w[0].y as f64 + t * (w[1].y as f64 - w[0].y as f64)) as f32,
            ));
            break;
        }
    }
    out
}

#[cfg(test)]
mod playback_tests {
    use super::*;

    fn pts(pairs: &[(f32, f32)]) -> Vec<Point> {
        pairs.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn empty_and_nonpositive_yield_nothing() {
        assert_eq!(stroke_slice(&[], 0.5), vec![]);
        assert_eq!(stroke_slice(&pts(&[(3.0, 4.0)]), 0.0), vec![]);
        assert_eq!(stroke_slice(&pts(&[(3.0, 4.0)]), -1.0), vec![]);
    }

    #[test]
    fn degenerate_stroke_returns_first_point() {
        let one = pts(&[(3.0, 4.0)]);
        assert_eq!(stroke_slice(&one, 0.5), one);
        let dup = pts(&[(3.0, 4.0), (3.0, 4.0)]);
        assert_eq!(stroke_slice(&dup, 0.5), vec![Point::new(3.0, 4.0)]);
    }

    #[test]
    fn horizontal_reveal_is_linear() {
        let line = pts(&[(0.0, 0.0), (10.0, 0.0)]);
        assert_eq!(stroke_slice(&line, 0.25), pts(&[(0.0, 0.0), (2.5, 0.0)]));
        assert_eq!(stroke_slice(&line, 1.0), line);
        // Over-full fractions clamp to the whole stroke.
        assert_eq!(stroke_slice(&line, 2.0), line);
    }

    #[test]
    fn reveal_follows_arc_length_not_point_count() {
        // Dense first half, sparse second half: halfway by arc
        // length lands past the dense cluster, not at its end.
        let pts = pts(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (12.0, 0.0)]);
        let out = stroke_slice(&pts, 0.5);
        assert_eq!(out.len(), 4);
        assert!((out[3].x - 6.0).abs() < 1e-6);
        assert_eq!(out[3].y, 0.0);
    }

    #[test]
    fn zero_length_segments_pass_through() {
        let line = pts(&[(0.0, 0.0), (0.0, 0.0), (10.0, 0.0)]);
        let out = stroke_slice(&line, 0.5);
        assert_eq!(out.len(), 3);
        assert!((out[2].x - 5.0).abs() < 1e-6);
    }
}
