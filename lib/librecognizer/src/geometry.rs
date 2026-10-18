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
