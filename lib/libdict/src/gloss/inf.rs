//! Kanji/reading info codes (`ke_inf`/`re_inf` entities, one
//! shared pool of 11): stored directly as variants instead of
//! repeated entity strings. Unknown codes at pack time fail loudly
//! (see `from_strs`) — same contract as [`Pos`]. Sense-level `info`
//! is free text and stays a `String` (see `SenseExtras`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Inf {
    Ateji,
    Gikun,
    IrregularKanji,
    IrregularKana,
    IrregularOkurigana,
    OutdatedKanji,
    OutdatedKana,
    RareKanji,
    RareKana,
    SearchKanji,
    SearchKana,
}

/// `from_index` order. Pinned by tests: appending is fine,
/// reordering is a format break.
const TABLE: [&str; 11] = [
    "&ateji;", "&gikun;", "&iK;", "&ik;", "&io;", "&oK;", "&ok;", "&rK;", "&rk;", "&sK;", "&sk;",
];

impl Inf {
    /// Entity to variant, via the table.
    pub fn from_entity(tag: &str) -> Option<Self> {
        TABLE
            .iter()
            .position(|&e| e == tag)
            .and_then(|i| u8::try_from(i).ok())
            .and_then(Self::from_index)
    }

    /// Discriminant to variant. Single place where table order meets
    /// discriminant order.
    pub fn from_index(i: u8) -> Option<Self> {
        Some(match i {
            0 => Self::Ateji,
            1 => Self::Gikun,
            2 => Self::IrregularKanji,
            3 => Self::IrregularKana,
            4 => Self::IrregularOkurigana,
            5 => Self::OutdatedKanji,
            6 => Self::OutdatedKana,
            7 => Self::RareKanji,
            8 => Self::RareKana,
            9 => Self::SearchKanji,
            10 => Self::SearchKana,
            _ => return None,
        })
    }

    /// Variant to entity.
    pub fn entity(self) -> &'static str {
        TABLE[self as usize]
    }

    /// Fold raw entity strings into variants. Returns the variants
    /// plus the unknown tags (borrowed — pack fails loudly on these).
    pub fn from_strs<'a>(tags: impl IntoIterator<Item = &'a str>) -> (Vec<Self>, Vec<&'a str>) {
        let mut out = Vec::new();
        let mut unknown = Vec::new();
        for tag in tags {
            match Self::from_entity(tag) {
                Some(inf) => out.push(inf),
                None => unknown.push(tag),
            }
        }
        (out, unknown)
    }
}

#[cfg(test)]
mod inf_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 11);
        for (i, entity) in TABLE.iter().enumerate() {
            let inf = Inf::from_index(i as u8).expect("dense 0..11");
            assert_eq!(inf.entity(), *entity);
        }
        assert_eq!(Inf::from_index(11), None);
    }

    #[test]
    fn case_pairs_stay_distinct() {
        assert_ne!(Inf::from_entity("&iK;"), Inf::from_entity("&ik;"));
        assert_ne!(Inf::from_entity("&oK;"), Inf::from_entity("&ok;"));
    }

    #[test]
    fn fold_decode_roundtrips() {
        let (values, unknown) = Inf::from_strs(["&ateji;", "&sk;"]);
        assert!(unknown.is_empty());
        let back: Vec<Inf> =
            postcard::from_bytes(&postcard::to_allocvec(&values).expect("encode")).expect("decode");
        assert_eq!(back, vec![Inf::Ateji, Inf::SearchKana]);
    }

    #[test]
    fn unknown_tags_report_without_storing() {
        let (values, unknown) = Inf::from_strs(["&ateji;", "&future-tag;"]);
        assert_eq!(unknown, vec!["&future-tag;"]);
        assert_eq!(values, vec![Inf::Ateji]);
    }
}
