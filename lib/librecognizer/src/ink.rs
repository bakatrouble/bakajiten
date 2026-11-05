//! Ink model and JNI-contract timestamp normalization.
//!
//! Mirrors `DigitalInkRecognizerJni.zzc`: strokes are `float[point][x, y, dt_ms, 0]`.
//! Timestamps are milliseconds relative to the first point of the whole ink.
//! If any stroke lacks timestamps, **all** points get synthetic timestamps of
//! `20ms * global_point_index` (same fallback as the Java layer).

/// One pen sample. `t` is milliseconds since the first point of the ink.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub t: f32,
}

/// One pen-down stroke.
#[derive(Debug, Clone, Default)]
pub struct Stroke {
    pub points: Vec<Point>,
}

/// Whole handwritten input.
#[derive(Debug, Clone, Default)]
pub struct Ink {
    pub strokes: Vec<Stroke>,
}

impl Ink {
    /// Build from raw strokes of `(x, y, timestamp_ms_or_none)`.
    pub fn from_raw(strokes: Vec<Vec<(f32, f32, Option<i64>)>>) -> Self {
        let any_missing = strokes.iter().any(|s| s.iter().any(|p| p.2.is_none()));
        let mut ink = Ink::default();
        if any_missing {
            // JNI fallback: sequential 20ms per point across the whole ink.
            let mut k: f32 = 0.0;
            for s in strokes {
                let mut pts = Vec::with_capacity(s.len());
                for (x, y, _) in s {
                    pts.push(Point { x, y, t: k * 20.0 });
                    k += 1.0;
                }
                ink.strokes.push(Stroke { points: pts });
            }
            return ink;
        }
        let t0: i64 = strokes
            .iter()
            .flat_map(|s| s.iter())
            .map(|p| p.2.unwrap())
            .min()
            .unwrap_or(0);
        for s in strokes {
            ink.strokes.push(Stroke {
                points: s
                    .into_iter()
                    .map(|(x, y, t)| Point { x, y, t: (t.unwrap() - t0) as f32 })
                    .collect(),
            });
        }
        ink
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.iter().all(|s| s.points.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_timestamps() {
        let ink = Ink::from_raw(vec![vec![(0.0, 0.0, Some(100)), (1.0, 1.0, Some(150))]]);
        assert_eq!(ink.strokes[0].points[0].t, 0.0);
        assert_eq!(ink.strokes[0].points[1].t, 50.0);
    }

    #[test]
    fn synthetic_timestamps() {
        let ink = Ink::from_raw(vec![vec![(0.0, 0.0, None), (1.0, 1.0, None)]]);
        assert_eq!(ink.strokes[0].points[0].t, 0.0);
        assert_eq!(ink.strokes[0].points[1].t, 20.0);
    }
}
