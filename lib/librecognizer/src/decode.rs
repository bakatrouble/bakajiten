//! CTC decoding: greedy + prefix beam search with an [`LmScorer`] hook.
//!
//! Ported from tegaki (`google-recog/src/decode.rs`). The production
//! system combines the network posteriors with a character n-gram
//! FST and character-class weights; the n-gram FST file
//! (`compact_lm`, proprietary format) is not readable, so decoding
//! uses the network posteriors plus the character-class feature
//! function extracted from the recospec at conversion time (see
//! [`crate::convert`]). Plug a real LM in via [`LmScorer`].
//!
//! Unlike tegaki, the codepoints and class tables are not bundled
//! via `include_str!`: [`ClassBoost::build`] takes them from the
//! converted bundle header, and every entry point takes a
//! [`crate::vocab::Vocab`].

use crate::vocab::Vocab;

/// A language-model score for extending `prefix` (as FST label ids) with
/// `next_label`. Scores are log-domain costs (lower is better), matching the
/// sign convention of OpenFst tropical weights.
pub trait LmScorer {
    fn score(&self, prefix: &[usize], next_label: usize) -> f32;
    fn final_cost(&self, prefix: &[usize]) -> f32 {
        let _ = prefix;
        0.0
    }
}

/// No LM: all extensions cost 0.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLm;
impl LmScorer for NoLm {
    fn score(&self, _prefix: &[usize], _next_label: usize) -> f32 {
        0.0
    }
}

/// Character-class unigram boost from the converted bundle
/// (`char_classes` + `precontext_classes`, extracted from the
/// recospec's char-class table).
/// Each emitted label belongs to script classes (hiragana/kanji/…) with
/// learned log-weights; rare-in-language classes are penalized. The mapping
/// label -> class comes from `precontext_classes`; this scorer takes a
/// ready-made per-label table.
#[derive(Debug, Clone)]
pub struct ClassBoost {
    /// Per FST-label additive score (index by label id).
    pub per_label: Vec<f32>,
}

impl ClassBoost {
    /// Build the production Japanese class table from converted
    /// Converted bundle parts: `weights` is `(class, weight)` in wire
    /// order, `spec` the `precontext_classes` text.
    /// `punct_ja`/`quote_ja` map to the `punct_`/`quote_` weight
    /// classes; anything uncovered (space, `<epsilon>`, …) falls
    /// back to `no_char_class`.
    pub fn build(codepoints: &[String], weights: &[(String, f32)], spec: &str) -> Self {
        let weights: std::collections::HashMap<&str, f32> =
            weights.iter().map(|(c, w)| (c.as_str(), *w)).collect();
        let mut class_of: std::collections::HashMap<char, &str> = std::collections::HashMap::new();
        for line in spec.lines() {
            if line.is_empty() {
                continue;
            }
            let sp = line.find(' ').unwrap_or(line.len());
            let (mut cls, tail) = (&line[..sp], line[sp..].chars().collect::<Vec<char>>());
            if cls == "punct_ja" {
                cls = "punct_";
            } else if cls == "quote_ja" {
                cls = "quote_";
            }
            // Skip the single separator space after the class name. (No class
            // set in this spec legitimately starts with a space; space itself
            // maps to no_char_class.)
            let mut i = usize::from(tail.first() == Some(&' '));
            while i < tail.len() {
                if tail[i..].starts_with(&['[', '[', '0', 'x']) {
                    // [[0xA]] or [[0xA-0xB]]
                    let mut j = i + 4;
                    while j < tail.len() && tail[j] != ']' {
                        j += 1;
                    }
                    let hex: String = tail[i + 4..j].iter().collect();
                    let (lo, hi) = if let Some(k) = hex.find('-') {
                        (
                            u32::from_str_radix(hex[..k].trim_start_matches("0x"), 16).unwrap(),
                            u32::from_str_radix(hex[k + 1..].trim_start_matches("0x"), 16).unwrap(),
                        )
                    } else {
                        let v = u32::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap();
                        (v, v)
                    };
                    for cp in lo..=hi {
                        if let Some(ch) = char::from_u32(cp) {
                            class_of.insert(ch, cls);
                        }
                    }
                    i = j + 2; // skip ']]'
                } else {
                    let ch = tail[i];
                    class_of.entry(ch).or_insert(cls);
                    i += 1;
                }
            }
        }
        let fallback = weights.get("no_char_class").copied().unwrap_or(0.0);
        let mut per_label = vec![fallback; codepoints.len() + 4];
        for (label, ch) in label_chars(codepoints) {
            let w = ch
                .and_then(|c| class_of.get(&c))
                .and_then(|cls| weights.get(cls))
                .copied()
                .unwrap_or(fallback);
            per_label[label] = w;
        }
        Self { per_label }
    }
}

