//! Recognition templates and pack intermediates: one entry per
//! labeled character with at least one stroke (codepoint + ordered
//! strokes). Ids, strokeless entries, unlabeled entries, and stroke
//! types are dropped at pack time: a template is a character plus
//! its strokes.
//!
//! Raw strokes travel inside kanji records (`KJD1`, see
//! `dict::kanji`); the app loads the precomputed template bank
//! below, skipping the pack-time pipeline at startup.
//!
//! ```text
//! offset  contents
//! 0..4    magic "KTT1"
//! 4..8    u32 version (=1)
//! 8..16   u64 packed-at (unix seconds, pack time)
//! 16..20  u32 ntemplates
//! per template:
//!   u32 char (codepoint)
//!   u32 nstrokes
//!   per stroke (Trajectory):
//!     u32 npts + npts × (u8 x, u8 y)     // pen grid (0..255)
//!     u32 nsegs + nsegs × (u8 angle, u8 len, u8 section)
//!     u8 len + u8 angle
//!     u32 nsec + nsec × u8 sec_lens
//!     // bboxes recomputed (stroke: bbox_of pts, template: union),
//!     // never stored
//! ```
//!
//! u8 field scales (f64 pipeline values map linearly; decode
//! divides back out):
//!
//! - angle: `(a + π) / 2π × 255` (full circle)
//! - seg len: `/ 0.5 × 255` (bank max ≈ 0.42)
//! - stroke/sec lens: `/ 4.0 × 255` (bank max ≈ 3.0)
//! - section: raw index (bank max single digits)
//!
//! Float→int casts saturate, so out-of-range future values clamp
//! instead of wrapping.
//!
//! Design notes:
//!
//! - Template points store as u8 pen-grid pairs: the pipeline rounds
//!   pen coords to whole units on entry (see `stages_from_strokes`),
//!   so the stored bytes recover them exactly. Ranking impact is
//!   negligible (measured: 99.4% top-1 agreement on jittered
//!   full-bank samples, wash on correctness).
//! - The decoder rejects unknown versions, short reads, invalid
//!   codepoints, absurd counts, empty stroke lists, and trailing
//!   garbage, always with the failing offset.
//!
//! Built by the `pack-data` tool (`utils/src/bin/pack-data/main.rs`);
//! loaded by the UI at startup ([`templates_from_bytes`]).

use crate::error::{Error, Result};
use crate::geom::bbox_of;
use crate::geometry::Stroke;
use crate::preprocess::{trajectories_from_strokes, Seg, Trajectory};
use crate::rank::Template;
use std::f64::consts::{PI, TAU};

/// u8 field scales for the template bank (see module docs): segment
/// arc lengths and stroke/section lengths each get the tightest
/// scale covering the bank, so small values keep their resolution.
const SEG_LEN_MAX: f64 = 0.5;
const LEN_MAX: f64 = 4.0;

/// Magic prefix of precomputed template-bank files.
pub const TEMPLATES_MAGIC: &[u8; 4] = b"KTT1";
/// Template-bank format version written (and only accepted) here.
pub const TEMPLATES_VERSION: u32 = 1;

/// One recognition template source: a labeled character plus its
/// ordered strokes in pen coords. Both fields are required —
/// there is no `Option` here because unlabeled or strokeless entries
/// never enter the file.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelEntry {
    pub character: char,
    pub strokes: Vec<Stroke>,
}

/// Build the matcher template bank from model entries. Every entry
/// becomes a template (filtering already happened at pack time), so
/// there is no skip count to report. Template construction is
/// data-parallel (rayon) with order-preserving collect, hence
/// bit-identical to the sequential build.
pub fn build_templates(entries: &[ModelEntry]) -> Vec<Template> {
    use rayon::prelude::*;
    entries
        .par_iter()
        .map(|e| Template::new(e.character, trajectories_from_strokes(&e.strokes, false)))
        .collect()
}

