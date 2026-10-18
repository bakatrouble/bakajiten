//! Field-of-application tags as byte indices: one `u8` per JMdict `<field>` entity, stored as a plain `Vec<u8>` per sense. Same treatment as `pos` (`Pos`).

use serde::{Deserialize, Serialize};

/// All `&...;` entities observed in JMdict `<field>` (96 total).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Field {
    /// `&Buddh;` — Buddhism
    Buddh,
    /// `&Christn;` — Christianity
    Christn,
    /// `&MA;` — martial arts
    MA,
    /// `&Shinto;` — Shinto
    Shinto,
    /// `&agric;` — agriculture
    Agric,
    /// `&anat;` — anatomy
    Anat,
    /// `&archeol;` — archeology
    Archeol,
    /// `&archit;` — architecture
    Archit,
    /// `&art;` — art, aesthetics
    Art,
    /// `&astron;` — astronomy
    Astron,
    /// `&audvid;` — audiovisual
    Audvid,
    /// `&aviat;` — aviation
    Aviat,
    /// `&baseb;` — baseball
    Baseb,
    /// `&biochem;` — biochemistry
    Biochem,
    /// `&biol;` — biology
    Biol,
    /// `&bot;` — botany
    Bot,
    /// `&boxing;` — boxing
    Boxing,
    /// `&bus;` — business
    Bus,
    /// `&cards;` — card games
    Cards,
    /// `&chem;` — chemistry
    Chem,
    /// `&chmyth;` — Chinese mythology
    Chmyth,
    /// `&civeng;` — civil engineering
    Civeng,
    /// `&cloth;` — clothing
    Cloth,
    /// `&comp;` — computing
    Comp,
    /// `&cryst;` — crystallography
    Cryst,
    /// `&dent;` — dentistry
    Dent,
    /// `&ecol;` — ecology
    Ecol,
    /// `&econ;` — economics
    Econ,
    /// `&elec;` — electricity, elec. eng.
    Elec,
    /// `&electr;` — electronics
    Electr,
    /// `&embryo;` — embryology
    Embryo,
    /// `&engr;` — engineering
    Engr,
    /// `&ent;` — entomology
    Ent,
    /// `&figskt;` — figure skating
    Figskt,
    /// `&film;` — film
    Film,
    /// `&finc;` — finance
    Finc,
    /// `&fish;` — fishing
    Fish,
    /// `&food;` — food, cooking
    Food,
    /// `&gardn;` — gardening, horticulture
    Gardn,
    /// `&genet;` — genetics
    Genet,
    /// `&geogr;` — geography
    Geogr,
    /// `&geol;` — geology
    Geol,
    /// `&geom;` — geometry
    Geom,
    /// `&go;` — go (game)
    Go,
    /// `&golf;` — golf
    Golf,
    /// `&gramm;` — grammar
    Gramm,
    /// `&grmyth;` — Greek mythology
    Grmyth,
    /// `&hanaf;` — hanafuda
    Hanaf,
    /// `&horse;` — horse racing
    Horse,
    /// `&internet;` — Internet
    Internet,
    /// `&jpmyth;` — Japanese mythology
    Jpmyth,
    /// `&kabuki;` — kabuki
    Kabuki,
    /// `&law;` — law
    Law,
    /// `&ling;` — linguistics
    Ling,
    /// `&logic;` — logic
    Logic,
    /// `&mahj;` — mahjong
    Mahj,
    /// `&manga;` — manga
    Manga,
    /// `&math;` — mathematics
    Math,
    /// `&mech;` — mechanical engineering
    Mech,
    /// `&med;` — medicine
    Med,
    /// `&met;` — meteorology
    Met,
    /// `&mil;` — military
    Mil,
    /// `&min;` — mineralogy
    Min,
    /// `&mining;` — mining
    Mining,
    /// `&motor;` — motorsport
    Motor,
    /// `&music;` — music
    Music,
    /// `&noh;` — noh
    Noh,
    /// `&ornith;` — ornithology
    Ornith,
    /// `&paleo;` — paleontology
    Paleo,
    /// `&pathol;` — pathology
    Pathol,
    /// `&pharm;` — pharmacology
    Pharm,
    /// `&phil;` — philosophy
    Phil,
    /// `&photo;` — photography
    Photo,
    /// `&physics;` — physics
    Physics,
    /// `&physiol;` — physiology
    Physiol,
    /// `&politics;` — politics
    Politics,
    /// `&print;` — printing
    Print,
    /// `&prowres;` — professional wrestling
    Prowres,
    /// `&psy;` — psychiatry
    Psy,
    /// `&psyanal;` — psychoanalysis
    Psyanal,
    /// `&psych;` — psychology
    Psych,
    /// `&rail;` — railway
    Rail,
    /// `&rommyth;` — Roman mythology
    Rommyth,
    /// `&shogi;` — shogi
    Shogi,
    /// `&ski;` — skiing
    Ski,
    /// `&sports;` — sports
    Sports,
    /// `&stat;` — statistics
    Stat,
    /// `&stockm;` — stock market
    Stockm,
    /// `&sumo;` — sumo
    Sumo,
    /// `&surg;` — surgery
    Surg,
    /// `&telec;` — telecommunications
    Telec,
    /// `&tradem;` — trademark
    Tradem,
    /// `&tv;` — television
    Tv,
    /// `&vet;` — veterinary terms
    Vet,
    /// `&vidg;` — video games
    Vidg,
    /// `&zool;` — zoology
    Zool,
}

