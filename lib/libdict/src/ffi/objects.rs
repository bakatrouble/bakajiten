//! Opaque dictionary handles: mmapped artifacts with zero-copy
//! queries (no validation — see `super` docs).
use std::sync::Arc;

use super::records::{
    Deinflection, DictEntry, DictError, KanjiEntry, KanjiPoint, KanjiStroke, SearchHit, SearchMode,
};
use crate::furigana::FuriganaSpan;
use crate::gloss::field::Field;
use crate::gloss::inf::Inf;
use crate::gloss::misc::Misc;
use crate::gloss::name_type::NameType;
use crate::gloss::pos::Pos;
use crate::gloss::search::{ArchivedDictionaryFile, SearchType};
use crate::gloss::sort_hits;
use crate::gloss::{
    ArchivedDictIndex, GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC, GLOSS_KOKUGO_MAGIC, decode_entry,
    deinflect, entry_classes, gloss_header, gloss_view,
};
use crate::kanji::{ArchivedKanjiFile, kanji_header};

/// Owned artifact storage: a file mapping on native targets
/// (instant, page-aligned, shared page cache), caller-provided bytes
/// on WASM (no mmap syscalls there). Both present the same borrowed
/// slice to the zero-copy views.
#[derive(Debug)]
enum Artifact {
    #[cfg(not(target_arch = "wasm32"))]
    Mapped(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Artifact {
    /// Map (native) or read (WASM) one artifact file.
    fn load(path: &str) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
            // SAFETY: read-only mapping of a file we never write;
            // pack output is immutable once written.
            let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| format!("{path}: {e}"))?;
            Ok(Self::Mapped(map))
        }
        #[cfg(target_arch = "wasm32")]
        {
            std::fs::read(path)
                .map(Self::Owned)
                .map_err(|e| format!("{path}: {e}"))
        }
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Mapped(map) => map,
            Self::Owned(bytes) => bytes,
        }
    }
}

/// Opaque dictionary: one JMdict, JMnedict, or kokugo artifact,
/// searched in place without decoding. The app loads exactly the
/// scoped file (scope switches reload); only one dictionary is
/// ever live.
#[derive(Debug, uniffi::Object)]
pub struct Dictionary {
    bytes: Artifact,
    /// True for JMnedict artifacts (name-type tags instead of pos):
    /// sniffed from the magic at construction.
    names: bool,
}

#[uniffi::export]
impl Dictionary {
    /// Load one gloss artifact from bytes (`GLD1` words, `GLN1`
    /// names, or `GLK1` kokugo as written by `pack-data`). Header
    /// check only (magic, version, exact length — instant); the
    /// archive itself is never validated, see module docs. Prefer
    /// [`Dictionary::from_path`] on native targets (no copy). Share
    /// the handle across threads and queries.
    #[uniffi::constructor]
    pub fn new(bytes: Vec<u8>) -> Result<Arc<Self>, DictError> {
        let names =
            check_artifact(&bytes).map_err(|message| DictError::InvalidArtifact { message })?;
        Ok(Arc::new(Self {
            bytes: Artifact::Owned(bytes),
            names,
        }))
    }

    /// Load one gloss artifact from a file path (mmapped on native
    /// targets — the Qt frontend path; instant, no copy).
    #[uniffi::constructor]
    pub fn from_path(path: String) -> Result<Arc<Self>, DictError> {
        let invalid = |message: String| DictError::InvalidArtifact { message };
        let bytes = Artifact::load(&path).map_err(invalid)?;
        let names = check_artifact(bytes.as_slice()).map_err(invalid)?;
        Ok(Arc::new(Self { bytes, names }))
    }

    /// Japanese key lookup: prefix/suffix edge, substring when
    /// `mode` is `Middle`, exact forms only when `Exact` (no
    /// deinflection, no partials). Record indices in file order,
    /// `cap` total. Empty query or `cap = 0` returns empty.
    pub fn search(&self, query: String, mode: SearchMode, cap: u32) -> Vec<DictEntry> {
        self.search_indices(query, mode, cap)
            .into_iter()
            .filter_map(|h| self.entry_at(h.idx))
            .collect()
    }

