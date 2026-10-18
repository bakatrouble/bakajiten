use crate::geom::{bbox_union, UBBox};
use crate::geometry::Stroke;
use crate::layout::{compute_positions, pos_diff, POS_DOT_SIZE, POS_FULL_MIN};
use crate::preprocess::{trajectories_from_strokes, Trajectory};
use crate::shape::compare;

pub const RANK_COUNT_PENALTY: f64 = 40000.0;
pub const RANK_ORDER_PENALTY: f64 = 300.0;
pub const RANK_COST_CAP: f64 = 100000.0;
pub const RANK_COMPDIST_FLOOR: f64 = 3000.0;
pub const RANK_POSW_ADJ: f64 = 0.09;
pub const RANK_POSW_GAP: f64 = 0.03;
pub const RANK_UNASSIGNED_W: f64 = 0.2;
/// Documented maximum of `pos_diff` (outline §6.2).
pub const RANK_POS_MAX: f64 = 196.0;
/// Tiny templates are below this unit-space size (§7.3: `w,h < 5000`
/// in `[0,50000]` units).
pub const RANK_TINY_TMPL: f64 = 0.1;
pub const RANK_SMALL_DRAW: f64 = 0.35;
pub const RANK_BIG_DRAW: f64 = 0.5;
pub const RANK_SIZE_DELTA: f64 = 0.05;
pub const RANK_MISS_BASE: f64 = 2500.0;
pub const RANK_MISS_STEP: f64 = 3333.0;
pub const RANK_MISS_CAP: f64 = 10000.0;
pub const RANK_MISS_MAXD: usize = 4;
/// Stroke-order tolerance window (±1): assignment scans
/// `k ∈ [iy-1, iy+1]` (outline §7: `swplimit = 1`).
pub const RANK_ORDER_WINDOW: isize = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub character: char,
    pub strokes: Vec<Trajectory>,
    pub full: UBBox,
}

impl Template {
    pub fn new(character: char, strokes: Vec<Trajectory>) -> Self {
        let mut full = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for s in &strokes {
            full = bbox_union(full, s.bbox);
        }
        if full.0.is_infinite() {
            full = (0.0, 0.0, 0.0, 0.0);
        }
        Self {
            character,
            strokes,
            full,
        }
    }
}

/// Query bundle: comparison strokes plus full bbox. Zone scale grows
/// small drawings to 0.65 per side (outline §6.1); the size prior uses
/// the actual bbox.
#[derive(Debug, Clone)]
pub struct Query {
    pub strokes: Vec<Trajectory>,
    pub full: UBBox,
}

impl Query {
    pub fn new(strokes: Vec<Trajectory>) -> Self {
        let mut full = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for s in &strokes {
            full = bbox_union(full, s.bbox);
        }
        if full.0.is_infinite() {
            full = (0.0, 0.0, 0.0, 0.0);
        }
        Self { strokes, full }
    }

    /// Hand-drawn query: raw pen strokes through the pipeline with
    /// mouse denoise (templates build the same way with `false`).
    pub fn drawn(strokes: &[Stroke]) -> Self {
        Self::new(trajectories_from_strokes(strokes, true))
    }

    /// Zone-scale bbox: actual box grown to [`POS_FULL_MIN`] per side.
    pub fn scaled_full(&self) -> UBBox {
        let (mut x0, mut y0, mut x1, mut y1) = self.full;
        if x1 - x0 < POS_FULL_MIN {
            let c = (x0 + x1) / 2.0;
            x0 = c - POS_FULL_MIN / 2.0;
            x1 = c + POS_FULL_MIN / 2.0;
        }
        if y1 - y0 < POS_FULL_MIN {
            let c = (y0 + y1) / 2.0;
            y0 = c - POS_FULL_MIN / 2.0;
            y1 = c + POS_FULL_MIN / 2.0;
        }
        (x0, y0, x1, y1)
    }
}