/// Entity per discriminant, in order. Single source for both
/// directions (fold + decode).
const TABLE: [&str; 96] = [
    "&Buddh;",
    "&Christn;",
    "&MA;",
    "&Shinto;",
    "&agric;",
    "&anat;",
    "&archeol;",
    "&archit;",
    "&art;",
    "&astron;",
    "&audvid;",
    "&aviat;",
    "&baseb;",
    "&biochem;",
    "&biol;",
    "&bot;",
    "&boxing;",
    "&bus;",
    "&cards;",
    "&chem;",
    "&chmyth;",
    "&civeng;",
    "&cloth;",
    "&comp;",
    "&cryst;",
    "&dent;",
    "&ecol;",
    "&econ;",
    "&elec;",
    "&electr;",
    "&embryo;",
    "&engr;",
    "&ent;",
    "&figskt;",
    "&film;",
    "&finc;",
    "&fish;",
    "&food;",
    "&gardn;",
    "&genet;",
    "&geogr;",
    "&geol;",
    "&geom;",
    "&go;",
    "&golf;",
    "&gramm;",
    "&grmyth;",
    "&hanaf;",
    "&horse;",
    "&internet;",
    "&jpmyth;",
    "&kabuki;",
    "&law;",
    "&ling;",
    "&logic;",
    "&mahj;",
    "&manga;",
    "&math;",
    "&mech;",
    "&med;",
    "&met;",
    "&mil;",
    "&min;",
    "&mining;",
    "&motor;",
    "&music;",
    "&noh;",
    "&ornith;",
    "&paleo;",
    "&pathol;",
    "&pharm;",
    "&phil;",
    "&photo;",
    "&physics;",
    "&physiol;",
    "&politics;",
    "&print;",
    "&prowres;",
    "&psy;",
    "&psyanal;",
    "&psych;",
    "&rail;",
    "&rommyth;",
    "&shogi;",
    "&ski;",
    "&sports;",
    "&stat;",
    "&stockm;",
    "&sumo;",
    "&surg;",
    "&telec;",
    "&tradem;",
    "&tv;",
    "&vet;",
    "&vidg;",
    "&zool;",
];

