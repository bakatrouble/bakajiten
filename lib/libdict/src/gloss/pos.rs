//! Part-of-speech tags as byte indices: one `u8` per JMdict
//! `<pos>` entity (83 fit easily), stored as a plain `Vec<u8>` per
//! sense. No bitmask, no variable-length codec — the distribution
//! (62.6% empty, 29.7% single) makes the 8-byte empty vec the whole
//! story, and indices stay readable in dumps. Discriminants are
//! declaration order, guaranteed sequential from 0 by the language;
//! `TABLE` below must stay in the same order (pinned by test).

use serde::{Deserialize, Serialize};

/// All `&...;` entities observed in JMdict `<pos>`, discriminant
/// order. Unknown tags at pack time fail loudly (see `from_strs`) —
/// this enum only grows on DTD revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Pos {
    /// `&adj-f;` — noun or verb acting prenominally
    AdjF,
    /// `&adj-i;` — adjective (keiyoushi)
    AdjI,
    /// `&adj-ix;` — adjective (keiyoushi) - yoi/ii class
    AdjIx,
    /// `&adj-ku;` — 'ku' adjective (archaic)
    AdjKu,
    /// `&adj-na;` — adjectival nouns or quasi-adjectives (keiyodoshi)
    AdjNa,
    /// `&adj-nari;` — archaic/formal form of na-adjective
    AdjNari,
    /// `&adj-no;` — nouns which may take the genitive case particle 'no'
    AdjNo,
    /// `&adj-pn;` — pre-noun adjectival (rentaishi)
    AdjPn,
    /// `&adj-shiku;` — 'shiku' adjective (archaic)
    AdjShiku,
    /// `&adj-t;` — 'taru' adjective
    AdjT,
    /// `&adv-to;` — adverb taking the 'to' particle
    AdvTo,
    /// `&adv;` — adverb (fukushi)
    Adv,
    /// `&aux-adj;` — auxiliary adjective
    AuxAdj,
    /// `&aux-v;` — auxiliary verb
    AuxV,
    /// `&aux;` — auxiliary
    Aux,
    /// `&conj;` — conjunction
    Conj,
    /// `&cop;` — copula
    Cop,
    /// `&ctr;` — counter
    Ctr,
    /// `&exp;` — expressions (phrases, clauses, etc.)
    Exp,
    /// `&int;` — interjection (kandoushi)
    Int,
    /// `&n-pref;` — noun, used as a prefix
    NPref,
    /// `&n-suf;` — noun, used as a suffix
    NSuf,
    /// `&n;` — noun (common) (futsuumeishi)
    N,
    /// `&num;` — numeric
    Num,
    /// `&pn;` — pronoun
    Pn,
    /// `&pref;` — prefix
    Pref,
    /// `&prt;` — particle
    Prt,
    /// `&suf;` — suffix
    Suf,
    /// `&unc;` — unclassified
    Unc,
    /// `&v-unspec;` — verb unspecified
    VUnspec,
    /// `&v1-s;` — Ichidan verb - kureru special class
    V1S,
    /// `&v1;` — Ichidan verb
    V1,
    /// `&v2a-s;` — Nidan verb with 'u' ending (archaic)
    V2aS,
    /// `&v2b-k;` — Nidan verb (upper class) with 'bu' ending (archaic)
    V2bK,
    /// `&v2d-s;` — Nidan verb (lower class) with 'dzu' ending (archaic)
    V2dS,
    /// `&v2g-k;` — Nidan verb (upper class) with 'gu' ending (archaic)
    V2gK,
    /// `&v2g-s;` — Nidan verb (lower class) with 'gu' ending (archaic)
    V2gS,
    /// `&v2h-k;` — Nidan verb (upper class) with 'hu/fu' ending (archaic)
    V2hK,
    /// `&v2h-s;` — Nidan verb (lower class) with 'hu/fu' ending (archaic)
    V2hS,
    /// `&v2k-k;` — Nidan verb (upper class) with 'ku' ending (archaic)
    V2kK,
    /// `&v2k-s;` — Nidan verb (lower class) with 'ku' ending (archaic)
    V2kS,
    /// `&v2m-s;` — Nidan verb (lower class) with 'mu' ending (archaic)
    V2mS,
    /// `&v2n-s;` — Nidan verb (lower class) with 'nu' ending (archaic)
    V2nS,
    /// `&v2r-k;` — Nidan verb (upper class) with 'ru' ending (archaic)
    V2rK,
    /// `&v2r-s;` — Nidan verb (lower class) with 'ru' ending (archaic)
    V2rS,
    /// `&v2s-s;` — Nidan verb (lower class) with 'su' ending (archaic)
    V2sS,
    /// `&v2t-k;` — Nidan verb (upper class) with 'tsu' ending (archaic)
    V2tK,
    /// `&v2t-s;` — Nidan verb (lower class) with 'tsu' ending (archaic)
    V2tS,
    /// `&v2w-s;` — Nidan verb (lower class) with 'u' ending and 'we' conjugation (archaic)
    V2wS,
    /// `&v2y-k;` — Nidan verb (upper class) with 'yu' ending (archaic)
    V2yK,
    /// `&v2y-s;` — Nidan verb (lower class) with 'yu' ending (archaic)
    V2yS,
    /// `&v2z-s;` — Nidan verb (lower class) with 'zu' ending (archaic)
    V2zS,
    /// `&v4b;` — Yodan verb with 'bu' ending (archaic)
    V4b,
    /// `&v4g;` — Yodan verb with 'gu' ending (archaic)
    V4g,
    /// `&v4h;` — Yodan verb with 'hu/fu' ending (archaic)
    V4h,
    /// `&v4k;` — Yodan verb with 'ku' ending (archaic)
    V4k,
    /// `&v4m;` — Yodan verb with 'mu' ending (archaic)
    V4m,
    /// `&v4r;` — Yodan verb with 'ru' ending (archaic)
    V4r,
    /// `&v4s;` — Yodan verb with 'su' ending (archaic)
    V4s,
    /// `&v4t;` — Yodan verb with 'tsu' ending (archaic)
    V4t,
    /// `&v5aru;` — Godan verb - -aru special class
    V5aru,
    /// `&v5b;` — Godan verb with 'bu' ending
    V5b,
    /// `&v5g;` — Godan verb with 'gu' ending
    V5g,
    /// `&v5k-s;` — Godan verb - Iku/Yuku special class
    V5kS,
    /// `&v5k;` — Godan verb with 'ku' ending
    V5k,
    /// `&v5m;` — Godan verb with 'mu' ending
    V5m,
    /// `&v5n;` — Godan verb with 'nu' ending
    V5n,
    /// `&v5r-i;` — Godan verb with 'ru' ending (irregular verb)
    V5rI,
    /// `&v5r;` — Godan verb with 'ru' ending
    V5r,
    /// `&v5s;` — Godan verb with 'su' ending
    V5s,
    /// `&v5t;` — Godan verb with 'tsu' ending
    V5t,
    /// `&v5u-s;` — Godan verb with 'u' ending (special class)
    V5uS,
    /// `&v5u;` — Godan verb with 'u' ending
    V5u,
    /// `&vi;` — intransitive verb
    Vi,
    /// `&vk;` — Kuru verb - special class
    Vk,
    /// `&vn;` — irregular nu verb
    Vn,
    /// `&vr;` — irregular ru verb, plain form ends with -ri
    Vr,
    /// `&vs-c;` — su verb - precursor to the modern suru
    VsC,
    /// `&vs-i;` — suru verb - included
    VsI,
    /// `&vs-s;` — suru verb - special class
    VsS,
    /// `&vs;` — noun or participle which takes the aux. verb suru
    Vs,
    /// `&vt;` — transitive verb
    Vt,
    /// `&vz;` — Ichidan verb - zuru verb (alternative form of -jiru verbs)
    Vz,
}

