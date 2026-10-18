//! Gloss type (`g_type` attribute: 4 codes observed). Stored as a
//! single variant — absent stays [`GlossType::Unspecified`] instead
//! of paying an `Option<String>` slot per gloss. Unknown codes at
//! pack time fail loudly. (`g_gend` is dropped from the schema: zero
//! occurrences in the data, no consumer.)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum GlossType {
    Unspecified,
    Expl,
    Fig,
    Lit,
    Tm,
}

/// `from_index` order. Pinned by tests: appending is fine,
/// reordering is a format break.
const TABLE: [&str; 4] = ["expl", "fig", "lit", "tm"];

impl GlossType {
    /// Attribute code to variant (never `Unspecified` — absence maps
    /// there directly).
    pub fn from_entity(code: &str) -> Option<Self> {
        TABLE
            .iter()
            .position(|&e| e == code)
            .and_then(|i| u8::try_from(i).ok())
            .and_then(|i| Self::from_index(i + 1))
    }

    /// Discriminant to variant. Single place where table order meets
    /// discriminant order.
    pub fn from_index(i: u8) -> Option<Self> {
        Some(match i {
            0 => Self::Unspecified,
            1 => Self::Expl,
            2 => Self::Fig,
            3 => Self::Lit,
            4 => Self::Tm,
            _ => return None,
        })
    }

    /// Variant to code (`""` when unspecified).
    pub fn entity(self) -> &'static str {
        match self {
            Self::Unspecified => "",
            Self::Expl => TABLE[0],
            Self::Fig => TABLE[1],
            Self::Lit => TABLE[2],
            Self::Tm => TABLE[3],
        }
    }
}

#[cfg(test)]
mod gloss_type_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 4);
        for (i, code) in TABLE.iter().enumerate() {
            let typ = GlossType::from_index(i as u8 + 1).expect("dense 1..=4");
            assert_eq!(typ.entity(), *code);
        }
        assert_eq!(GlossType::from_index(5), None);
        assert_eq!(GlossType::Unspecified.entity(), "");
    }

    #[test]
    fn codes_roundtrip_through_archive() {
        for code in TABLE {
            let typ = GlossType::from_entity(code).expect("known code");
            let back: GlossType =
                postcard::from_bytes(&postcard::to_allocvec(&typ).expect("encode"))
                    .expect("decode");
            assert_eq!(back, typ);
        }
        assert_eq!(GlossType::from_entity("future-code"), None);
    }
}