/// (FST label id, representative char) for all labels:
/// 0 = `<epsilon>`, 1 = `<reserved>`, 2.. = codepoints, then
/// `<S>` and `</S>`.
fn label_chars(codepoints: &[String]) -> Vec<(usize, Option<char>)> {
    let mut out = Vec::with_capacity(codepoints.len() + 4);
    out.push((0, None)); // <epsilon>
    out.push((1, None)); // <reserved>
    for (i, s) in codepoints.iter().enumerate() {
        let ch = if s == "[[space]]" { Some(' ') } else { s.chars().next() };
        out.push((i + 2, ch));
    }
    out.push((codepoints.len() + 2, None)); // <S>
    out.push((codepoints.len() + 3, None)); // </S>
    out
}

impl LmScorer for ClassBoost {
    fn score(&self, _prefix: &[usize], next_label: usize) -> f32 {
        self.per_label.get(next_label).copied().unwrap_or(0.0)
    }
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let m = logits.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let mut s = 0.0;
    let mut out = Vec::with_capacity(logits.len());
    for &v in logits {
        let e = (v - m).exp();
        out.push(e);
        s += e;
    }
    for v in &mut out {
        *v /= s;
    }
    out
}

/// Greedy CTC decode: argmax per frame, collapse repeats, drop blanks.
/// Returns `(text, mean_frame_confidence)`.
pub fn greedy(logits: &[Vec<f32>], vocab: &Vocab) -> (String, f32) {
    let mut text = String::new();
    let mut prev = crate::vocab::BLANK;
    let mut conf = 0.0f32;
    let mut n = 0;
    for row in logits {
        let probs = softmax(row);
        let (best, &p) = probs.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap();
        if best != crate::vocab::BLANK && best != prev {
            if let Some(ch) = vocab.char_of_class(best) {
                text.push_str(ch);
            }
        }
        if best != crate::vocab::BLANK {
            conf += p;
            n += 1;
        }
        prev = best;
    }
    (text, if n > 0 { conf / n as f32 } else { 0.0 })
}

/// One decoded candidate.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    pub score: f32,
}

/// CTC prefix beam search over classes, rescored with `lm` (labels = class+1).
/// Only the top `top_k` classes per frame (plus blank) are expanded — standard
/// production pruning, negligible accuracy impact at `top_k >= 32`.
/// `beam_width` ~ 8–10 (tegaki's GUI passes 8; recospec decoder flag
/// `f12 = 2` is the production beam factor; wider is safer without
/// the full LM).
pub fn beam_search(
    logits: &[Vec<f32>],
    beam_width: usize,
    lm: &dyn LmScorer,
    vocab: &Vocab,
) -> Vec<Candidate> {
    beam_search_topk(logits, beam_width, 64, lm, vocab)
}