    /// Word entries for one kanji (the kanji dialog's Words
    /// tab), most relevant first, capped. Words only. Runs the
    /// substring index (`Middle` tier + relevance rank) instead of
    /// the old keb linear scan: single-kanji queries carry no
    /// trigrams, so short queries fall back to a bounded scan
    /// inside `Middle` — same result set for kanji (readings are
    /// kana and never contain one), ranked order instead of raw
    /// frequency. Readings count now (all keys, not keb only);
    /// empty and multi-char inputs miss.
    pub fn search_middle(&self, character: String, cap: u32) -> Vec<DictEntry> {
        let Some(ch) = single_char(&character) else {
            return Vec::new();
        };
        if cap == 0 {
            return Vec::new();
        }
        let bytes = self.bytes.as_slice();
        let Some((rkyv_bytes, blob)) = crate::gloss::sections_unchecked(bytes) else {
            return Vec::new();
        };
        // SAFETY: same contract as [`Dictionary::view`].
        let index: &ArchivedDictIndex = unsafe { rkyv::access_unchecked(rkyv_bytes) };
        let hits = index.search(
            crate::gloss::SearchType::Middle,
            &ch.to_string(),
            usize::MAX,
        );
        ranked(index, blob, hits)
            .into_iter()
            .take(cap as usize)
            .filter_map(|idx| self.entry_at(idx as u32))
            .collect()
    }

    /// Index-only lookup behind [`Dictionary::search`]: file flag plus    /// record index per hit, no entry contents. Backs on-demand row
    /// materialization (the table holds these, converting rows as
    /// they scroll into view). Tiered (§7): exact dictionary-form
    /// matches and class-verified deinflected hits share ONE
    /// frequency bucket per file (frequency primary, relevance
    /// order within equal frequencies; exact wins shared records
    /// and keeps its empty chain), then edge-partial leftovers.
    /// `cap` total.
    pub fn search_indices(&self, query: String, mode: SearchMode, cap: u32) -> Vec<SearchHit> {
        if query.is_empty() || cap == 0 {
            return Vec::new();
        }
        let cap = cap as usize;
        let bytes = self.bytes.as_slice();
        let view = self.view();
        let blob = crate::gloss::sections_unchecked(bytes)
            .map(|(_, blob)| blob)
            .unwrap_or_default();
        let mut out = Vec::new();
        let mut tiered: std::collections::HashSet<usize> = std::collections::HashSet::new();
        // Deinflected candidates (uncapped — the tier below caps
        // after merging). Exact mode skips them (exact forms only).
        let deinflected = if mode == SearchMode::Exact {
            Vec::new()
        } else {
            self.deinflected(&query, usize::MAX)
        };
        // Tier 1: exact + deinflected in one frequency bucket. Runs
        // in both modes — exact forms are edge-independent, and so
        // are dictionary-form candidates.
        let exact: std::collections::HashSet<usize> =
            view.exact(&query, usize::MAX).into_iter().collect();
        let mut chains: std::collections::HashMap<usize, Vec<String>> =
            std::collections::HashMap::new();
        let mut surfaces: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        for (idx, chain, matched) in &deinflected {
            if exact.contains(&(*idx as usize)) {
                continue;
            }
            chains.insert(*idx as usize, chain.clone());
            surfaces.insert(*idx as usize, matched.clone());
        }
        let mut combined: Vec<usize> = exact.into_iter().chain(chains.keys().copied()).collect();
        sort_hits(view, blob, &mut combined);
        // Stable: frequency primary, relevance order within
        // equal frequencies.
        combined.sort_by_key(|&i| {
            std::cmp::Reverse(if i < view.freqs.len() {
                view.freqs[i].to_native()
            } else {
                0
            })
        });
        for i in combined.into_iter().take(cap) {
            tiered.insert(i);
            out.push(SearchHit {
                idx: i as u32,
                chain: chains.remove(&i).unwrap_or_default(),
                matched: surfaces.remove(&i).unwrap_or_default(),
            });
        }
        // Tier 2: edge-partial leftovers (ranked hits minus the Tier 1
        // set — exact keys always prefix-match too). Exact mode has
        // no Tier 2 (exact forms only).
        if mode != SearchMode::Exact {
            let edge = match mode {
                SearchMode::Prefix => SearchType::Prefix,
                SearchMode::Suffix => SearchType::Suffix,
                SearchMode::Middle => SearchType::Middle,
                SearchMode::Exact => unreachable!("guarded above"),
            };
            let partial = ranked(view, blob, view.search(edge, &query, usize::MAX));
            for i in partial {
                if out.len() >= cap {
                    break;
                }
                if tiered.insert(i) {
                    out.push(SearchHit {
                        idx: i as u32,
                        chain: Vec::new(),
                        matched: String::new(),
                    });
                }
            }
        }
        out
    }