fn is_dot_box(bb: UBBox) -> bool {
    (bb.2 - bb.0).max(bb.3 - bb.1) < POS_DOT_SIZE
}

/// Per-candidate score components, in the order `rank_element`
/// applies them. The hand fused total is
/// `(shape + layout) * size + missing` (outline units).
#[derive(Debug, Clone, Copy)]
pub struct Fusion {
    /// Stage-1 assignment: extra-stroke penalty plus capped
    /// `compare()/2 + order` costs.
    pub shape: f64,
    /// Subtotal of the `|k−iy|·300` order terms (also inside `shape`,
    /// uncapped — exposed separately as a feature).
    pub order_pen: f64,
    /// Stage-2 layout: neighbor terms, unassigned-stroke terms, and
    /// the unswap delta.
    pub layout: f64,
    /// Stage-3 size factor (multiplicative, ≈0.95–1.05).
    pub size: f64,
    /// Stage-4 missing-stroke addition (0 when nothing is missing).
    pub missing: f64,
    /// Signed stroke-count difference, template − query.
    pub count_diff: f64,
    /// Final hand-fused distance (what [`rank_element`] returns,
    /// pre-rounding).
    pub total: f64,
}

/// One greedy assignment pair: query stroke `qy` wears template
/// stroke `tm`, contributing uncapped shape cost `cost`
/// (`compare()/2 + order`) of which `ord` is the `|k−iy|·300` order
/// term. The capped value actually added to `shape` is
/// `cost.min(RANK_COST_CAP)`.
#[derive(Debug, Clone, Copy)]
pub struct AssignPair {
    pub qy: usize,
    pub tm: usize,
    pub cost: f64,
    pub ord: f64,
}

/// Greedy order-tolerant shape assignment used by [`rank_fused`]:
/// each query stroke scans template strokes `k ∈ [iy−1, iy+1]`, taking the best *unused* one. Returns the
/// template→query map plus the winning pairs in query order.
/// Deterministic: same inputs, same pairs, same floating-point ops.
fn greedy_assign(
    qstrokes: &[Trajectory],
    tstrokes: &[Trajectory],
) -> (Vec<Option<usize>>, Vec<AssignPair>) {
    let mut used = vec![false; tstrokes.len()];
    let mut assign: Vec<Option<usize>> = vec![None; tstrokes.len()];
    let mut pairs = Vec::new();
    for (iy, q) in qstrokes.iter().enumerate() {
        let mut best: Option<(usize, f64, f64)> = None;
        let lo = iy.saturating_sub(RANK_ORDER_WINDOW as usize);
        let hi = (iy + RANK_ORDER_WINDOW as usize).min(tstrokes.len().saturating_sub(1));
        for (k, slot) in used
            .iter()
            .enumerate()
            .skip(lo)
            .take(hi.saturating_sub(lo) + 1)
        {
            if *slot {
                continue;
            }
            let ord = ((k as isize - iy as isize).abs() as f64) * RANK_ORDER_PENALTY;
            let cost = compare(&tstrokes[k], q) / 2.0 + ord;
            if best.is_none_or(|(_, c, _)| cost < c) {
                best = Some((k, cost, ord));
            }
        }
        if let Some((k, cost, ord)) = best {
            used[k] = true;
            assign[k] = Some(iy);
            pairs.push(AssignPair {
                qy: iy,
                tm: k,
                cost,
                ord,
            });
        }
    }
    (assign, pairs)
}

/// Per-stroke assignment detail for inspection (drives the UI detail
/// view): `(query_idx, tmpl_idx)` pairs in query order with the
/// uncapped shape cost each contributed. Unassigned query strokes are
/// absent — compare lengths against both stroke lists to spot them.
pub fn assignment_detail(query: &Query, tmpl: &Template) -> Vec<AssignPair> {
    greedy_assign(&query.strokes, &tmpl.strokes).1
}