/// Entity per discriminant, in order. Single source for both
/// directions (fold + decode).
const TABLE: [&str; 83] = [
    "&adj-f;",
    "&adj-i;",
    "&adj-ix;",
    "&adj-ku;",
    "&adj-na;",
    "&adj-nari;",
    "&adj-no;",
    "&adj-pn;",
    "&adj-shiku;",
    "&adj-t;",
    "&adv-to;",
    "&adv;",
    "&aux-adj;",
    "&aux-v;",
    "&aux;",
    "&conj;",
    "&cop;",
    "&ctr;",
    "&exp;",
    "&int;",
    "&n-pref;",
    "&n-suf;",
    "&n;",
    "&num;",
    "&pn;",
    "&pref;",
    "&prt;",
    "&suf;",
    "&unc;",
    "&v-unspec;",
    "&v1-s;",
    "&v1;",
    "&v2a-s;",
    "&v2b-k;",
    "&v2d-s;",
    "&v2g-k;",
    "&v2g-s;",
    "&v2h-k;",
    "&v2h-s;",
    "&v2k-k;",
    "&v2k-s;",
    "&v2m-s;",
    "&v2n-s;",
    "&v2r-k;",
    "&v2r-s;",
    "&v2s-s;",
    "&v2t-k;",
    "&v2t-s;",
    "&v2w-s;",
    "&v2y-k;",
    "&v2y-s;",
    "&v2z-s;",
    "&v4b;",
    "&v4g;",
    "&v4h;",
    "&v4k;",
    "&v4m;",
    "&v4r;",
    "&v4s;",
    "&v4t;",
    "&v5aru;",
    "&v5b;",
    "&v5g;",
    "&v5k-s;",
    "&v5k;",
    "&v5m;",
    "&v5n;",
    "&v5r-i;",
    "&v5r;",
    "&v5s;",
    "&v5t;",
    "&v5u-s;",
    "&v5u;",
    "&vi;",
    "&vk;",
    "&vn;",
    "&vr;",
    "&vs-c;",
    "&vs-i;",
    "&vs-s;",
    "&vs;",
    "&vt;",
    "&vz;",
];