    /// English gloss substring lookup (the UI's meaning mode):
    /// normalized casefolded substring, `cap` total,
    /// relevance-ranked. No deinflection (English input) and no
    /// edge modes — substring only. Empty query or `cap = 0`
    /// returns empty. Hits carry empty chains (never tinted) and
    /// empty surfaces (the table highlights the query itself,
    /// which the gloss contains).
    pub fn search_meaning_indices(&self, query: String, cap: u32) -> Vec<SearchHit> {
        if query.is_empty() || cap == 0 {
            return Vec::new();
        }
        let cap = cap as usize;
        let bytes = self.bytes.as_slice();
        let view = self.view();
        let blob = crate::gloss::sections_unchecked(bytes)
            .map(|(_, blob)| blob)
            .unwrap_or_default();
        ranked(view, blob, view.search_meaning(blob, &query, usize::MAX))
            .into_iter()
            .take(cap)
            .map(|i| SearchHit {
                idx: i as u32,
                chain: Vec::new(),
                matched: String::new(),
            })
            .collect()
    }

    /// Conjugated-form candidates for `query`: dictionary-form
    /// surfaces with reason chains, shallowest first. No dictionary
    /// verification here — the "did you mean" path from §8; see
    /// [`Dictionary::search_indices`] for class-verified hits.
    pub fn deinflect(&self, query: String) -> Vec<Deinflection> {
        deinflect(&query)
            .into_iter()
            .map(|c| Deinflection {
                surface: c.surface,
                chain: c.chain.into_iter().map(str::to_string).collect(),
            })
            .collect()
    }

    /// Record count.
    pub fn count(&self) -> u64 {
        self.view().len() as u64
    }

    /// One entry's contents on demand: `idx` is a record index from
    /// [`Dictionary::search`]. `None` for out-of-range indices or
    /// corrupt frames (logged, never unwound through paint). The
    /// table model calls this per visible row instead of
    /// materializing every hit up front.
    pub fn entry_at(&self, idx: u32) -> Option<DictEntry> {
        let bytes = self.bytes.as_slice();
        let (rkyv_bytes, blob) = crate::gloss::sections_unchecked(bytes)?;
        // SAFETY: same contract as [`Dictionary::view`], plus an exact-length
        // header check at load (offsets cannot overrun).
        let index: &ArchivedDictIndex = unsafe { rkyv::access_unchecked(rkyv_bytes) };
        let frame = index.entry_frame(blob, idx as usize)?;
        match decode_entry(frame) {
            Ok(entry) => Some(DictEntry::from_native(
                &entry,
                &index.pool,
                &index.str_blob,
                self.names,
            )),
            Err(e) => {
                // Corrupt frame means a bad pack, not a bad query.
                eprintln!("Dictionary::entry_at: {e}");
                None
            }
        }
    }
}

impl Dictionary {
    /// Borrowed index (unchecked: header-checked at load, own
    /// pack output, storage owned and immutable — see module docs).
    fn view(&self) -> &ArchivedDictIndex {
        // SAFETY: same contract as `KanjiDictionary::view`.
        unsafe { rkyv::access_unchecked(index_bytes(self.bytes.as_slice())) }
    }

    /// Class-verified deinflected hits: `(file flag, record index,
    /// reason chain, candidate surface)` in BFS order (shallowest chains first),
    /// capped. Each candidate surface exact-lookups its file; hits
    /// survive when the entry's `pos` classes intersect the
    /// candidate's claims (empty claims = any entry — transparent
    /// chains carry no class info).
    fn deinflected(&self, query: &str, cap: usize) -> Vec<(u32, Vec<String>, String)> {
        let mut out = Vec::new();
        // BFS order (shallowest chains first) — mirrors the direct
        // phase.
        let candidates = deinflect(query);
        let bytes = self.bytes.as_slice();
        let view = self.view();
        let blob = crate::gloss::sections_unchecked(bytes)
            .map(|(_, blob)| blob)
            .unwrap_or_default();
        for candidate in &candidates {
            let chain: Vec<String> = candidate.chain.iter().map(|r| r.to_string()).collect();
            let matched = candidate.surface.clone();
            for idx in view.exact(&candidate.surface, cap.saturating_sub(out.len())) {
                let verified = view
                    .entry_frame(blob, idx)
                    .and_then(|frame| decode_entry(frame).ok())
                    .map(|entry| {
                        candidate.classes.is_empty()
                            || entry_classes(&entry).intersects(&candidate.classes)
                    })
                    .unwrap_or(false);
                if verified {
                    out.push((idx as u32, chain.clone(), matched.clone()));
                }
                if out.len() >= cap {
                    return out;
                }
            }
        }
        out
    }
}

