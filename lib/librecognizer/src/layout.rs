use crate::geom::UBBox;

pub const POS_D_DIV: f64 = 6.0;
/// Strictly-outside gap: `max(d*0.63, orig_size*0.10)`.
pub const POS_GAP_K: f64 = 0.63;
pub const POS_GAP_MIN: f64 = 0.10;
/// Bbox inflation (`lovalue`): tighter for small drawings…
pub const POS_PAD_SMALL: f64 = 0.015;
/// …looser otherwise…
pub const POS_PAD_BIG: f64 = 0.03;
/// …plus extra slack for degenerate (dot-like) strokes. [RECON: the
/// outline states the slack without a value; 0.02 keeps dots matchable
/// without swallowing neighbors.]
pub const POS_PAD_DOT: f64 = 0.02;
/// Stroke bboxes below this (unit space) count as degenerate.
pub const POS_DOT_SIZE: f64 = 0.025;
/// Query drawings smaller than this per side are grown to it for zone
/// scale (outline §6.1: `max(dim, 0.65)` rect), so small drawings stay
/// stable instead of producing hair-trigger zones.
pub const POS_FULL_MIN: f64 = 0.65;

pub const POS_VERT: [u8; 8] = [0, 4, 8, 12, 15, 11, 7, 3];
/// Horizontal band order: far-left → overlap-left → overlap-right →
/// far-right (outline §6.2).
pub const POS_HORZ: [u8; 8] = [1, 5, 9, 13, 14, 10, 6, 2];

/// 16-bit relative-position descriptor of `comp` as seen from `orig`
/// (outline §6.1: `computeStrokePositions`). `full` is the whole
/// drawing's bbox (zone scale derives from it).
///
/// Band layout (bit numbers), vertical column then horizontal row:
///
/// ```text
///                 0          strictly above (gap)
///                 4          near band above
///                 8          touching top edge
///                12          overlap, upper interior
///  1  5  9  13        14 10  6  2
///                15         overlap, lower interior
///                11         touching bottom edge
///                 7         near band below
///                 3         strictly below (gap)
/// ```
///
/// (Left/right mirror this: 1 far-left, 5 near-left, 9 touching left
/// edge, 13 overlap left interior; 14 overlap right interior, 10
/// touching right edge, 6 near right, 2 far right.)
///
/// [RECON: the outline names the bands and the strict gap
/// `max(d*0.63, size*0.10)` but not the near/touch/overlap cutoffs.
/// This implements the only consistent reading: strict bands outside
/// the gap, near bands inside it, touch bands where the boxes meet or
/// cross an edge, overlap bands where an endpoint rests in an interior
/// half. `size` is read as the origin stroke's size.]
pub fn compute_positions(
    orig: UBBox,
    comp: UBBox,
    full: UBBox,
    orig_dot: bool,
    comp_dot: bool,
) -> u16 {
    let fw = (full.2 - full.0).max(1e-12);
    let fh = (full.3 - full.1).max(1e-12);
    let d = fw.max(fh) / POS_D_DIV;
    let osize = (orig.2 - orig.0).max(orig.3 - orig.1).max(1e-12);
    let gap = (d * POS_GAP_K).max(osize * POS_GAP_MIN);
    // Inflate both boxes: tighter for small drawings, plus slack for
    // degenerate strokes so dots stay matchable.
    let small = fw.max(fh) < 0.5;
    let mut pad = if small { POS_PAD_SMALL } else { POS_PAD_BIG };
    if orig_dot || comp_dot {
        pad += POS_PAD_DOT;
    }
    let o = (orig.0 - pad, orig.1 - pad, orig.2 + pad, orig.3 + pad);
    let c = (comp.0 - pad, comp.1 - pad, comp.2 + pad, comp.3 + pad);
    let mid_y = (o.1 + o.3) / 2.0;
    let mid_x = (o.0 + o.2) / 2.0;
    let mut bits: u16 = 0;
    let set = |bits: &mut u16, b: u8| *bits |= 1 << b;
    // Vertical: comp's bottom vs orig's top, then comp's top vs orig's
    // bottom, then interior endpoints.
    if c.3 < o.1 - gap {
        set(&mut bits, 0); // strictly above
    } else if c.3 < o.1 {
        set(&mut bits, 4); // near band above
    }
    if c.0 < o.1 && c.3 >= o.1 {
        set(&mut bits, 8); // straddles the top edge: touching
    }
    if c.1 >= o.1 && c.1 < mid_y {
        set(&mut bits, 12); // starts in the upper interior
    }
    if c.3 > mid_y && c.3 <= o.3 {
        set(&mut bits, 15); // ends in the lower interior
    }
    if c.0 < o.3 && c.3 > o.3 {
        set(&mut bits, 11); // straddles the bottom edge: touching
    }
    if c.1 > o.3 && c.1 <= o.3 + gap {
        set(&mut bits, 7); // near band below
    } else if c.1 > o.3 + gap {
        set(&mut bits, 3); // strictly below
    }
    // Horizontal: mirror image.
    if c.2 < o.0 - gap {
        set(&mut bits, 1);
    } else if c.2 < o.0 {
        set(&mut bits, 5);
    }
    if c.0 < o.0 && c.2 >= o.0 {
        set(&mut bits, 9);
    }
    if c.0 >= o.0 && c.0 < mid_x {
        set(&mut bits, 13);
    }
    if c.2 > mid_x && c.2 <= o.2 {
        set(&mut bits, 14);
    }
    if c.0 < o.2 && c.2 > o.2 {
        set(&mut bits, 10);
    }
    if c.0 > o.2 && c.0 <= o.2 + gap {
        set(&mut bits, 6);
    } else if c.0 > o.2 + gap {
        set(&mut bits, 2);
    }
    bits
}

/// Band-index distance between two descriptors (outline §6.2:
/// `posDiff`). Per axis, in band order, take the first and last set
/// bit of each descriptor and accumulate the squared index gaps.
/// Maximum per pair is 196 (2 endpoints × 2 axes × 7²). An axis with no
/// set bits on either side contributes 0. [RECON: the outline doesn't
/// state the empty-axis case; 0 keeps identical-but-tiny strokes at
/// distance 0 instead of punishing them maximally.]
pub fn pos_diff(p1: u16, p2: u16) -> u64 {
    let mut total = 0u64;
    for order in [POS_VERT, POS_HORZ] {
        let mut a = Vec::new();
        let mut b = Vec::new();
        for (ix, bit) in order.iter().enumerate() {
            if p1 & (1 << bit) != 0 {
                a.push(ix);
            }
            if p2 & (1 << bit) != 0 {
                b.push(ix);
            }
        }
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let d = (a[0] as i64 - b[0] as i64).pow(2)
            + (a[a.len() - 1] as i64 - b[b.len() - 1] as i64).pow(2);
        total += d.max(0) as u64;
    }
    total
}