impl Pos {
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
            0 => Self::AdjF,
            1 => Self::AdjI,
            2 => Self::AdjIx,
            3 => Self::AdjKu,
            4 => Self::AdjNa,
            5 => Self::AdjNari,
            6 => Self::AdjNo,
            7 => Self::AdjPn,
            8 => Self::AdjShiku,
            9 => Self::AdjT,
            10 => Self::AdvTo,
            11 => Self::Adv,
            12 => Self::AuxAdj,
            13 => Self::AuxV,
            14 => Self::Aux,
            15 => Self::Conj,
            16 => Self::Cop,
            17 => Self::Ctr,
            18 => Self::Exp,
            19 => Self::Int,
            20 => Self::NPref,
            21 => Self::NSuf,
            22 => Self::N,
            23 => Self::Num,
            24 => Self::Pn,
            25 => Self::Pref,
            26 => Self::Prt,
            27 => Self::Suf,
            28 => Self::Unc,
            29 => Self::VUnspec,
            30 => Self::V1S,
            31 => Self::V1,
            32 => Self::V2aS,
            33 => Self::V2bK,
            34 => Self::V2dS,
            35 => Self::V2gK,
            36 => Self::V2gS,
            37 => Self::V2hK,
            38 => Self::V2hS,
            39 => Self::V2kK,
            40 => Self::V2kS,
            41 => Self::V2mS,
            42 => Self::V2nS,
            43 => Self::V2rK,
            44 => Self::V2rS,
            45 => Self::V2sS,
            46 => Self::V2tK,
            47 => Self::V2tS,
            48 => Self::V2wS,
            49 => Self::V2yK,
            50 => Self::V2yS,
            51 => Self::V2zS,
            52 => Self::V4b,
            53 => Self::V4g,
            54 => Self::V4h,
            55 => Self::V4k,
            56 => Self::V4m,
            57 => Self::V4r,
            58 => Self::V4s,
            59 => Self::V4t,
            60 => Self::V5aru,
            61 => Self::V5b,
            62 => Self::V5g,
            63 => Self::V5kS,
            64 => Self::V5k,
            65 => Self::V5m,
            66 => Self::V5n,
            67 => Self::V5rI,
            68 => Self::V5r,
            69 => Self::V5s,
            70 => Self::V5t,
            71 => Self::V5uS,
            72 => Self::V5u,
            73 => Self::Vi,
            74 => Self::Vk,
            75 => Self::Vn,
            76 => Self::Vr,
            77 => Self::VsC,
            78 => Self::VsI,
            79 => Self::VsS,
            80 => Self::Vs,
            81 => Self::Vt,
            82 => Self::Vz,
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
            Self::AdjF => "noun or verb acting prenominally",
            Self::AdjI => "adjective (keiyoushi)",
            Self::AdjIx => "adjective (keiyoushi) - yoi/ii class",
            Self::AdjKu => "'ku' adjective (archaic)",
            Self::AdjNa => "adjectival nouns or quasi-adjectives (keiyodoshi)",
            Self::AdjNari => "archaic/formal form of na-adjective",
            Self::AdjNo => "nouns which may take the genitive case particle 'no'",
            Self::AdjPn => "pre-noun adjectival (rentaishi)",
            Self::AdjShiku => "'shiku' adjective (archaic)",
            Self::AdjT => "'taru' adjective",
            Self::AdvTo => "adverb taking the 'to' particle",
            Self::Adv => "adverb (fukushi)",
            Self::AuxAdj => "auxiliary adjective",
            Self::AuxV => "auxiliary verb",
            Self::Aux => "auxiliary",
            Self::Conj => "conjunction",
            Self::Cop => "copula",
            Self::Ctr => "counter",
            Self::Exp => "expressions (phrases, clauses, etc.)",
            Self::Int => "interjection (kandoushi)",
            Self::NPref => "noun, used as a prefix",
            Self::NSuf => "noun, used as a suffix",
            Self::N => "noun (common) (futsuumeishi)",
            Self::Num => "numeric",
            Self::Pn => "pronoun",
            Self::Pref => "prefix",
            Self::Prt => "particle",
            Self::Suf => "suffix",
            Self::Unc => "unclassified",
            Self::VUnspec => "verb unspecified",
            Self::V1S => "Ichidan verb - kureru special class",
            Self::V1 => "Ichidan verb",
            Self::V2aS => "Nidan verb with 'u' ending (archaic)",
            Self::V2bK => "Nidan verb (upper class) with 'bu' ending (archaic)",
            Self::V2dS => "Nidan verb (lower class) with 'dzu' ending (archaic)",
            Self::V2gK => "Nidan verb (upper class) with 'gu' ending (archaic)",
            Self::V2gS => "Nidan verb (lower class) with 'gu' ending (archaic)",
            Self::V2hK => "Nidan verb (upper class) with 'hu/fu' ending (archaic)",
            Self::V2hS => "Nidan verb (lower class) with 'hu/fu' ending (archaic)",
            Self::V2kK => "Nidan verb (upper class) with 'ku' ending (archaic)",
            Self::V2kS => "Nidan verb (lower class) with 'ku' ending (archaic)",
            Self::V2mS => "Nidan verb (lower class) with 'mu' ending (archaic)",
            Self::V2nS => "Nidan verb (lower class) with 'nu' ending (archaic)",
            Self::V2rK => "Nidan verb (upper class) with 'ru' ending (archaic)",
            Self::V2rS => "Nidan verb (lower class) with 'ru' ending (archaic)",
            Self::V2sS => "Nidan verb (lower class) with 'su' ending (archaic)",
            Self::V2tK => "Nidan verb (upper class) with 'tsu' ending (archaic)",
            Self::V2tS => "Nidan verb (lower class) with 'tsu' ending (archaic)",
            Self::V2wS => "Nidan verb (lower class) with 'u' ending and 'we' conjugation (archaic)",
            Self::V2yK => "Nidan verb (upper class) with 'yu' ending (archaic)",
            Self::V2yS => "Nidan verb (lower class) with 'yu' ending (archaic)",
            Self::V2zS => "Nidan verb (lower class) with 'zu' ending (archaic)",
            Self::V4b => "Yodan verb with 'bu' ending (archaic)",
            Self::V4g => "Yodan verb with 'gu' ending (archaic)",
            Self::V4h => "Yodan verb with 'hu/fu' ending (archaic)",
            Self::V4k => "Yodan verb with 'ku' ending (archaic)",
            Self::V4m => "Yodan verb with 'mu' ending (archaic)",
            Self::V4r => "Yodan verb with 'ru' ending (archaic)",
            Self::V4s => "Yodan verb with 'su' ending (archaic)",
            Self::V4t => "Yodan verb with 'tsu' ending (archaic)",
            Self::V5aru => "Godan verb - -aru special class",
            Self::V5b => "Godan verb with 'bu' ending",
            Self::V5g => "Godan verb with 'gu' ending",
            Self::V5kS => "Godan verb - Iku/Yuku special class",
            Self::V5k => "Godan verb with 'ku' ending",
            Self::V5m => "Godan verb with 'mu' ending",
            Self::V5n => "Godan verb with 'nu' ending",
            Self::V5rI => "Godan verb with 'ru' ending (irregular verb)",
            Self::V5r => "Godan verb with 'ru' ending",
            Self::V5s => "Godan verb with 'su' ending",
            Self::V5t => "Godan verb with 'tsu' ending",
            Self::V5uS => "Godan verb with 'u' ending (special class)",
            Self::V5u => "Godan verb with 'u' ending",
            Self::Vi => "intransitive verb",
            Self::Vk => "Kuru verb - special class",
            Self::Vn => "irregular nu verb",
            Self::Vr => "irregular ru verb, plain form ends with -ri",
            Self::VsC => "su verb - precursor to the modern suru",
            Self::VsI => "suru verb - included",
            Self::VsS => "suru verb - special class",
            Self::Vs => "noun or participle which takes the aux. verb suru",
            Self::Vt => "transitive verb",
            Self::Vz => "Ichidan verb - zuru verb (alternative form of -jiru verbs)",
        }
    }

    /// Fold raw entity strings into index bytes. Takes an iterator
    /// so callers never allocate a `Vec<String>` just to convert
    /// (borrow parser output or string literals directly). Returns
    /// the bytes plus the unknown tags (borrowed — pack fails loudly
    /// on these).
    pub fn from_strs<'a>(tags: impl IntoIterator<Item = &'a str>) -> (Vec<Self>, Vec<&'a str>) {
        let mut out = Vec::new();
        let mut unknown = Vec::new();
        for tag in tags {
            match Self::from_entity(tag) {
                Some(pos) => out.push(pos),
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
mod pos_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 83);
        for (i, entity) in TABLE.iter().enumerate() {
            let pos = Pos::from_index(i as u8).expect("dense 0..83");
            assert_eq!(pos as u8, i as u8);
            assert_eq!(pos.entity(), *entity);
            assert_eq!(Pos::from_entity(entity), Some(pos));
        }
        assert_eq!(Pos::from_index(83), None);
    }

    #[test]
    fn fold_decode_roundtrips() {
        let (values, unknown) = Pos::from_strs(["&n;", "&v5r;"]);
        assert!(unknown.is_empty());
        let back: Vec<Pos> =
            postcard::from_bytes(&postcard::to_allocvec(&values).expect("encode")).expect("decode");
        assert_eq!(back, values);
    }

    #[test]
    fn unknown_tags_report_without_storing() {
        let (bytes, unknown) = Pos::from_strs(["&n;", "&future-tag;"]);
        assert_eq!(unknown, vec!["&future-tag;"]);
        assert_eq!(bytes, vec![Pos::N]);
    }
}