/// Header check accepting any gloss artifact (`GLD1` words,
/// `GLN1` names, `GLK1` kokugo): returns the names flag (JMnedict
/// entries carry name-type tags instead of pos). Header only
/// (magic, version, exact length — instant).
fn check_artifact(bytes: &[u8]) -> Result<bool, String> {
    const MAGICS: [&[u8; 4]; 3] = [GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC, GLOSS_KOKUGO_MAGIC];
    let magic = MAGICS
        .into_iter()
        .find(|m| bytes.get(..4) == Some(m.as_slice()))
        .ok_or_else(|| "gloss: missing magic (want GLD1, GLN1, or GLK1)".to_string())?;
    gloss_header(bytes, magic).map_err(|e| e.to_string())?;
    Ok(magic == GLOSS_JMNEDICT_MAGIC)
}

/// rkyv index section of mapped artifact bytes (header-checked at
/// load, so the split cannot fail — bad lengths fail the
/// exact-length gate first).
fn index_bytes(bytes: &[u8]) -> &[u8] {
    crate::gloss::index_section(bytes).expect("header checked")
}

/// Source version stamp of one gloss artifact on disk (`GLD1`
/// words, `GLN1` names, or `GLK1` kokugo): the `DictIndex.version`
/// packed at build time (JMdict/JMnedict date or mtime), read
/// zero-copy off the mapping. Lets the Android dictionary manager
/// show the saved version without opening a search handle.
#[uniffi::export]
pub fn gloss_version(path: String) -> Result<String, DictError> {
    let invalid = |message: String| DictError::InvalidArtifact { message };
    let bytes = Artifact::load(&path).map_err(invalid)?;
    let bytes = bytes.as_slice();
    let magic = [GLOSS_JMDICT_MAGIC, GLOSS_JMNEDICT_MAGIC, GLOSS_KOKUGO_MAGIC]
        .into_iter()
        .find(|m| bytes.get(..4) == Some(m.as_slice()))
        .ok_or_else(|| invalid("gloss: missing magic (want GLD1, GLN1, or GLK1)".to_string()))?;
    let view = gloss_view(bytes, magic).map_err(invalid)?;
    Ok(view.index.version.as_str().to_owned())
}

/// Source versions stamped into `kanji.bin`: kanjidic2
/// `<date_of_creation>` plus the KanjiVG marker.
#[derive(Debug, Clone, uniffi::Record)]
pub struct KanjiVersions {
    pub kanjidic2: String,
    pub kanjivg: String,
}

/// Saved versions of one kanji artifact on disk, read zero-copy off
/// the mapping (same use as [`gloss_version`]).
#[uniffi::export]
pub fn kanji_versions(path: String) -> Result<KanjiVersions, DictError> {
    let invalid = |message: String| DictError::InvalidArtifact { message };
    let bytes = Artifact::load(&path).map_err(invalid)?;
    let file = crate::kanji::kanji_view(bytes.as_slice()).map_err(invalid)?;
    Ok(KanjiVersions {
        kanjidic2: file.kanjidic2_version.as_str().to_owned(),
        kanjivg: file.kanjivg_version.as_str().to_owned(),
    })
}

/// Opaque kanji dictionary: `KJD1` artifact with lookup by
/// character.
#[derive(Debug, uniffi::Object)]
pub struct KanjiDictionary {
    bytes: Artifact,
}

#[uniffi::export]
impl KanjiDictionary {
    /// Load the kanji artifact from bytes (header check only — see
    /// [`Dictionary::new`]); share the handle across threads and
    /// queries.
    #[uniffi::constructor]
    pub fn new(kanji_bytes: Vec<u8>) -> Result<Arc<Self>, DictError> {
        kanji_header(&kanji_bytes).map_err(|e| DictError::InvalidArtifact {
            message: e.to_string(),
        })?;
        Ok(Arc::new(Self {
            bytes: Artifact::Owned(kanji_bytes),
        }))
    }

