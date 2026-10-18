//! Miscellaneous-info tags as byte indices: one `u8` per JMdict `<misc>` entity, stored as a plain `Vec<u8>` per sense. Same treatment as `pos` (`Pos`).

use serde::{Deserialize, Serialize};

/// All `&...;` entities observed in JMdict `<misc>` (52 total).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Misc {
    /// `&abbr;` — abbreviation
    Abbr,
    /// `&arch;` — archaic
    Arch,
    /// `&char;` — character
    Char,
    /// `&chn;` — children's language
    Chn,
    /// `&col;` — colloquial
    Col,
    /// `&company;` — company name
    Company,
    /// `&creat;` — creature
    Creat,
    /// `&dated;` — dated term
    Dated,
    /// `&dei;` — deity
    Dei,
    /// `&derog;` — derogatory
    Derog,
    /// `&doc;` — document
    Doc,
    /// `&euph;` — euphemistic
    Euph,
    /// `&ev;` — event
    Ev,
    /// `&fam;` — familiar language
    Fam,
    /// `&fem;` — female term or language
    Fem,
    /// `&fict;` — fiction
    Fict,
    /// `&form;` — formal or literary term
    Form,
    /// `&given;` — given name or forename, gender not specified
    Given,
    /// `&group;` — group
    Group,
    /// `&hist;` — historical term
    Hist,
    /// `&hon;` — honorific or respectful (sonkeigo) language
    Hon,
    /// `&hum;` — humble (kenjougo) language
    Hum,
    /// `&id;` — idiomatic expression
    Id,
    /// `&joc;` — jocular, humorous term
    Joc,
    /// `&leg;` — legend
    Leg,
    /// `&m-sl;` — manga slang
    MSl,
    /// `&male;` — male term or language
    Male,
    /// `&myth;` — mythology
    Myth,
    /// `&net-sl;` — Internet slang
    NetSl,
    /// `&obj;` — object
    Obj,
    /// `&obs;` — obsolete term
    Obs,
    /// `&on-mim;` — onomatopoeic or mimetic word
    OnMim,
    /// `&organization;` — organization name
    Organization,
    /// `&person;` — full name of a particular person
    Person,
    /// `&place;` — place name
    Place,
    /// `&poet;` — poetical term
    Poet,
    /// `&pol;` — polite (teineigo) language
    Pol,
    /// `&product;` — product name
    Product,
    /// `&proverb;` — proverb
    Proverb,
    /// `&quote;` — quotation
    Quote,
    /// `&rare;` — rare term
    Rare,
    /// `&relig;` — religion
    Relig,
    /// `&sens;` — sensitive
    Sens,
    /// `&serv;` — service
    Serv,
    /// `&ship;` — ship name
    Ship,
    /// `&sl;` — slang
    Sl,
    /// `&surname;` — family or surname
    Surname,
    /// `&uk;` — word usually written using kana alone
    Uk,
    /// `&unclass;` — unclassified name
    Unclass,
    /// `&vulg;` — vulgar expression or word
    Vulg,
    /// `&work;` — work of art, literature, music, etc. name
    Work,
    /// `&yoji;` — yojijukugo
    Yoji,
}

/// Entity per discriminant, in order. Single source for both
/// directions (fold + decode).
const TABLE: [&str; 52] = [
    "&abbr;",
    "&arch;",
    "&char;",
    "&chn;",
    "&col;",
    "&company;",
    "&creat;",
    "&dated;",
    "&dei;",
    "&derog;",
    "&doc;",
    "&euph;",
    "&ev;",
    "&fam;",
    "&fem;",
    "&fict;",
    "&form;",
    "&given;",
    "&group;",
    "&hist;",
    "&hon;",
    "&hum;",
    "&id;",
    "&joc;",
    "&leg;",
    "&m-sl;",
    "&male;",
    "&myth;",
    "&net-sl;",
    "&obj;",
    "&obs;",
    "&on-mim;",
    "&organization;",
    "&person;",
    "&place;",
    "&poet;",
    "&pol;",
    "&product;",
    "&proverb;",
    "&quote;",
    "&rare;",
    "&relig;",
    "&sens;",
    "&serv;",
    "&ship;",
    "&sl;",
    "&surname;",
    "&uk;",
    "&unclass;",
    "&vulg;",
    "&work;",
    "&yoji;",
];