impl Field {
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
            0 => Self::Buddh,
            1 => Self::Christn,
            2 => Self::MA,
            3 => Self::Shinto,
            4 => Self::Agric,
            5 => Self::Anat,
            6 => Self::Archeol,
            7 => Self::Archit,
            8 => Self::Art,
            9 => Self::Astron,
            10 => Self::Audvid,
            11 => Self::Aviat,
            12 => Self::Baseb,
            13 => Self::Biochem,
            14 => Self::Biol,
            15 => Self::Bot,
            16 => Self::Boxing,
            17 => Self::Bus,
            18 => Self::Cards,
            19 => Self::Chem,
            20 => Self::Chmyth,
            21 => Self::Civeng,
            22 => Self::Cloth,
            23 => Self::Comp,
            24 => Self::Cryst,
            25 => Self::Dent,
            26 => Self::Ecol,
            27 => Self::Econ,
            28 => Self::Elec,
            29 => Self::Electr,
            30 => Self::Embryo,
            31 => Self::Engr,
            32 => Self::Ent,
            33 => Self::Figskt,
            34 => Self::Film,
            35 => Self::Finc,
            36 => Self::Fish,
            37 => Self::Food,
            38 => Self::Gardn,
            39 => Self::Genet,
            40 => Self::Geogr,
            41 => Self::Geol,
            42 => Self::Geom,
            43 => Self::Go,
            44 => Self::Golf,
            45 => Self::Gramm,
            46 => Self::Grmyth,
            47 => Self::Hanaf,
            48 => Self::Horse,
            49 => Self::Internet,
            50 => Self::Jpmyth,
            51 => Self::Kabuki,
            52 => Self::Law,
            53 => Self::Ling,
            54 => Self::Logic,
            55 => Self::Mahj,
            56 => Self::Manga,
            57 => Self::Math,
            58 => Self::Mech,
            59 => Self::Med,
            60 => Self::Met,
            61 => Self::Mil,
            62 => Self::Min,
            63 => Self::Mining,
            64 => Self::Motor,
            65 => Self::Music,
            66 => Self::Noh,
            67 => Self::Ornith,
            68 => Self::Paleo,
            69 => Self::Pathol,
            70 => Self::Pharm,
            71 => Self::Phil,
            72 => Self::Photo,
            73 => Self::Physics,
            74 => Self::Physiol,
            75 => Self::Politics,
            76 => Self::Print,
            77 => Self::Prowres,
            78 => Self::Psy,
            79 => Self::Psyanal,
            80 => Self::Psych,
            81 => Self::Rail,
            82 => Self::Rommyth,
            83 => Self::Shogi,
            84 => Self::Ski,
            85 => Self::Sports,
            86 => Self::Stat,
            87 => Self::Stockm,
            88 => Self::Sumo,
            89 => Self::Surg,
            90 => Self::Telec,
            91 => Self::Tradem,
            92 => Self::Tv,
            93 => Self::Vet,
            94 => Self::Vidg,
            95 => Self::Zool,
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
            Self::Buddh => "Buddhism",
            Self::Christn => "Christianity",
            Self::MA => "martial arts",
            Self::Shinto => "Shinto",
            Self::Agric => "agriculture",
            Self::Anat => "anatomy",
            Self::Archeol => "archeology",
            Self::Archit => "architecture",
            Self::Art => "art, aesthetics",
            Self::Astron => "astronomy",
            Self::Audvid => "audiovisual",
            Self::Aviat => "aviation",
            Self::Baseb => "baseball",
            Self::Biochem => "biochemistry",
            Self::Biol => "biology",
            Self::Bot => "botany",
            Self::Boxing => "boxing",
            Self::Bus => "business",
            Self::Cards => "card games",
            Self::Chem => "chemistry",
            Self::Chmyth => "Chinese mythology",
            Self::Civeng => "civil engineering",
            Self::Cloth => "clothing",
            Self::Comp => "computing",
            Self::Cryst => "crystallography",
            Self::Dent => "dentistry",
            Self::Ecol => "ecology",
            Self::Econ => "economics",
            Self::Elec => "electricity, elec. eng.",
            Self::Electr => "electronics",
            Self::Embryo => "embryology",
            Self::Engr => "engineering",
            Self::Ent => "entomology",
            Self::Figskt => "figure skating",
            Self::Film => "film",
            Self::Finc => "finance",
            Self::Fish => "fishing",
            Self::Food => "food, cooking",
            Self::Gardn => "gardening, horticulture",
            Self::Genet => "genetics",
            Self::Geogr => "geography",
            Self::Geol => "geology",
            Self::Geom => "geometry",
            Self::Go => "go (game)",
            Self::Golf => "golf",
            Self::Gramm => "grammar",
            Self::Grmyth => "Greek mythology",
            Self::Hanaf => "hanafuda",
            Self::Horse => "horse racing",
            Self::Internet => "Internet",
            Self::Jpmyth => "Japanese mythology",
            Self::Kabuki => "kabuki",
            Self::Law => "law",
            Self::Ling => "linguistics",
            Self::Logic => "logic",
            Self::Mahj => "mahjong",
            Self::Manga => "manga",
            Self::Math => "mathematics",
            Self::Mech => "mechanical engineering",
            Self::Med => "medicine",
            Self::Met => "meteorology",
            Self::Mil => "military",
            Self::Min => "mineralogy",
            Self::Mining => "mining",
            Self::Motor => "motorsport",
            Self::Music => "music",
            Self::Noh => "noh",
            Self::Ornith => "ornithology",
            Self::Paleo => "paleontology",
            Self::Pathol => "pathology",
            Self::Pharm => "pharmacology",
            Self::Phil => "philosophy",
            Self::Photo => "photography",
            Self::Physics => "physics",
            Self::Physiol => "physiology",
            Self::Politics => "politics",
            Self::Print => "printing",
            Self::Prowres => "professional wrestling",
            Self::Psy => "psychiatry",
            Self::Psyanal => "psychoanalysis",
            Self::Psych => "psychology",
            Self::Rail => "railway",
            Self::Rommyth => "Roman mythology",
            Self::Shogi => "shogi",
            Self::Ski => "skiing",
            Self::Sports => "sports",
            Self::Stat => "statistics",
            Self::Stockm => "stock market",
            Self::Sumo => "sumo",
            Self::Surg => "surgery",
            Self::Telec => "telecommunications",
            Self::Tradem => "trademark",
            Self::Tv => "television",
            Self::Vet => "veterinary terms",
            Self::Vidg => "video games",
            Self::Zool => "zoology",
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
mod field_tests {
    use super::*;

    #[test]
    fn table_matches_discriminants_both_ways() {
        assert_eq!(TABLE.len(), 96);
        for (i, entity) in TABLE.iter().enumerate() {
            let v = Field::from_index(i as u8).expect("dense discriminants");
            assert_eq!(v as u8, i as u8);
            assert_eq!(v.entity(), *entity);
            assert_eq!(Field::from_entity(entity), Some(v));
        }
        assert_eq!(Field::from_index(96), None);
    }

    #[test]
    fn fold_decode_roundtrips() {
        let (values, unknown) = Field::from_strs(["&Buddh;", "&Christn;"]);
        assert!(unknown.is_empty());
        let back: Vec<Field> =
            postcard::from_bytes(&postcard::to_allocvec(&values).expect("encode")).expect("decode");
        assert_eq!(back, values);
    }

    #[test]
    fn unknown_tags_report_without_storing() {
        let (bytes, unknown) = Field::from_strs(["&Buddh;", "&future-tag;"]);
        assert_eq!(unknown, vec!["&future-tag;"]);
        assert_eq!(bytes.len(), 1);
    }
}