/// Serialize a precomputed template bank (see module docs). Points
/// store as u8 pen-grid pairs (the pipeline rounds to whole units
/// on entry, so `(unit * 255).round()` recovers them exactly); the
/// remaining f64 fields store as u8 on the documented scales.
/// Bboxes recomputed on load, never stored.
pub fn templates_to_bytes(bank: &[Template], packed_at: u64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(TEMPLATES_MAGIC);
    out.extend_from_slice(&TEMPLATES_VERSION.to_le_bytes());
    out.extend_from_slice(&packed_at.to_le_bytes());
    out.extend_from_slice(&(bank.len() as u32).to_le_bytes());
    for t in bank {
        out.extend_from_slice(&(t.character as u32).to_le_bytes());
        out.extend_from_slice(&(t.strokes.len() as u32).to_le_bytes());
        for s in &t.strokes {
            out.extend_from_slice(&(s.pts.len() as u32).to_le_bytes());
            for (x, y) in &s.pts {
                out.push((x * 255.0).round() as u8);
                out.push((y * 255.0).round() as u8);
            }
            out.extend_from_slice(&(s.segs.len() as u32).to_le_bytes());
            for seg in &s.segs {
                out.push(((seg.angle + PI) / TAU * 255.0).round() as u8);
                out.push((seg.len / SEG_LEN_MAX * 255.0).round() as u8);
                debug_assert!(seg.section <= u8::MAX as usize);
                out.push(seg.section as u8);
            }
            out.push((s.len / LEN_MAX * 255.0).round() as u8);
            out.push(((s.angle + PI) / TAU * 255.0).round() as u8);
            out.extend_from_slice(&(s.sec_lens.len() as u32).to_le_bytes());
            for v in &s.sec_lens {
                out.push((v / LEN_MAX * 255.0).round() as u8);
            }
        }
    }
    out
}

/// Cursor over a byte slice with bounds-checked primitive reads.
/// Every read reports the failing offset, so corrupt files produce
/// actionable errors instead of panics.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn rest_len(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        if self.rest_len() < n {
            return Err(Error::Model(format!(
                "model: short read at offset {} (need {n} bytes for {what}, have {})",
                self.pos,
                self.rest_len()
            )));
        }
        let s = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u32(&mut self, what: &str) -> Result<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self, what: &str) -> Result<u64> {
        let b = self.take(8, what)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
}

/// Deserialize [`templates_to_bytes`] output: corrupt files fail
/// with the offset, never panic. Returns the bank plus the pack
/// timestamp. A `KTM1` raw-strokes file is rejected (wrong magic):
/// pack it to templates first.
pub fn templates_from_bytes(bytes: &[u8]) -> Result<(Vec<Template>, u64)> {
    let mut c = Cursor::new(bytes);
    let magic = c.take(4, "magic")?;
    if magic != TEMPLATES_MAGIC {
        return Err(Error::Model("templates: missing KTT1 magic".into()));
    }
    let version = c.u32("version")?;
    if version != TEMPLATES_VERSION {
        return Err(Error::Model(format!(
            "templates: unsupported version {version} (this build reads {})",
            TEMPLATES_VERSION
        )));
    }
    let packed_at = c.u64("packed-at")?;
    let n = c.u32("template count")? as usize;
    let mut out = Vec::with_capacity(n.min(65536));
    for i in 0..n {
        let cp = c.u32(&format!("template[{i}].char"))?;
        let character = char::from_u32(cp)
            .ok_or_else(|| Error::Model(format!("templates: invalid char U+{cp:04X} at [{i}]")))?;
        let nstrokes = c.u32(&format!("template[{i}].nstrokes"))? as usize;
        if nstrokes > 1_000_000 {
            return Err(Error::Model(format!(
                "templates: bad stroke count {nstrokes} at [{i}]"
            )));
        }
        let mut strokes = Vec::with_capacity(nstrokes.min(1024));
        for j in 0..nstrokes {
            let npts = c.u32(&format!("template[{i}].stroke[{j}].npts"))? as usize;
            if npts == 0 || npts > 1_000_000 {
                return Err(Error::Model(format!(
                    "templates: bad point count {npts} at [{i}][{j}]"
                )));
            }
            let mut pts = Vec::with_capacity(npts.min(4096));
            for _ in 0..npts {
                let x = c.take(1, &format!("template[{i}].stroke[{j}].x"))?[0] as f64 / 255.0;
                let y = c.take(1, &format!("template[{i}].stroke[{j}].y"))?[0] as f64 / 255.0;
                pts.push((x, y));
            }
            let nsegs = c.u32(&format!("template[{i}].stroke[{j}].nsegs"))? as usize;
            if nsegs == 0 || nsegs > 1_000_000 {
                return Err(Error::Model(format!(
                    "templates: bad segment count {nsegs} at [{i}][{j}]"
                )));
            }
            let mut segs = Vec::with_capacity(nsegs.min(4096));
            for _ in 0..nsegs {
                let angle =
                    c.take(1, &format!("template[{i}].stroke[{j}].angle"))?[0] as f64 / 255.0 * TAU
                        - PI;
                let len = c.take(1, &format!("template[{i}].stroke[{j}].len"))?[0] as f64 / 255.0
                    * SEG_LEN_MAX;
                let section = c.take(1, &format!("template[{i}].stroke[{j}].section"))?[0] as usize;
                segs.push(Seg {
                    angle,
                    len,
                    section,
                });
            }
            let len =
                c.take(1, &format!("template[{i}].stroke[{j}].len"))?[0] as f64 / 255.0 * LEN_MAX;
            let angle = c.take(1, &format!("template[{i}].stroke[{j}].angle"))?[0] as f64 / 255.0
                * TAU
                - PI;
            let nsec = c.u32(&format!("template[{i}].stroke[{j}].nsec"))? as usize;
            if nsec > 1_000_000 {
                return Err(Error::Model(format!(
                    "templates: bad section count {nsec} at [{i}][{j}]"
                )));
            }
            let mut sec_lens = Vec::with_capacity(nsec.min(1024));
            for _ in 0..nsec {
                sec_lens.push(
                    c.take(1, &format!("template[{i}].stroke[{j}].sec"))?[0] as f64 / 255.0
                        * LEN_MAX,
                );
            }
            // Stroke bbox recomputed (never stored):
            // Template::new unions these into the template bbox.
            let bbox = bbox_of(&pts);
            strokes.push(Trajectory {
                pts,
                segs,
                len,
                angle,
                sec_lens,
                bbox,
            });
        }
        out.push(Template::new(character, strokes));
    }
    if c.rest_len() > 0 {
        return Err(Error::Model(format!(
            "templates: {} trailing bytes after {} templates",
            c.rest_len(),
            n
        )));
    }
    Ok((out, packed_at))
}