impl Misc {
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
            0 => Self::Abbr,
            1 => Self::Arch,
            2 => Self::Char,
            3 => Self::Chn,
            4 => Self::Col,
            5 => Self::Company,
            6 => Self::Creat,
            7 => Self::Dated,
            8 => Self::Dei,
            9 => Self::Derog,
            10 => Self::Doc,
            11 => Self::Euph,
            12 => Self::Ev,
            13 => Self::Fam,
            14 => Self::Fem,
            15 => Self::Fict,
            16 => Self::Form,
            17 => Self::Given,
            18 => Self::Group,
            19 => Self::Hist,
            20 => Self::Hon,
            21 => Self::Hum,
            22 => Self::Id,
            23 => Self::Joc,
            24 => Self::Leg,
            25 => Self::MSl,
            26 => Self::Male,
            27 => Self::Myth,
            28 => Self::NetSl,
            29 => Self::Obj,
            30 => Self::Obs,
            31 => Self::OnMim,
            32 => Self::Organization,
            33 => Self::Person,
            34 => Self::Place,
            35 => Self::Poet,
            36 => Self::Pol,
            37 => Self::Product,
            38 => Self::Proverb,
            39 => Self::Quote,
            40 => Self::Rare,
            41 => Self::Relig,
            42 => Self::Sens,
            43 => Self::Serv,
            44 => Self::Ship,
            45 => Self::Sl,
            46 => Self::Surname,
            47 => Self::Uk,
            48 => Self::Unclass,
            49 => Self::Vulg,
            50 => Self::Work,
            51 => Self::Yoji,
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
            Self::Abbr => "abbreviation",
            Self::Arch => "archaic",
            Self::Char => "character",
            Self::Chn => "children's language",
            Self::Col => "colloquial",
            Self::Company => "company name",
            Self::Creat => "creature",
            Self::Dated => "dated term",
            Self::Dei => "deity",
            Self::Derog => "derogatory",
            Self::Doc => "document",
            Self::Euph => "euphemistic",
            Self::Ev => "event",
            Self::Fam => "familiar language",
            Self::Fem => "female term or language",
            Self::Fict => "fiction",
            Self::Form => "formal or literary term",
            Self::Given => "given name or forename, gender not specified",
            Self::Group => "group",
            Self::Hist => "historical term",
            Self::Hon => "honorific or respectful (sonkeigo) language",
            Self::Hum => "humble (kenjougo) language",
            Self::Id => "idiomatic expression",
            Self::Joc => "jocular, humorous term",
            Self::Leg => "legend",
            Self::MSl => "manga slang",
            Self::Male => "male term or language",
            Self::Myth => "mythology",
            Self::NetSl => "Internet slang",
            Self::Obj => "object",
            Self::Obs => "obsolete term",
            Self::OnMim => "onomatopoeic or mimetic word",
            Self::Organization => "organization name",
            Self::Person => "full name of a particular person",
            Self::Place => "place name",
            Self::Poet => "poetical term",
            Self::Pol => "polite (teineigo) language",
            Self::Product => "product name",
            Self::Proverb => "proverb",
            Self::Quote => "quotation",
            Self::Rare => "rare term",
            Self::Relig => "religion",
            Self::Sens => "sensitive",
            Self::Serv => "service",
            Self::Ship => "ship name",
            Self::Sl => "slang",
            Self::Surname => "family or surname",
            Self::Uk => "word usually written using kana alone",
            Self::Unclass => "unclassified name",
            Self::Vulg => "vulgar expression or word",
            Self::Work => "work of art, literature, music, etc. name",
            Self::Yoji => "yojijukugo",
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
                Some(v) => out.push(v),
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
mod misc_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 52);
        for (i, entity) in TABLE.iter().enumerate() {
            let v = Misc::from_index(i as u8).expect("dense discriminants");
            assert_eq!(v as u8, i as u8);
            assert_eq!(v.entity(), *entity);
            assert_eq!(Misc::from_entity(entity), Some(v));
        }
        assert_eq!(Misc::from_index(52), None);
    }

    #[test]
    fn fold_decode_roundtrips() {
        let (values, unknown) = Misc::from_strs(["&abbr;", "&arch;"]);
        assert!(unknown.is_empty());
        let back: Vec<Misc> =
            postcard::from_bytes(&postcard::to_allocvec(&values).expect("encode")).expect("decode");
        assert_eq!(back, values);
    }

    #[test]
    fn unknown_tags_report_without_storing() {
        let (bytes, unknown) = Misc::from_strs(["&abbr;", "&future-tag;"]);
        assert_eq!(unknown, vec!["&future-tag;"]);
        assert_eq!(bytes.len(), 1);
    }
}
