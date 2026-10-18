//! Conjugated-form search (deinflection): rule-based suffix
//! stripping at query time against classes compiled from each
//! entry's `pos` tags. The index stays dictionary-form-only
//! (expansion would multiply key rows ~50x and rot on every rule
//! change; rules as data stay fixable without repacking).
//!
//! Engine recap (rule inventory lives in the table source
//! [`deinflect_table`]): BFS over surfaces, every
//! suffix-matching rule fires, visited-set on surfaces keyed by
//! surface alone, depth cap 5. Revisit merges class claims by union
//! (same surface, different derivation — e.g. たべられる reaches
//! たべる as godan-5r and as ichidan; first-wins would drop the
//! ichidan claim and the class filter would reject a correct
//! answer). The final exact lookup keeps hits whose entry classes
//! intersect the claim set (empty claim set = any entry —
//! transparent-only chains). Every candidate is verified against
//! the dictionary, so over-generation costs hash lookups, never
//! wrong answers.
//!
//! Classes compute from `pos` on the fly — no storage, so rule fixes
//! ship in the binary with no artifact version bump.

use std::collections::{HashMap, VecDeque};

use super::deinflect_table::rules_for;
use super::pos::Pos;
use super::record::{EntryRecord, SenseDetail};
use super::search::normalize_jap;

/// Inflection class: one per JMdict conjugation family.
/// Runtime-only, never stored —
/// discriminants are engine-internal (no table to keep in sync).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InflClass {
    V1,
    V5k,
    V5g,
    V5s,
    V5t,
    V5n,
    V5b,
    V5m,
    V5r,
    V5u,
    V5kS,
    V5aru,
    Vk,
    Vs,
    AdjI,
    Vz,
    /// `v2*`/`v4*` classical plus `vn`/`vr`/archaic adjectives:
    /// distinct from every modern class, no rules yet (§6).
    Classical,
}

impl InflClass {
    const COUNT: usize = 17;
}

/// Accumulated class claims for one BFS surface: union semantics
/// (plain bool array, never a bitmask — see crate storage rules).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassSet([bool; InflClass::COUNT]);

impl ClassSet {
    pub fn empty() -> Self {
        Self([false; InflClass::COUNT])
    }

    pub fn of(classes: &[InflClass]) -> Self {
        let mut set = Self::empty();
        for &c in classes {
            set.insert(c);
        }
        set
    }

    pub fn insert(&mut self, class: InflClass) {
        self.0[class as usize] = true;
    }

    /// Union `other` in. States only grow along a path.
    pub fn absorb(&mut self, other: &Self) {
        for (slot, claim) in self.0.iter_mut().zip(other.0.iter()) {
            *slot |= claim;
        }
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.0.iter().zip(other.0.iter()).any(|(a, b)| *a && *b)
    }

    pub fn contains(&self, class: InflClass) -> bool {
        self.0[class as usize]
    }

    pub fn is_empty(&self) -> bool {
        !self.0.iter().any(|&claim| claim)
    }
}

/// One deinflected candidate: dictionary-form surface, the class
/// claims behind it, and the reason chain that produced it
/// (display: `食べて → 食べる · conjunctive`).
pub struct Deinflection {
    pub surface: String,
    pub classes: ClassSet,
    pub chain: Vec<&'static str>,
}

/// BFS depth cap: `食べさせられたくなかった`-grade chains settle
/// within 5 hops (§3 worked chains max out at depth 3).
const MAX_DEPTH: u8 = 5;

