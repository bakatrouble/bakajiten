//! Priority codes (`ke_pri`/`re_pri`, 56 observed): stored directly
//! as variants (1 byte archived). Unknown codes fail pack loudly.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Priority {
    Gai1,
    Gai2,
    Ichi1,
    Ichi2,
    News1,
    News2,
    Nf01,
    Nf02,
    Nf03,
    Nf04,
    Nf05,
    Nf06,
    Nf07,
    Nf08,
    Nf09,
    Nf10,
    Nf11,
    Nf12,
    Nf13,
    Nf14,
    Nf15,
    Nf16,
    Nf17,
    Nf18,
    Nf19,
    Nf20,
    Nf21,
    Nf22,
    Nf23,
    Nf24,
    Nf25,
    Nf26,
    Nf27,
    Nf28,
    Nf29,
    Nf30,
    Nf31,
    Nf32,
    Nf33,
    Nf34,
    Nf35,
    Nf36,
    Nf37,
    Nf38,
    Nf39,
    Nf40,
    Nf41,
    Nf42,
    Nf43,
    Nf44,
    Nf45,
    Nf46,
    Nf47,
    Nf48,
    Spec1,
    Spec2,
}

/// `from_index` order. Pinned by tests: appending is fine,
/// reordering is a format break.
const TABLE: [&str; 56] = [
    "gai1", "gai2", "ichi1", "ichi2", "news1", "news2", "nf01", "nf02", "nf03", "nf04", "nf05",
    "nf06", "nf07", "nf08", "nf09", "nf10", "nf11", "nf12", "nf13", "nf14", "nf15", "nf16", "nf17",
    "nf18", "nf19", "nf20", "nf21", "nf22", "nf23", "nf24", "nf25", "nf26", "nf27", "nf28", "nf29",
    "nf30", "nf31", "nf32", "nf33", "nf34", "nf35", "nf36", "nf37", "nf38", "nf39", "nf40", "nf41",
    "nf42", "nf43", "nf44", "nf45", "nf46", "nf47", "nf48", "spec1", "spec2",
];

impl Priority {
    /// Code to variant, via the table.
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
            0 => Self::Gai1,
            1 => Self::Gai2,
            2 => Self::Ichi1,
            3 => Self::Ichi2,
            4 => Self::News1,
            5 => Self::News2,
            6 => Self::Nf01,
            7 => Self::Nf02,
            8 => Self::Nf03,
            9 => Self::Nf04,
            10 => Self::Nf05,
            11 => Self::Nf06,
            12 => Self::Nf07,
            13 => Self::Nf08,
            14 => Self::Nf09,
            15 => Self::Nf10,
            16 => Self::Nf11,
            17 => Self::Nf12,
            18 => Self::Nf13,
            19 => Self::Nf14,
            20 => Self::Nf15,
            21 => Self::Nf16,
            22 => Self::Nf17,
            23 => Self::Nf18,
            24 => Self::Nf19,
            25 => Self::Nf20,
            26 => Self::Nf21,
            27 => Self::Nf22,
            28 => Self::Nf23,
            29 => Self::Nf24,
            30 => Self::Nf25,
            31 => Self::Nf26,
            32 => Self::Nf27,
            33 => Self::Nf28,
            34 => Self::Nf29,
            35 => Self::Nf30,
            36 => Self::Nf31,
            37 => Self::Nf32,
            38 => Self::Nf33,
            39 => Self::Nf34,
            40 => Self::Nf35,
            41 => Self::Nf36,
            42 => Self::Nf37,
            43 => Self::Nf38,
            44 => Self::Nf39,
            45 => Self::Nf40,
            46 => Self::Nf41,
            47 => Self::Nf42,
            48 => Self::Nf43,
            49 => Self::Nf44,
            50 => Self::Nf45,
            51 => Self::Nf46,
            52 => Self::Nf47,
            53 => Self::Nf48,
            54 => Self::Spec1,
            55 => Self::Spec2,
            _ => return None,
        })
    }

    /// Variant to code.
    pub fn entity(self) -> &'static str {
        TABLE[self as usize]
    }

    /// Fold raw codes into variants, plus the unknown codes
    /// (borrowed — pack fails loudly on these).
    pub fn from_strs<'a>(tags: impl IntoIterator<Item = &'a str>) -> (Vec<Self>, Vec<&'a str>) {
        let mut out = Vec::new();
        let mut unknown = Vec::new();
        for tag in tags {
            match Self::from_entity(tag) {
                Some(priority) => out.push(priority),
                None => unknown.push(tag),
            }
        }
        (out, unknown)
    }
}

#[cfg(test)]
mod priority_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 56);
        for (i, code) in TABLE.iter().enumerate() {
            let priority = Priority::from_index(i as u8).expect("dense 0..56");
            assert_eq!(priority.entity(), *code);
        }
        assert_eq!(Priority::from_index(56), None);
    }

    #[test]
    fn fold_decode_roundtrips() {
        let (values, unknown) = Priority::from_strs(["news1", "nf01"]);
        assert!(unknown.is_empty());
        let back: Vec<Priority> =
            postcard::from_bytes(&postcard::to_allocvec(&values).expect("encode")).expect("decode");
        assert_eq!(back, vec![Priority::News1, Priority::Nf01]);
    }

    #[test]
    fn unknown_tags_report_without_storing() {
        let (values, unknown) = Priority::from_strs(["news1", "future-code"]);
        assert_eq!(unknown, vec!["future-code"]);
        assert_eq!(values, vec![Priority::News1]);
    }
}