/// Fuse shape + layout + priors for one query/template pair (outline
/// §7: `findCandidates`, minus the cross-candidate beam — every
/// candidate scores fully and the caller sorts).
///
/// 1. **Shape assignment** (greedy, order-tolerant): extra query
///    strokes cost `40000` each up front; each query stroke scans
///    template strokes `k ∈ [iy−1, iy+1]` for
///    `compare()/2 + |k−iy|·300`, taking the best *unused* one
///    (capped at `100000` per stroke).
/// 2. **Layout pass**: `compdist = max(3000, dist)`; each assigned
///    template stroke compares position descriptors against its next
///    2 / previous 2 assigned neighbors
///    (`pos_diff · compdist · posw / n`, `posw` 0.09 adjacent, 0.03
///    at gap 2); unassigned template strokes pay the worst case
///    (`compdist·0.2·196/n`). Adjacent template strokes whose query
///    order came out swapped are scored both ways, keeping the min.
/// 3. **Size prior (±5%)**: tiny templates (< 0.1) discount small
///    drawings (< 0.35) and tax large ones (> 0.5); large templates do
///    the inverse.
/// 4. **Missing-stroke penalty**: `d·2500 + min((d−1)·3333, 10000)`
///    for `d = min(4, template − query)` extra template strokes.
///
/// Returns an integer-ish fused distance (lower is better). Absolute
/// values are meaningless across inputs (beam note in the outline);
/// only ordering matters.
pub fn rank_element(query: &Query, tmpl: &Template) -> u64 {
    let f = rank_fused(query, tmpl);
    if f.total >= u64::MAX as f64 {
        return u64::MAX;
    }
    f.total.max(0.0).round() as u64
}

