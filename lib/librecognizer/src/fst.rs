//! Language-model FST support.
//!
//! ## Status of the production `compact_lm` file
//!
//! `ja.compact.fst.local` (20.7 MB, 1,635,817 states, 6,398,903 arcs) uses a
//! **proprietary** OpenFst type:
//!
//! - header type string: `compact_lm`, arc type `standard`, version 2;
//! - implemented by `fst::speech::CompactLmFstImpl` (`ShortArc`, mmap-aligned
//!   reads — see `CompactLmFst::Read: Alignment failed` in `libdigitalink.so`);
//! - decoded through `NGramFstMatcher` + `QuantizeAndRemoveFinalMapper`.
//!
//! Stock OpenFst (`fstinfo`/`fstprint`) rejects it with
//! `Unknown FST type compact_lm`, and no open-source implementation of this
//! class exists, so the requested `fstprint -> text -> rustfst` route is
//! blocked on the file itself. `tools/fst_probe.py` records the format
//! findings. Decoding therefore runs on network posteriors + character-class
//! scores (see [`crate::decode`]); the [`LmScorer`] hook is ready for a real
//! LM.
//!
//! ## What *is* implemented here
//!
//! [`TextFst`]: loader for the OpenFst **text** format
//! (`src dst ilabel [olabel] [weight]`, `state [final]`), plus an
//! epsilon/backoff-following [`LmScorer`] over it. Use it with an openly
//! trained character LM (e.g. KenLM ARPA -> FST text) to restore LM decoding.

use crate::decode::LmScorer;
use std::collections::HashMap;

/// One arc in tropical semiring (cost = -ln prob).
#[derive(Debug, Clone)]
pub struct Arc {
    pub dst: u32,
    pub ilabel: u32,
    pub weight: f32,
}

/// Minimal OpenFst text-format FST (acceptor or transducer; only ilabels
/// and weights are used for scoring).
#[derive(Debug, Clone, Default)]
pub struct TextFst {
    pub start: u32,
    pub finals: HashMap<u32, f32>,
    pub arcs: HashMap<u32, Vec<Arc>>,
}

#[derive(Debug)]
pub struct FstParseError(pub String);

impl TextFst {
    /// Parse OpenFst text format. Label strings are resolved through
    /// `syms` (symbol -> id); unknown symbols are skipped.
    pub fn parse(text: &str, syms: &HashMap<String, u32>) -> Result<Self, FstParseError> {
        let mut fst = TextFst::default();
        let mut first = true;
        for (lineno, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            let num = |s: &str| -> Result<u32, FstParseError> {
                s.parse::<u32>()
                    .or_else(|_| {
                        syms.get(s).copied().ok_or_else(|| FstParseError(format!("line {lineno}: bad state/symbol {s}")))
                    })
                    .map_err(|e| FstParseError(e.0))
            };
            match parts.len() {
                1 => {
                    let s = num(parts[0])?;
                    fst.finals.insert(s, 0.0);
                    if first {
                        fst.start = s;
                        first = false;
                    }
                }
                2 => {
                    let (s, w) = (num(parts[0])?, parts[1].parse::<f32>().map_err(|_| FstParseError(format!("line {lineno}: bad weight")))?);
                    fst.finals.insert(s, w);
                    if first {
                        fst.start = s;
                        first = false;
                    }
                }
                3 | 4 | 5 => {
                    let (s, d) = (num(parts[0])?, num(parts[1])?);
                    let ilabel = num(parts[2])?;
                    let weight = if parts.len() == 5 {
                        parts[4].parse::<f32>().map_err(|_| FstParseError(format!("line {lineno}: bad weight")))?
                    } else if parts.len() == 4 {
                        // Could be olabel (no weight) — treat as weight only
                        // if it parses as a number, else weight 0.
                        parts[3].parse::<f32>().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    if first {
                        fst.start = s;
                        first = false;
                    }
                    fst.arcs.entry(s).or_default().push(Arc { dst: d, ilabel, weight });
                }
                _ => return Err(FstParseError(format!("line {lineno}: bad field count"))),
            }
        }
        Ok(fst)
    }
}

/// LM scorer that walks `fst`, following epsilon (ilabel 0) backoff arcs
/// when no direct arc matches. Costs accumulate; unknown labels cost 0.
#[derive(Debug, Clone)]
pub struct FstLm {
    pub fst: TextFst,
    pub max_backoff: usize,
}

impl LmScorer for FstLm {
    fn score(&self, prefix: &[usize], next_label: usize) -> f32 {
        let state = prefix.iter().fold(self.fst.start, |s, &l| self.step(s, l as u32).unwrap_or(s));
        let _ = &prefix;
        self.cost(state, next_label as u32).unwrap_or(0.0)
    }

    fn final_cost(&self, prefix: &[usize]) -> f32 {
        let state = prefix.iter().fold(self.fst.start, |s, &l| self.step(s, l as u32).unwrap_or(s));
        self.fst.finals.get(&state).copied().unwrap_or(0.0)
    }
}

impl FstLm {
    fn step(&self, mut state: u32, label: u32) -> Option<u32> {
        for _ in 0..=self.max_backoff {
            if let Some(arcs) = self.fst.arcs.get(&state) {
                if let Some(a) = arcs.iter().find(|a| a.ilabel == label) {
                    return Some(a.dst);
                }
                if let Some(b) = arcs.iter().find(|a| a.ilabel == 0) {
                    state = b.dst;
                    continue;
                }
            }
            return None;
        }
        None
    }

    fn cost(&self, mut state: u32, label: u32) -> Option<f32> {
        let mut cost = 0.0;
        for _ in 0..=self.max_backoff {
            if let Some(arcs) = self.fst.arcs.get(&state) {
                if let Some(a) = arcs.iter().find(|a| a.ilabel == label) {
                    return Some(cost + a.weight);
                }
                if let Some(b) = arcs.iter().find(|a| a.ilabel == 0) {
                    cost += b.weight;
                    state = b.dst;
                    continue;
                }
            }
            return None;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> (TextFst, HashMap<String, u32>) {
        let syms: HashMap<String, u32> =
            [("<eps>", 0), ("a", 1), ("b", 2)].iter().map(|(a, b)| (a.to_string(), *b)).collect();
        let fst = TextFst::parse("0 1 a 0.5\n1 2 b 0.25\n1 1 <eps> 1.0\n2 0.1\n", &syms).unwrap();
        (fst, syms)
    }

    #[test]
    fn parse_and_walk() {
        let (fst, _) = tiny();
        assert_eq!(fst.start, 0);
        assert_eq!(fst.arcs[&0].len(), 1);
        let lm = FstLm { fst, max_backoff: 4 };
        assert!((lm.score(&[], 1) - 0.5).abs() < 1e-6);
        // label 2 from state 0: no direct arc, no epsilon -> 0.0 fallback
        assert_eq!(lm.score(&[], 2), 0.0);
        // after [1]: state 1, direct arc b costs 0.25
        assert!((lm.score(&[1], 2) - 0.25).abs() < 1e-6);
        assert!((lm.final_cost(&[1, 2]) - 0.1).abs() < 1e-6);
    }
}