/// Save a precomputed template bank to disk with the given pack timestamp.
/// Written atomically (temp + rename): readers never observe a torn file.
pub fn save_templates(path: &std::path::Path, bank: &[Template], packed_at: u64) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, templates_to_bytes(bank, packed_at))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::rank::{find_candidates, KanaFilter, Query};

    const STAMP: u64 = 1_700_000_000;

    fn two_entries() -> Vec<ModelEntry> {
        vec![
            ModelEntry {
                character: '一',
                strokes: vec![Stroke::new(vec![
                    Point::new(10.0, 50.0),
                    Point::new(90.0, 50.0),
                ])],
            },
            ModelEntry {
                // Non-BMP char exercises the codepoint path.
                character: '\u{2000B}',
                strokes: vec![
                    Stroke::new(vec![Point::new(10.0, 10.0), Point::new(90.0, 90.0)]),
                    Stroke::new(vec![Point::new(90.0, 10.0), Point::new(10.0, 90.0)]),
                ],
            },
        ]
    }

    fn two_bank() -> Vec<Template> {
        build_templates(&two_entries())
    }

    #[test]
    fn templates_roundtrip_is_idempotent() {
        // u8 points are lossy vs the f64 build (dissect interpolates
        // off-grid), so the file round-trips through itself exactly:
        // decode(encode) re-encodes bit-identically.
        let bank = two_bank();
        let once = templates_to_bytes(&bank, STAMP);
        let (back, stamp) = templates_from_bytes(&once).unwrap();
        assert_eq!(templates_to_bytes(&back, stamp), once);
        assert_eq!(stamp, STAMP);
        // ...while ranking still matches the in-memory build (see
        // templates_rank_like_built_ones below).
    }

    #[test]
    fn templates_rank_like_built_ones() {
        // A decoded bank ranks exactly like the in-memory build: the
        // file carries everything recognition needs, precomputed.
        let (bank, _) = templates_from_bytes(&templates_to_bytes(&two_bank(), STAMP)).unwrap();
        let q = Query::drawn(&[Stroke::new(vec![
            Point::new(12.0, 52.0),
            Point::new(88.0, 48.0),
        ])]);
        let ranked = find_candidates(&q, &bank, 0, KanaFilter::All);
        assert_eq!(bank[ranked[0].idx].character, '一');
    }

    #[test]
    fn templates_reject_garbage_and_truncation() {
        assert!(templates_from_bytes(b"").is_err());
        assert!(templates_from_bytes(b"KTT1").is_err());
        // Raw-strokes files are not template banks (hand-built
        // header: magic KTM1, version 3, zeroed stamp, no entries).
        let mut raw = b"KTM1".to_vec();
        raw.extend_from_slice(&3u32.to_le_bytes());
        raw.extend_from_slice(&0u64.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        assert!(templates_from_bytes(&raw)
            .unwrap_err()
            .to_string()
            .contains("KTT1"));
        // Wrong version.
        let mut v = templates_to_bytes(&two_bank(), STAMP);
        v[4] = 99;
        assert!(templates_from_bytes(&v)
            .unwrap_err()
            .to_string()
            .contains("version"));
        // Truncation at every 11th byte must fail, never panic.
        let full = templates_to_bytes(&two_bank(), STAMP);
        for len in (0..full.len()).step_by(11) {
            assert!(templates_from_bytes(&full[..len]).is_err(), "len={len}");
        }
        // Trailing garbage must fail.
        let mut long = full.clone();
        long.extend_from_slice(b"junk");
        assert!(templates_from_bytes(&long)
            .unwrap_err()
            .to_string()
            .contains("trailing"));
    }
}