/// [`rank_element`] with all intermediate terms exposed (see [`Fusion`]).
/// The returned `total` rounds to exactly what `rank_element` returns.
pub fn rank_fused(query: &Query, tmpl: &Template) -> Fusion {
    let n = query.strokes.len();
    let m = tmpl.strokes.len();
    if m == 0 {
        return Fusion {
            shape: 0.0,
            order_pen: 0.0,
            layout: 0.0,
            size: 1.0,
            missing: 0.0,
            count_diff: 0.0,
            total: u64::MAX as f64,
        };
    }
    // 1. Shape assignment.
    let mut shape = (n.saturating_sub(m)) as f64 * RANK_COUNT_PENALTY;
    let mut order_pen = 0.0f64;
    // assign[k] = query index wearing template stroke k (both directions
    // stay consistent: each query stroke is spent at most once because
    // `used` blocks reuse and each query stroke assigns exactly once).
    let (mut assign, pairs) = greedy_assign(&query.strokes, &tmpl.strokes);
    for p in &pairs {
        order_pen += p.ord;
        shape += p.cost.min(RANK_COST_CAP);
    }
    // Position descriptors for the assigned pairs. Both sides see the
    // same construction: bits of the *other* stroke relative to self.
    let qfull = query.scaled_full();
    let qactual = query.full;
    let model_pos = |tk: usize, tk2: usize| {
        compute_positions(
            tmpl.strokes[tk].bbox,
            tmpl.strokes[tk2].bbox,
            tmpl.full,
            is_dot_box(tmpl.strokes[tk].bbox),
            is_dot_box(tmpl.strokes[tk2].bbox),
        )
    };
    let input_pos = |a: usize, b: usize| {
        compute_positions(
            query.strokes[a].bbox,
            query.strokes[b].bbox,
            qfull,
            is_dot_box(query.strokes[a].bbox),
            is_dot_box(query.strokes[b].bbox),
        )
    };
    // 2. Layout pass.
    let compdist = RANK_COMPDIST_FLOOR.max(shape);
    let div = (n.min(m).max(1)) as f64;
    // Neighbor-term helper under an arbitrary assignment (lets the
    // unswap check reuse it).
    let neighbor_terms = |assign: &[Option<usize>]| -> f64 {
        let mut sub = 0.0f64;
        for k in 0..m {
            let Some(a) = assign[k] else { continue };
            for dk in [1isize, 2, -1, -2] {
                let k2 = k as isize + dk;
                if k2 < 0 || k2 >= m as isize {
                    continue;
                }
                let Some(b) = assign[k2 as usize] else {
                    continue;
                };
                let posw = if dk.abs() == 1 {
                    RANK_POSW_ADJ
                } else {
                    RANK_POSW_GAP
                };
                sub +=
                    pos_diff(model_pos(k, k2 as usize), input_pos(a, b)) as f64 * compdist * posw
                        / div;
            }
        }
        sub
    };
    let mut layout = neighbor_terms(&assign);
    for slot in assign.iter().take(m) {
        if slot.is_none() {
            layout += compdist * RANK_UNASSIGNED_W * RANK_POS_MAX / div;
        }
    }
    // Unswap: adjacent template strokes worn in swapped query order
    // score both ways; keep the better layout. Swaps APPLY
    // cumulatively: each pair is evaluated against the current map and
    // the map updates on improvement, so the deltas telescope exactly
    // (layout ends as unassigned + neighbor_terms(final map)).
    // Comparing every pair against the original map without applying
    // would subtract the full neighbor sum once per swapped pair —
    // with 2+ swaps the layout goes hugely negative (e.g. -18841
    // against shape 18889), promoting terrible pairs to the top and
    // producing non-finite totals downstream.
    for k in 0..m.saturating_sub(1) {
        let (Some(a), Some(b)) = (assign[k], assign[k + 1]) else {
            continue;
        };
        if a > b {
            let mut swapped = assign.clone();
            swapped[k] = Some(b);
            swapped[k + 1] = Some(a);
            let alt = neighbor_terms(&swapped);
            let cur = neighbor_terms(&assign);
            if alt < cur {
                layout += alt - cur;
                assign = swapped;
            }
        }
    }
    // 3. Size prior.
    let tsize = (tmpl.full.2 - tmpl.full.0).max(tmpl.full.3 - tmpl.full.1);
    let qsize = (qactual.2 - qactual.0).max(qactual.3 - qactual.1);
    let tiny = tsize < RANK_TINY_TMPL;
    let size = if tiny {
        if qsize < RANK_SMALL_DRAW {
            1.0 - RANK_SIZE_DELTA
        } else if qsize > RANK_BIG_DRAW {
            1.0 + RANK_SIZE_DELTA
        } else {
            1.0
        }
    } else if qsize < RANK_SMALL_DRAW {
        1.0 + RANK_SIZE_DELTA
    } else if qsize > RANK_BIG_DRAW {
        1.0 - RANK_SIZE_DELTA
    } else {
        1.0
    };
    // 4. Missing strokes.
    let mut missing = 0.0f64;
    if m > n {
        let d = (m - n).min(RANK_MISS_MAXD) as f64;
        missing = d * RANK_MISS_BASE + ((d - 1.0) * RANK_MISS_STEP).min(RANK_MISS_CAP);
    }
    Fusion {
        shape,
        order_pen,
        layout,
        size,
        missing,
        count_diff: m as f64 - n as f64,
        total: (shape + layout) * size + missing,
    }
}

/// Hiragana + katakana blocks (including halfwidth): the model's
/// kana templates live here (see `model`), matched only in kana mode.
pub fn is_kana(c: char) -> bool {
    ('\u{3040}'..='\u{30ff}').contains(&c)
}

/// Which templates `find_candidates` scores: everything, or
/// kanji-only with the kana templates skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KanaFilter {
    /// Score every template (kanji + kana; previous behavior).
    All,
    /// Skip kana templates (kanji-only mode).
    SkipKana,
}

/// One ranked candidate: bank index, fused distance (lower is
/// better), and whether the template is kana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateHit {
    pub idx: usize,
    pub distance: u64,
    pub character: char,
    pub is_kana: bool,
}