/// Deinflect a query surface into dictionary-form candidates with
/// reason chains, shallowest first. No dictionary verification here
/// (callers exact-lookup each surface and class-filter the hits).
/// Chains are first-discovery (shortest derivation); classes are
/// the final union over every derivation that reached the surface.
pub fn deinflect(query: &str) -> Vec<Deinflection> {
    use std::collections::hash_map::Entry;
    let start = normalize_jap(query);
    // Discovery order (output order) plus per-surface state.
    let mut order = Vec::new();
    let mut states: HashMap<String, (ClassSet, Vec<&'static str>)> = HashMap::new();
    let mut queue = VecDeque::new();
    states.insert(start.clone(), (ClassSet::empty(), Vec::new()));
    queue.push_back((start, 0u8));
    while let Some((surface, depth)) = queue.pop_front() {
        if depth >= MAX_DEPTH {
            continue;
        }
        let last = match surface.chars().next_back() {
            Some(c) => c,
            None => continue,
        };
        // Longest-`from`-first within the bucket: the most specific
        // peel wins ties on first discovery.
        let (classes, chain) = states[&surface].clone();
        for rule in rules_for(last) {
            let Some(stem) = surface.strip_suffix(rule.from.as_str()) else {
                continue;
            };
            let next = format!("{stem}{}", rule.to);
            // Empty-surface candidates are never entries (§6).
            if next.is_empty() {
                continue;
            }
            let mut merged = classes.clone();
            merged.absorb(&rule.out);
            let mut next_chain = chain.clone();
            next_chain.push(rule.reason);
            match states.entry(next.clone()) {
                Entry::Vacant(slot) => {
                    slot.insert((merged, next_chain));
                    order.push(next.clone());
                    queue.push_back((next, depth + 1));
                }
                // Same surface, new claims: merge and re-expand
                // (first chain stays — shortest derivation wins
                // display; classes are the union).
                Entry::Occupied(mut slot) => {
                    let (known, _) = slot.get_mut();
                    let before = known.clone();
                    known.absorb(&merged);
                    if *known != before {
                        queue.push_back((next, depth + 1));
                    }
                }
            }
        }
    }
    order
        .into_iter()
        .map(|surface| {
            let (classes, chain) = states.remove(&surface).expect("ordered");
            Deinflection {
                surface,
                classes,
                chain,
            }
        })
        .collect()
}

/// Entry classes from `pos` tags (§2 compiler mapping): union over
/// senses. Special classes keep their row behavior (`v5k-s` is
/// regular 5k plus the te/ta special; `v5aru` conjugates 5r-like),
/// so entries carry both. Transitivity markers (`vi`/`vt`),
/// `v-unspec`, nouns and na-adjectives carry no class; names never
/// reach here (name senses hold `name_types`, not `pos`).
pub fn entry_classes(entry: &EntryRecord) -> ClassSet {
    let mut classes = ClassSet::empty();
    for sense in &entry.senses {
        let Some(detail) = &sense.detail else {
            continue;
        };
        if let SenseDetail::Word { pos, .. } = &**detail {
            for class in pos.iter().flat_map(|tag| classes_of(*tag)) {
                classes.insert(*class);
            }
        }
    }
    classes
}

/// One `pos` entity to its classes (empty = no conjugation). Small
/// static arrays, no allocation — callers iterate.
fn classes_of(tag: Pos) -> &'static [InflClass] {
    use InflClass as Class;
    use Pos as Tag;
    match tag {
        Tag::V1 | Tag::V1S => &[Class::V1],
        Tag::V5k => &[Class::V5k],
        Tag::V5g => &[Class::V5g],
        Tag::V5s => &[Class::V5s],
        Tag::V5t => &[Class::V5t],
        Tag::V5n => &[Class::V5n],
        Tag::V5b => &[Class::V5b],
        Tag::V5m => &[Class::V5m],
        Tag::V5r | Tag::V5rI => &[Class::V5r],
        Tag::V5u | Tag::V5uS => &[Class::V5u],
        Tag::V5kS => &[Class::V5kS, Class::V5k],
        Tag::V5aru => &[Class::V5aru, Class::V5r],
        Tag::Vk => &[Class::Vk],
        Tag::Vs | Tag::VsS | Tag::VsI | Tag::VsC => &[Class::Vs],
        Tag::AdjI | Tag::AdjIx => &[Class::AdjI],
        Tag::Vz => &[Class::Vz],
        Tag::V2aS
        | Tag::V2bK
        | Tag::V2dS
        | Tag::V2gK
        | Tag::V2gS
        | Tag::V2hK
        | Tag::V2hS
        | Tag::V2kK
        | Tag::V2kS
        | Tag::V2mS
        | Tag::V2nS
        | Tag::V2rK
        | Tag::V2rS
        | Tag::V2sS
        | Tag::V2tK
        | Tag::V2tS
        | Tag::V2wS
        | Tag::V2yK
        | Tag::V2yS
        | Tag::V2zS
        | Tag::V4b
        | Tag::V4g
        | Tag::V4h
        | Tag::V4k
        | Tag::V4m
        | Tag::V4r
        | Tag::V4s
        | Tag::V4t
        | Tag::Vn
        | Tag::Vr
        | Tag::AdjKu
        | Tag::AdjShiku => &[Class::Classical],
        _ => &[],
    }
}
