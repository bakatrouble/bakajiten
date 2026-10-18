//! Name-type tags as byte indices: one `u8` per JMnedict
//! `<name_type>` entity (25 fit easily), stored as a plain `Vec<u8>`
//! per translation. Same treatment as `pos` (`Pos`): discriminants
//! are declaration order, guaranteed sequential from 0 by the
//! language; `TABLE` below must stay in the same order (pinned by
//! test). Glosses below come from the entity declarations embedded
//! in `dataimports/JMnedict.xml` itself.

use serde::{Deserialize, Serialize};

/// All `&...;` entities observed in JMnedict `<name_type>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum NameType {
    /// `&char;` — character
    Char,
    /// `&company;` — company name
    Company,
    /// `&creat;` — creature
    Creat,
    /// `&dei;` — deity
    Dei,
    /// `&doc;` — document
    Doc,
    /// `&ev;` — event
    Ev,
    /// `&fem;` — female given name or forename
    Fem,
    /// `&fict;` — fiction
    Fict,
    /// `&given;` — given name or forename, gender not specified
    Given,
    /// `&group;` — group
    Group,
    /// `&leg;` — legend
    Leg,
    /// `&masc;` — male given name or forename
    Masc,
    /// `&myth;` — mythology
    Myth,
    /// `&obj;` — object
    Obj,
    /// `&organization;` — organization name
    Organization,
    /// `&person;` — full name of a particular person
    Person,
    /// `&place;` — place name
    Place,
    /// `&product;` — product name
    Product,
    /// `&relig;` — religion
    Relig,
    /// `&serv;` — service
    Serv,
    /// `&ship;` — ship name
    Ship,
    /// `&station;` — railway station
    Station,
    /// `&surname;` — family or surname
    Surname,
    /// `&unclass;` — unclassified name
    Unclass,
    /// `&work;` — work of art, literature, music, etc. name
    Work,
}

/// Entity per discriminant, in order. Single source for both
/// directions (fold + decode).
const TABLE: [&str; 25] = [
    "&char;",
    "&company;",
    "&creat;",
    "&dei;",
    "&doc;",
    "&ev;",
    "&fem;",
    "&fict;",
    "&given;",
    "&group;",
    "&leg;",
    "&masc;",
    "&myth;",
    "&obj;",
    "&organization;",
    "&person;",
    "&place;",
    "&product;",
    "&relig;",
    "&serv;",
    "&ship;",
    "&station;",
    "&surname;",
    "&unclass;",
    "&work;",
];

impl NameType {
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
            0 => Self::Char,
            1 => Self::Company,
            2 => Self::Creat,
            3 => Self::Dei,
            4 => Self::Doc,
            5 => Self::Ev,
            6 => Self::Fem,
            7 => Self::Fict,
            8 => Self::Given,
            9 => Self::Group,
            10 => Self::Leg,
            11 => Self::Masc,
            12 => Self::Myth,
            13 => Self::Obj,
            14 => Self::Organization,
            15 => Self::Person,
            16 => Self::Place,
            17 => Self::Product,
            18 => Self::Relig,
            19 => Self::Serv,
            20 => Self::Ship,
            21 => Self::Station,
            22 => Self::Surname,
            23 => Self::Unclass,
            24 => Self::Work,
            _ => return None,
        })
    }

    /// Variant to entity.
    pub fn entity(self) -> &'static str {
        TABLE[self as usize]
    }

    /// Human-readable description for tag tooltips (transcribed
    /// from the variant docs above — keep in sync; the match is
    /// exhaustive so new variants fail to compile here).
    pub fn description(self) -> &'static str {
        match self {
            Self::Char => "character",
            Self::Company => "company name",
            Self::Creat => "creature",
            Self::Dei => "deity",
            Self::Doc => "document",
            Self::Ev => "event",
            Self::Fem => "female given name or forename",
            Self::Fict => "fiction",
            Self::Given => "given name or forename, gender not specified",
            Self::Group => "group",
            Self::Leg => "legend",
            Self::Masc => "male given name or forename",
            Self::Myth => "mythology",
            Self::Obj => "object",
            Self::Organization => "organization name",
            Self::Person => "full name of a particular person",
            Self::Place => "place name",
            Self::Product => "product name",
            Self::Relig => "religion",
            Self::Serv => "service",
            Self::Ship => "ship name",
            Self::Station => "railway station",
            Self::Surname => "family or surname",
            Self::Unclass => "unclassified name",
            Self::Work => "work of art, literature, music, etc. name",
        }
    }

    /// Fold raw entity strings into index bytes. Takes an iterator
    /// so callers never allocate a `Vec<String>` just to convert
    /// (borrow parser output or string literals directly). Returns
    /// the bytes plus the unknown tags (borrowed — pack fails loudly
    /// on these, same as `pos`).
    pub fn from_strs<'a>(tags: impl IntoIterator<Item = &'a str>) -> (Vec<Self>, Vec<&'a str>) {
        let mut out = Vec::new();
        let mut unknown = Vec::new();
        for tag in tags {
            match Self::from_entity(tag) {
                Some(name_type) => out.push(name_type),
                None => unknown.push(tag),
            }
        }
        (out, unknown)
    }
}

/// Decode archived index bytes to entity strings. Out-of-range bytes
/// (only possible in hand-crafted files — pack writes valid indices)
/// are skipped, never panicked on (required on the unchecked
/// zero-copy path).
#[cfg(test)]
mod name_type_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 25);
        for (i, entity) in TABLE.iter().enumerate() {
            let name_type = NameType::from_index(i as u8).expect("dense 0..25");
            assert_eq!(name_type as u8, i as u8);
            assert_eq!(name_type.entity(), *entity);
            assert_eq!(NameType::from_entity(entity), Some(name_type));
        }
        assert_eq!(NameType::from_index(25), None);
    }

    #[test]
    fn fold_decode_roundtrips() {
        let (values, unknown) = NameType::from_strs(["&surname;", "&person;"]);
        assert!(unknown.is_empty());
        let back: Vec<NameType> =
            postcard::from_bytes(&postcard::to_allocvec(&values).expect("encode")).expect("decode");
        assert_eq!(back, values);
    }

    #[test]
    fn unknown_tags_report_without_storing() {
        let (bytes, unknown) = NameType::from_strs(["&place;", "&future-tag;"]);
        assert_eq!(unknown, vec!["&future-tag;"]);
        assert_eq!(bytes, vec![NameType::Place]);
    }
}