/// Rank all templates against one query (outline §7, final sort).
/// Returns [`CandidateHit`] ascending by distance; `take` caps the
/// output (`0` = all). `kana` prefilters by script before scoring;
/// indexes still address `bank` because filtering happens after
/// `enumerate`.
///
/// Candidate scoring is data-parallel (rayon): each score is a pure
/// function of shared `&Query`/`&Template` refs, so any thread
/// computes bit-identical values. The final sort stays sequential,
/// keeping the output deterministic regardless of thread count.
pub fn find_candidates(
    query: &Query,
    bank: &[Template],
    take: usize,
    kana: KanaFilter,
) -> Vec<CandidateHit> {
    use rayon::prelude::*;
    let mut out: Vec<CandidateHit> = bank
        .par_iter()
        .enumerate()
        .map(|(i, t)| CandidateHit {
            idx: i,
            distance: rank_element(query, t),
            character: t.character,
            is_kana: is_kana(t.character),
        })
        .filter(|candidate| match kana {
            KanaFilter::All => true,
            KanaFilter::SkipKana => !candidate.is_kana,
        })
        .collect();
    out.sort_by_key(|h| (h.distance, h.idx));
    out.dedup_by_key(|h| h.character);
    if take > 0 {
        out.truncate(take);
    }
    out
}

#[cfg(test)]
mod rank_tests {
    use super::*;
    use crate::geometry::{Point, Stroke};
    use crate::preprocess::trajectories_from_strokes;

    fn kstroke(pts: &[(f32, f32)]) -> Stroke {
        Stroke::new(pts.iter().map(|&(x, y)| Point::new(x, y)).collect())
    }

    fn bank_two() -> Vec<Template> {
        // 'h': one horizontal; 'v': one vertical (pen space).
        let h = kstroke(&[(10.0, 50.0), (90.0, 50.0)]);
        let v = kstroke(&[(50.0, 10.0), (50.0, 90.0)]);
        vec![
            Template::new('h', trajectories_from_strokes(&[h], false)),
            Template::new('v', trajectories_from_strokes(&[v], false)),
        ]
    }

    fn query_of(strokes: &[Stroke]) -> Query {
        Query::drawn(strokes)
    }

    #[test]
    fn fusion_components_recompose_the_total() {
        let bank = bank_two();
        let q = query_of(&[kstroke(&[(12.0, 52.0), (88.0, 48.0)])]);
        let f = rank_fused(&q, &bank[0]);
        // Components recompose (up to float association order), and the
        // rounded total matches rank_element exactly.
        assert!((f.total - ((f.shape + f.layout) * f.size + f.missing)).abs() < 1e-6);
        assert_eq!(rank_element(&q, &bank[0]), f.total.max(0.0).round() as u64);
    }

    #[test]
    fn fusion_components_stay_nonnegative_and_finite() {
        // Structural invariant: shape/layout/missing/order are sums of
        // non-negative terms (the unswap fix keeps layout exact, so it
        // can no longer borrow its way negative).
        let bank = bank_two();
        let queries = [
            query_of(&[kstroke(&[(12.0, 52.0), (88.0, 48.0)])]),
            query_of(&[kstroke(&[(52.0, 12.0), (48.0, 88.0)])]),
            query_of(&[
                kstroke(&[(10.0, 10.0), (90.0, 10.0)]),
                kstroke(&[(10.0, 10.0), (10.0, 90.0)]),
            ]),
        ];
        for q in &queries {
            for t in &bank {
                let f = rank_fused(q, t);
                assert!(f.shape >= 0.0, "{f:?}");
                assert!(f.layout >= 0.0, "{f:?}");
                assert!(f.missing >= 0.0, "{f:?}");
                assert!(f.order_pen >= 0.0, "{f:?}");
                assert!(f.total.is_finite(), "{f:?}");
            }
        }
    }

