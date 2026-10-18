use serde::{Deserialize, Serialize};

use super::{
    field::Field, gloss_type::GlossType, inf::Inf, misc::Misc, name_type::NameType, pos::Pos,
    priority::Priority,
};

/// zkanji frequency points per priority code (import.cpp
/// addKPri/addRPri — readings score half): news1/spec1 = 1000,
/// news2/spec2 = 500, ichi1 = 2000, ichi2 = 1000, nfXX =
/// (50-XX)*25, everything else 0. Unknown codes never reach here
/// (pack fails loudly on them).
fn pri_points(code: &str, reading: bool) -> i32 {
    let (news1, news2) = if reading { (500, 250) } else { (1000, 500) };
    match code {
        "news1" | "spec1" => news1,
        "news2" | "spec2" => news2,
        "ichi1" => 2000,
        "ichi2" => 1000,
        _ if code.len() == 4 && code.starts_with("nf") => {
            code[2..].parse::<i32>().map(|v| (50 - v) * 25).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Per-record frequency for the relevance sort: zkanji points
/// summed over every kanji and reading form, saturated to u16.
pub fn record_freq(kanji: &[KanjiElementRecord], readings: &[ReadingElementRecord]) -> u16 {
    let mut freq: i32 = 0;
    for k in kanji {
        for p in &k.priorities {
            freq += pri_points(p.entity(), false);
        }
    }
    for r in readings {
        for p in &r.priorities {
            freq += pri_points(p.entity(), true);
        }
    }
    freq.clamp(0, u16::MAX as i32) as u16
}

/// One stored string: byte offset/length into the shared blob.
/// The single reference shape for every string in the artifact
/// (pool entries, gram keys, entry texts alike). Offsets stay byte
/// indices (UTF-8 order is byte order); every interned string is
/// whole, so slices always land on char boundaries.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct StrRef {
    pub offset: u32,
    pub len: u32,
}

impl StrRef {
    /// Blob text for this ref (pack guarantees char boundaries).
    pub fn resolve<'b>(&self, blob: &'b [u8]) -> &'b str {
        let (o, l) = (self.offset as usize, self.len as usize);
        std::str::from_utf8(&blob[o..o + l]).expect("blob holds whole strings")
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub struct IndexRecord {
    /// First 8 bytes of the normalized key, zero-padded. Stored as
    /// bytes (not `u64`) so the archived struct needs no 8-alignment
    /// padding: 12 bytes, not 16. Byte order is big-endian by
    /// construction, so lexicographic array order is key order.
    /// Lossy prefix by design — exact matches verify against the
    /// real strings.
    pub key: [u8; 8],
    pub record_idx: u32,
}

/// One dictionary entry, words and names alike (files stay separate —
/// `GLD1` vs `GLN1` — but the schema is shared). Headword forms live
/// in the file pool; these are indices into it. Stored postcard-
/// encoded in the entries blob (see `file`); only the index section
/// is rkyv.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryRecord {
    pub ent_seq: u32,
    /// JLPT level 1–5 (N1–N5, tanos snapshot), 0 when unknown.
    pub jlpt: u8,
    pub kanji: Vec<KanjiElementRecord>,
    pub readings: Vec<ReadingElementRecord>,
    pub senses: Vec<SenseRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KanjiElementRecord {
    /// Kanji form as a file-pool index.
    pub keb: u32,
    /// `ke_inf` codes, stored directly (see [`Inf`]).
    pub info: Vec<Inf>,
    /// `ke_pri` codes, stored directly (see [`Priority`]).
    pub priorities: Vec<Priority>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadingElementRecord {
    /// Reading as a file-pool index.
    pub reb: u32,
    pub nokanji: bool,
    pub restricted_to: Vec<StrRef>,
    /// `re_inf` codes, stored directly (see [`Inf`]).
    pub info: Vec<Inf>,
    /// `re_pri` codes, stored directly (see [`Priority`]).
    pub priorities: Vec<Priority>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenseRecord {
    pub glosses: Vec<GlossRecord>,
    /// Word/name tags. `None` is the common case (untagged word
    /// senses); boxing keeps it one pointer instead of three empty
    /// vec slots for words or a second inline shape for names.
    pub detail: Option<Box<SenseDetail>>,
    /// Rare per-sense data (restrictions, cross-refs, free-text info,
    /// sources). Boxed for the same reason; words-only in practice.
    pub extras: Option<Box<SenseExtras>>,
}

/// The tagged half of [`SenseRecord`]: word tags or name tags, never
/// both (pack writes exactly one). `None` on the sense means
/// untagged — no prefix either way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SenseDetail {
    Word {
        /// Part-of-speech tags, stored directly (see [`Pos`]).
        pos: Vec<Pos>,
        /// Field-of-application tags (see [`Field`]).
        field: Vec<Field>,
        /// Miscellaneous-info tags (see [`Misc`]).
        misc: Vec<Misc>,
    },
    Name {
        /// Name-type tags, stored directly (see [`NameType`]).
        name_types: Vec<NameType>,
    },
}

impl SenseDetail {
    /// `None` when every tag vec is empty (most word senses) —
    /// callers pass parser output straight through.
    pub fn packed_word(pos: Vec<Pos>, field: Vec<Field>, misc: Vec<Misc>) -> Option<Box<Self>> {
        if pos.is_empty() && field.is_empty() && misc.is_empty() {
            return None;
        }
        Some(Box::new(Self::Word { pos, field, misc }))
    }

    /// `None` when tagless (5 such names in the data); displays the
    /// same as untagged either way.
    pub fn packed_name(name_types: Vec<NameType>) -> Option<Box<Self>> {
        if name_types.is_empty() {
            return None;
        }
        Some(Box::new(Self::Name { name_types }))
    }
}

/// The boxed half of [`SenseRecord`]: everything the hot paths
/// (search verify, meaning display) skip. `None` is the common case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenseExtras {
    pub stagk: Vec<StrRef>,
    pub stagr: Vec<StrRef>,
    pub xref: Vec<StrRef>,
    pub ant: Vec<StrRef>,
    pub info: Vec<StrRef>,
    pub sources: Vec<LangSourceRecord>,
}

impl SenseExtras {
    /// `None` when every field is empty (the common case) — callers
    /// pass parser output straight through, no emptiness checks.
    pub fn packed(
        stagk: Vec<StrRef>,
        stagr: Vec<StrRef>,
        xref: Vec<StrRef>,
        ant: Vec<StrRef>,
        info: Vec<StrRef>,
        sources: Vec<LangSourceRecord>,
    ) -> Option<Box<Self>> {
        if stagk.is_empty()
            && stagr.is_empty()
            && xref.is_empty()
            && ant.is_empty()
            && info.is_empty()
            && sources.is_empty()
        {
            return None;
        }
        Some(Box::new(Self {
            stagk,
            stagr,
            xref,
            ant,
            info,
            sources,
        }))
    }
}

impl SenseRecord {
    /// Tag indices for the FFI boundary (`pos` or `name_types` —
    /// the caller resolves labels by file). Empty when untagged.
    pub fn tag_indices(&self) -> Vec<u8> {
        match &self.detail {
            Some(boxed) => match &**boxed {
                SenseDetail::Word { pos, .. } => pos.iter().map(|p| *p as u8).collect(),
                SenseDetail::Name { name_types } => name_types.iter().map(|n| *n as u8).collect(),
            },
            None => Vec::new(),
        }
    }

    /// Field-tag indices for the FFI boundary (words only; names
    /// carry none). Empty when untagged.
    pub fn field_indices(&self) -> Vec<u8> {
        match &self.detail {
            Some(boxed) => match &**boxed {
                SenseDetail::Word { field, .. } => field.iter().map(|f| *f as u8).collect(),
                SenseDetail::Name { .. } => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    /// Misc-tag indices for the FFI boundary (words only; names
    /// carry none). Empty when untagged.
    pub fn misc_indices(&self) -> Vec<u8> {
        match &self.detail {
            Some(boxed) => match &**boxed {
                SenseDetail::Word { misc, .. } => misc.iter().map(|m| *m as u8).collect(),
                SenseDetail::Name { .. } => Vec::new(),
            },
            None => Vec::new(),
        }
    }

    /// Tag entities for display. Empty when untagged.
    pub fn tag_entities(&self) -> Vec<&'static str> {
        match &self.detail {
            Some(boxed) => match &**boxed {
                SenseDetail::Word { pos, .. } => pos.iter().map(|p| p.entity()).collect(),
                SenseDetail::Name { name_types } => name_types.iter().map(|n| n.entity()).collect(),
            },
            None => Vec::new(),
        }
    }

    /// Cross-references, empty when the sense carries no extras.
    pub fn xref(&self) -> &[StrRef] {
        match &self.extras {
            Some(boxed) => &boxed.xref,
            None => &[],
        }
    }

    /// Antonyms, empty without extras.
    pub fn ant(&self) -> &[StrRef] {
        match &self.extras {
            Some(boxed) => &boxed.ant,
            None => &[],
        }
    }

    /// Kanji-form restrictions, empty without extras.
    pub fn stagk(&self) -> &[StrRef] {
        match &self.extras {
            Some(boxed) => &boxed.stagk,
            None => &[],
        }
    }

    /// Reading restrictions, empty without extras.
    pub fn stagr(&self) -> &[StrRef] {
        match &self.extras {
            Some(boxed) => &boxed.stagr,
            None => &[],
        }
    }

    /// Free-text notes, empty without extras.
    pub fn info_notes(&self) -> &[StrRef] {
        match &self.extras {
            Some(boxed) => &boxed.info,
            None => &[],
        }
    }

    /// Source attestations, empty without extras.
    pub fn sources(&self) -> &[LangSourceRecord] {
        match &self.extras {
            Some(boxed) => &boxed.sources,
            None => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LangSourceRecord {
    pub lang: Option<StrRef>,
    pub text: StrRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlossRecord {
    /// `g_type` attribute, stored directly (`Unspecified` when
    /// absent — always, for names). `g_gend` is dropped: zero
    /// occurrences, no consumer.
    pub typ: GlossType,
    /// Display text: English gloss (words; import keeps
    /// `xml:lang`-absent glosses only) or transliteration (names).
    pub text: StrRef,
}