    /// Load the kanji artifact from a file path (mmapped on native
    /// targets — instant, no copy).
    #[uniffi::constructor]
    pub fn from_path(kanji_path: String) -> Result<Arc<Self>, DictError> {
        let invalid = |message: String| DictError::InvalidArtifact { message };
        let bytes = Artifact::load(&kanji_path).map_err(invalid)?;
        kanji_header(bytes.as_slice()).map_err(|e| invalid(e.to_string()))?;
        Ok(Arc::new(Self { bytes }))
    }

    /// Metadata for one character (single-char string; anything else
    /// returns `None`), or `None` when absent.
    pub fn lookup(&self, character: String) -> Option<KanjiEntry> {
        let ch = single_char(&character)?;
        self.view()
            .records
            .iter()
            .find(|r| r.character == ch)
            .map(KanjiEntry::from)
    }

    /// Kanji record count.
    pub fn kanji_count(&self) -> u64 {
        self.view().records.len() as u64
    }

    /// KanjiVG stroke polylines for one character, in stroke order —
    /// the stroke-order diagram source. Empty for uncovered
    /// characters (kana etc. carry no kanjidic2 entry, or no
    /// strokes at all).
    pub fn strokes(&self, character: String) -> Vec<KanjiStroke> {
        let Some(ch) = single_char(&character) else {
            return Vec::new();
        };
        self.view()
            .records
            .iter()
            .find(|r| r.character == ch)
            .map(|r| {
                r.strokes
                    .iter()
                    .map(|s| KanjiStroke {
                        points: s
                            .0
                            .iter()
                            .map(|p| KanjiPoint {
                                x: p.x.into(),
                                y: p.y.into(),
                            })
                            .collect(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Characters whose kradfile parts contain `part` (the "part of"
    /// tab), in file order. Empty for multi-char or partless input.
    pub fn containing(&self, part: String) -> Vec<String> {
        let Some(ch) = single_char(&part) else {
            return Vec::new();
        };
        self.view()
            .records
            .iter()
            .filter(|r| r.parts.iter().any(|c| *c == ch))
            .map(|r| r.character.to_string())
            .collect()
    }
}

impl KanjiDictionary {
    /// Borrowed file view (unchecked: header-checked at load, own
    /// pack output, storage owned and immutable — see module docs).
    fn view(&self) -> &ArchivedKanjiFile {
        // SAFETY: see `Dictionary::view`.
        unsafe { rkyv::access_unchecked(self.bytes.as_slice()) }
    }
}

/// Trait hits in relevance order with duplicates removed.
/// Matching and verification live in the trait; ranking (`sort_hits`:
/// zkanji relevance over stored frequencies) lives here. Callers cap
/// after tier filtering so filtered hits eat no cap slots.
fn ranked(view: &ArchivedDictIndex, blob: &[u8], hits: Vec<usize>) -> Vec<usize> {
    let mut hits = hits;
    sort_hits(view, blob, &mut hits);
    // Dedup after ranking (equal records sort adjacently).
    hits.dedup();
    hits
}

/// Single-char string to `char` (UniFFI has no `char` type, so the
/// boundary speaks strings and validates here).
fn single_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}

/// Entity label for a word-hit tag index (`DictSense.pos` when
/// `is_name` is false). Out-of-range indices (only possible in
/// hand-crafted files) yield an empty string, never a panic.
#[uniffi::export]
pub fn pos_label(idx: u8) -> String {
    Pos::from_index(idx)
        .map(|p| p.entity().to_string())
        .unwrap_or_default()
}

/// Human-readable description for a word-hit tag index (tooltip
/// text for the abbreviated table tags). Same empty-on-invalid
/// contract as [`pos_label`].
#[uniffi::export]
pub fn pos_description(idx: u8) -> String {
    Pos::from_index(idx)
        .map(|p| p.description().to_string())
        .unwrap_or_default()
}

/// Entity label for a name-hit tag index (`DictSense.pos` when
/// `is_name` is true). Same empty-on-invalid contract as
/// [`pos_label`].
#[uniffi::export]
pub fn name_type_label(idx: u8) -> String {
    NameType::from_index(idx)
        .map(|n| n.entity().to_string())
        .unwrap_or_default()
}

/// Human-readable description for a name-hit tag index. Same
/// empty-on-invalid contract as [`pos_label`].
#[uniffi::export]
pub fn name_type_description(idx: u8) -> String {
    NameType::from_index(idx)
        .map(|n| n.description().to_string())
        .unwrap_or_default()
}

/// Entity label for a field-of-application tag index
/// (`DictSense.field`, words only). Same empty-on-invalid contract
/// as [`pos_label`].
#[uniffi::export]
pub fn field_label(idx: u8) -> String {
    Field::from_index(idx)
        .map(|f| f.entity().to_string())
        .unwrap_or_default()
}

/// Human-readable description for a field-of-application tag
/// index. Same empty-on-invalid contract as [`pos_label`].
#[uniffi::export]
pub fn field_description(idx: u8) -> String {
    Field::from_index(idx)
        .map(|f| f.description().to_string())
        .unwrap_or_default()
}

/// Entity label for a misc-info tag index (`DictSense.misc`, words
/// only). Same empty-on-invalid contract as [`pos_label`].
#[uniffi::export]
pub fn misc_label(idx: u8) -> String {
    Misc::from_index(idx)
        .map(|m| m.entity().to_string())
        .unwrap_or_default()
}

/// Classical Kangxi radical glyph for a 1–214 number (empty when
/// out of range).
#[uniffi::export]
pub fn radical_glyph(number: u32) -> String {
    crate::kanji::display::radical_glyph(number)
}

/// JIS X 0208 code for kuten (`0x385e` form).
#[uniffi::export]
pub fn jis_hex(ku: u8, ten: u8) -> String {
    crate::kanji::display::jis_hex(ku, ten)
}

/// Grade category for display (translations stay trackside behind
/// the kind).
#[uniffi::export]
pub fn grade_kind(grade: u32) -> crate::kanji::display::GradeKind {
    crate::kanji::display::grade_kind(grade)
}

/// Kana → romaji for reading display:
/// Hepburn macrons or Kunrei circumflexes, same long-vowel rules.
#[uniffi::export]
pub fn kana_to_romaji(kana: String, style: crate::romaji::RomajiStyle) -> String {
    crate::romaji::to_romaji(&kana, style)
}

/// True when the string holds no kanji (a reading in effect).
#[uniffi::export]
pub fn is_kana(text: String) -> bool {
    crate::romaji::is_kana(&text)
}

/// Segments a headword against its reading (`None` when unalignable
/// or ruby-free — both render as the plain heading).
#[uniffi::export]
pub fn furigana_segment(head: String, reading: String) -> Option<Vec<FuriganaSpan>> {
    crate::furigana::segment(&head, &reading)
}

/// Human-readable description for a misc-info tag index. Same
/// empty-on-invalid contract as [`pos_label`].
#[uniffi::export]
pub fn misc_description(idx: u8) -> String {
    Misc::from_index(idx)
        .map(|m| m.description().to_string())
        .unwrap_or_default()
}

/// Entity label for a kanji/reading info-code index
/// (`DictKanjiElement.info`, words only). Same empty-on-invalid
/// contract as [`pos_label`].
#[uniffi::export]
pub fn inf_label(idx: u8) -> String {
    Inf::from_index(idx)
        .map(|i| i.entity().to_string())
        .unwrap_or_default()
}

/// Romaji → hiragana batch conversion for the query edge:
/// ASCII runs convert, kana/kanji pass
/// through, so mixed queries convert character-class by
/// character-class. Total and infallible — unknown input falls
/// through untouched.
#[uniffi::export]
pub fn romaji_to_kana(text: String) -> String {
    crate::kana::convert::to_kana(&text)
}

/// IME display for a raw romaji buffer:
/// settled kana plus the pending live tail shown raw (`shin` → しn).
/// Same total contract as [`romaji_to_kana`].
#[uniffi::export]
pub fn romaji_display(raw: String) -> String {
    crate::kana::convert::display(&raw)
}

/// True while the buffer ends mid-mora (same scan as
/// [`romaji_display`]: `shin` pending, `shi` settled). The
/// trackside input gates composition behind this: only
/// backspace and conversion keystrokes land while set.
#[uniffi::export]
pub fn romaji_has_pending(raw: String) -> bool {
    crate::kana::convert::has_pending_tail(&raw)
}

/// Numbered pinyin (`ding1`, `nu:3`) → tone-marked (`dīng`, `nǚ`).
/// Neutral `5` strips to bare text, missing digits pass through.
/// Total and infallible, like the kana converters.
#[uniffi::export]
pub fn pinyin_marked(text: String) -> String {
    crate::pinyin::to_marked(&text)
}