    #[test]
    fn assignment_detail_maps_strokes_with_costs() {
        let bank = bank_two();
        let q = query_of(&[kstroke(&[(12.0, 52.0), (88.0, 48.0)])]);
        // Near-horizontal query on the horizontal template: single
        // pair q0→t0 with a small shape cost and no order penalty.
        let pairs = assignment_detail(&q, &bank[0]);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].qy, pairs[0].tm), (0, 0));
        assert_eq!(pairs[0].ord, 0.0);
        assert!(pairs[0].cost >= 0.0);
        // Same query on the vertical template still assigns (greedy
        // takes the only candidate) but at a much higher cost.
        let bad = assignment_detail(&q, &bank[1]);
        assert_eq!(bad.len(), 1);
        assert!(bad[0].cost > pairs[0].cost + 100.0);
        // Two-stroke query vs one-stroke template: one pair, one
        // unassigned query stroke (absent from the pairs).
        let q2 = query_of(&[
            kstroke(&[(12.0, 52.0), (88.0, 48.0)]),
            kstroke(&[(52.0, 12.0), (48.0, 88.0)]),
        ]);
        let pairs2 = assignment_detail(&q2, &bank[0]);
        assert_eq!(pairs2.len(), 1);
    }

    #[test]
    fn identical_query_ranks_its_template_first() {
        let bank = bank_two();
        let q = query_of(&[kstroke(&[(12.0, 52.0), (88.0, 48.0)])]);
        let ranked = find_candidates(&q, &bank, 0, KanaFilter::All);
        assert_eq!(ranked[0].idx, 0);
        assert!(ranked[0].distance < ranked[1].distance);
    }

    #[test]
    fn kana_filter_selects_script_before_scoring() {
        let k = kstroke(&[(10.0, 50.0), (90.0, 50.0)]);
        let bank = vec![
            Template::new(
                'あ',
                trajectories_from_strokes(std::slice::from_ref(&k), false),
            ),
            Template::new('x', trajectories_from_strokes(&[k], false)),
        ];
        let q = query_of(&[kstroke(&[(12.0, 52.0), (88.0, 48.0)])]);
        assert_eq!(find_candidates(&q, &bank, 0, KanaFilter::All).len(), 2);
        let all = find_candidates(&q, &bank, 0, KanaFilter::All);
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|h| h.is_kana));
        assert!(all.iter().any(|h| !h.is_kana));
        // SkipKana keeps only non-kana, still bank-indexed.
        let kanji = find_candidates(&q, &bank, 0, KanaFilter::SkipKana);
        assert_eq!(kanji.len(), 1);
        assert_eq!(kanji[0].idx, 1);
        assert!(!kanji[0].is_kana);
    }

    #[test]
    fn position_bits_see_above_and_below() {
        // comp strictly above orig with a gap → bit 0 only (plus
        // nothing else: no overlap, no touching).
        let orig = (0.3, 0.5, 0.7, 0.9);
        let comp = (0.3, 0.1, 0.7, 0.2);
        let full = (0.0, 0.0, 1.0, 1.0);
        let bits = compute_positions(orig, comp, full, false, false);
        assert_ne!(bits & (1 << 0), 0, "{bits:016b}");
        // And the mirror: comp strictly below → bit 3.
        let bits = compute_positions(comp, orig, full, false, false);
        assert_ne!(bits & (1 << 3), 0, "{bits:016b}");
    }

    #[test]
    fn pos_diff_is_zero_for_identical_and_positive_for_far() {
        assert_eq!(pos_diff(0b1, 0b1), 0);
        // far-above vs far-below on the vertical axis: first bits
        // differ by 7 slots → 49, lasts likewise → 98 total… but each
        // axis contributes first+last; horizontal empty on both → 0.
        assert_eq!(pos_diff(1 << 0, 1 << 3), 98);
        assert!(pos_diff(1 << 0, 1 << 2) < 98);
    }
}