/// [`beam_search`] with explicit per-frame class pruning.
pub fn beam_search_topk(
    logits: &[Vec<f32>],
    beam_width: usize,
    top_k: usize,
    lm: &dyn LmScorer,
    vocab: &Vocab,
) -> Vec<Candidate> {
    use std::collections::HashMap;

    #[derive(Clone, Hash, PartialEq, Eq)]
    struct Key {
        labels: Vec<usize>, // FST label ids (blank never stored)
        last_blank: bool,
    }

    // Hypothesis: (log_blank_prob, log_nonblank_prob, lm_cost).
    let mut beam: HashMap<Key, (f32, f32, f32)> = HashMap::new();
    beam.insert(Key { labels: vec![], last_blank: true }, (0.0, f32::NEG_INFINITY, 0.0));

    for row in logits {
        let probs = softmax(row);
        let logp: Vec<f32> = probs.iter().map(|&p| p.max(1e-30).ln()).collect();
        // Per-frame pruning: blank + top_k classes.
        let mut order: Vec<usize> = (0..logp.len().min(crate::vocab::NUM_CLASSES)).filter(|&c| c != crate::vocab::BLANK).collect();
        order.sort_by(|&a, &b| logp[b].partial_cmp(&logp[a]).unwrap());
        order.truncate(top_k);
        let mut next: HashMap<Key, (f32, f32, f32)> = HashMap::new();
        let blank_lp = logp[crate::vocab::BLANK];
        for (key, (lb, lnb, lmc)) in beam {
            let total = log_add(lb, lnb);
            // Extend with blank.
            let kb = Key { labels: key.labels.clone(), last_blank: true };
            let e = next.entry(kb).or_insert((f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0));
            e.0 = log_add(e.0, total + blank_lp);
            e.2 = lmc;
            // Extend with each pruned non-blank class.
            for &c in &order {
                let label = c + 2; // class -> FST label
                let mut labels = key.labels.clone();
                let repeat = !key.last_blank && *labels.last().unwrap_or(&usize::MAX) == label;
                if !repeat {
                    labels.push(label);
                }
                let lm_cost = if repeat { 0.0 } else { lm.score(&key.labels, label) };
                let k = Key { labels, last_blank: false };
                let e = next.entry(k).or_insert((f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0));
                // Repeats can only extend the non-blank path; new labels
                // extend both (CTC prefix rules).
                let from = if repeat { lnb } else { total };
                e.1 = log_add(e.1, from + logp[c] - lm_cost);
                e.2 = lmc;
            }
        }
        // Prune.
        let mut items: Vec<_> = next.into_iter().collect();
        items.sort_by(|a, b| {
            let sa = log_add(a.1 .0, a.1 .1) - a.1 .2;
            let sb = log_add(b.1 .0, b.1 .1) - b.1 .2;
            sb.partial_cmp(&sa).unwrap()
        });
        items.truncate(beam_width);
        beam = items.into_iter().collect();
    }

    let mut out: Vec<Candidate> = beam
        .into_iter()
        .map(|(k, (lb, lnb, lmc))| {
            let text: String = k
                .labels
                .iter()
                .filter_map(|&l| {
                    if l == 0 || l == 1 {
                        return None;
                    }
                    let class = l.checked_sub(2)?;
                    vocab.char_of_class(class)
                })
                .collect();
            Candidate { text, score: -(log_add(lb, lnb) - lmc - lm.final_cost(&k.labels)) }
        })
        .collect();
    out.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());
    out.dedup_by(|a, b| a.text == b.text);
    out
}

fn log_add(a: f32, b: f32) -> f32 {
    if a == f32::NEG_INFINITY {
        return b;
    }
    if b == f32::NEG_INFINITY {
        return a;
    }
    let (m, d) = if a > b { (a, b - a) } else { (b, a - b) };
    m + (1.0 + d.exp()).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_vocab() -> Vocab {
        // class 0 = " " (space), matching the one_hot test below.
        let mut cps = vec![String::from(" "), String::from("!")];
        cps.resize(crate::vocab::BLANK, String::from("x"));
        Vocab::new(cps).expect("test vocab")
    }

    fn one_hot(t: usize, class: usize, n: usize) -> Vec<Vec<f32>> {
        let mut rows = vec![vec![-10.0f32; n]; t];
        for (i, r) in rows.iter_mut().enumerate() {
            r[if i % 2 == 0 { class } else { crate::vocab::BLANK }] = 10.0;
        }
        rows
    }

    #[test]
    fn greedy_collapses_repeats_and_blanks() {
        // class 0 = " " (space), frames alternate space/blank -> three spaces.
        let (text, conf) = greedy(&one_hot(5, 0, crate::vocab::NUM_CLASSES), &tiny_vocab());
        assert_eq!(text, "   ");
        assert!(conf > 0.99);
    }

    #[test]
    fn beam_matches_greedy_on_clear_input() {
        let logits = one_hot(5, 0, crate::vocab::NUM_CLASSES);
        let cands = beam_search(&logits, 5, &NoLm, &tiny_vocab());
        assert!(!cands.is_empty());
        assert_eq!(cands[0].text, "   ");
    }
}
