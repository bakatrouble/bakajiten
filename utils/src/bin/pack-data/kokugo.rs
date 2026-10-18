//! Kokugo import: kaikki jawiktionary extract plus Japanese WordNet
//! LMF into entry lines shaped exactly like the JMdict parser's
//! `Entry` (plus `synonyms`, `hypernyms`, `source`, which pack-data
//! ignores). Rust port of the retired `utils/kokugo_import.py` —
//! same rules, same audit counters, same deterministic output:
//!
//! - Entries: `lang_code == "ja"` only; `character` (kanjidic2 covers
//!   it) and `name` (JMnedict covers it) skipped.
//! - keb: non-kana headword + non-kana forms tagged `kanji`.
//! - reb: kana headword + kana forms whose tags are a subset of the
//!   reading tag set (conjugations and on-reading variants never
//!   qualify); reb-less entries stay empty here — pack-data fills
//!   them from JMdict (`--jmdict-backfill`) when exactly one reading
//!   exists across all keb.
//! - pos: conservative kaikki→JMdict map, plus verb classes derived
//!   from conjugation tags (row + grade); unmapped kinds omitted,
//!   never fabricated.
//! - WordNet backfills headwords Wiktionary misses (one sense per
//!   synset definition) and attaches synonym/hypernym member words.
//! - Deterministic: entries sorted by headword, ent_seq 1..N,
//!   relation lists sorted and capped at 12.

use dict::gloss::pos::Pos;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;

const REL_CAP: usize = 12;

/// kaikki pos -> JMdict entity ref. Verbs derive separately (a bare
/// `&v1;` on a godan verb would be wrong data).
fn pos_entity(pos: &str) -> Option<&'static str> {
    Some(match pos {
        "noun" => "&n;",
        "adj_noun" => "&adj-na;",
        "adj" => "&adj-i;",
        "adv" => "&adv;",
        "pron" => "&pn;",
        "intj" => "&int;",
        "conj" => "&conj;",
        "particle" => "&prt;",
        "prefix" => "&pref;",
        "suffix" => "&suf;",
        "num" => "&num;",
        "counter" => "&ctr;",
        "adnominal" => "&adj-pn;",
        // No &abbr; entity exists — abbreviations pack as nouns.
        "abbrev" => "&n;",
        "phrase" => "&exp;",
        "proverb" => "&exp;",
        _ => return None,
    })
}

fn godan_row(row: char) -> Option<&'static str> {
    Some(match row {
        'k' => "&v5k;",
        'g' => "&v5g;",
        's' => "&v5s;",
        't' => "&v5t;",
        'n' => "&v5n;",
        'b' => "&v5b;",
        'm' => "&v5m;",
        'r' => "&v5r;",
        'w' => "&v5u;",
        _ => return None,
    })
}

fn yodan_row(row: char) -> Option<&'static str> {
    Some(match row {
        'b' => "&v4b;",
        'g' => "&v4g;",
        'h' => "&v4h;",
        'k' => "&v4k;",
        'm' => "&v4m;",
        'r' => "&v4r;",
        's' => "&v4s;",
        't' => "&v4t;",
        _ => return None,
    })
}

fn is_kana(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || c == 'ー' || c == '・')
}

fn is_reading_tagset(tags: &BTreeSet<String>) -> bool {
    tags.iter().all(|t| {
        matches!(
            t.as_str(),
            "transliteration"
                | "kun"
                | "ko-kun"
                | "joyo"
                | "hiragana"
                | "katakana"
                | "kana"
                | "canonical"
        )
    })
}

/// JMdict verb classes evidenced by conjugation-form tags. Rows
/// without an entity stay untagged; multiple classes all emit.
fn verb_classes(forms: &[Vec<String>]) -> Vec<String> {
    let mut tags = BTreeSet::new();
    let mut rows = BTreeSet::new();
    for form_tags in forms {
        for t in form_tags {
            tags.insert(t.clone());
            if let Some(row) = t.strip_suffix("-row").and_then(|r| r.chars().next()) {
                rows.insert(row);
            }
        }
    }
    let mut out = BTreeSet::new();
    if tags.contains("ichidan") {
        // Modern ichidan governs; co-listed historical forms are
        // never v2 classes.
        out.insert("&v1;".to_string());
    }
    if tags.contains("irregular") {
        for (row, entity) in [('s', "&vs;"), ('k', "&vk;"), ('z', "&vz;")] {
            if rows.contains(&row) {
                out.insert(entity.to_string());
            }
        }
    }
    if tags.contains("godan") {
        for r in &rows {
            if let Some(entity) = godan_row(*r) {
                out.insert(entity.to_string());
            }
        }
    }
    if tags.contains("yodan") && !tags.contains("ichidan") {
        for r in &rows {
            if let Some(entity) = yodan_row(*r) {
                out.insert(entity.to_string());
            }
        }
    }
    out.into_iter().collect()
}

#[derive(Default)]
struct Stats {
    entries: u64,
    skip_pos: u64,
    skip_empty: u64,
    skip_glossless: u64,
    verb_derived: u64,
    verb_bare: u64,
    backfilled: u64,
    emitted: u64,
    wordnet_lemmas: u64,
    wordnet_synsets: u64,
    wordnet_nodef: u64,
    /// (count, first-seen) so ties print in encounter order, like
    /// `Counter.most_common`.
    reading_tagsets: HashMap<String, (u64, usize)>,
    pos_unmapped: HashMap<String, (u64, usize)>,
    seen: usize,
}

impl Stats {
    fn bump_map(&mut self, map: &str, key: String) {
        let target = if map == "tag" {
            &mut self.reading_tagsets
        } else {
            &mut self.pos_unmapped
        };
        if let Some(entry) = target.get_mut(&key) {
            entry.0 += 1;
        } else {
            let first = self.seen;
            self.seen += 1;
            target.insert(key, (1, first));
        }
    }

    fn top(&self, map: &str, n: usize) -> Vec<(String, u64)> {
        let source = if map == "tag" {
            &self.reading_tagsets
        } else {
            &self.pos_unmapped
        };
        let mut items: Vec<_> = source
            .iter()
            .map(|(k, (c, f))| (k.clone(), *c, *f))
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
        items.into_iter().take(n).map(|(k, c, _)| (k, c)).collect()
    }
}

fn js(s: &str) -> String {
    serde_json::to_string(s).expect("JSON string escape")
}

/// Merged dictionary entry: Wiktionary and WordNet passes append
/// here, readings backfilled downstream at pack time.
struct Merged {
    word: String,
    keb: Vec<String>,
    reb: Vec<String>,
    senses: Vec<(Vec<String>, Vec<String>)>,
    syn: Vec<String>,
    hyp: Vec<String>,
    source: &'static str,
}

struct KaikkiEntry {
    word: String,
    keb: Vec<String>,
    reb: Vec<String>,
    /// (entities, glosses) per sense.
    senses: Vec<(Vec<String>, Vec<String>)>,
    synonyms: Vec<String>,
}

fn str_list(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_kaikki(path: &Path, stats: &mut Stats) -> Result<Vec<KaikkiEntry>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let reader: Box<dyn BufRead> = if path.extension().is_some_and(|e| e == "gz") {
        Box::new(BufReader::new(flate2::read::GzDecoder::new(
            BufReader::new(file),
        )))
    } else {
        Box::new(BufReader::new(file))
    };
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("{}: {e}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let e: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| format!("{}: {e}", path.display()))?;
        if e.get("lang_code").and_then(|v| v.as_str()) != Some("ja") {
            continue;
        }
        let pos = e.get("pos").and_then(|v| v.as_str()).unwrap_or("");
        if pos == "character" || pos == "name" {
            stats.skip_pos += 1;
            continue;
        }
        let word = e
            .get("word")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if word.is_empty() {
            stats.skip_empty += 1;
            continue;
        }
        let mut keb = Vec::new();
        if !is_kana(&word) {
            keb.push(word.clone());
        }
        let mut reb = Vec::new();
        if is_kana(&word) {
            reb.push(word.clone());
        }
        let mut form_tags: Vec<Vec<String>> = Vec::new();
        if let Some(forms) = e.get("forms").and_then(|v| v.as_array()) {
            for fm in forms {
                let form = fm.get("form").and_then(|v| v.as_str()).unwrap_or("");
                let tags: BTreeSet<String> =
                    str_list(fm.get("tags").unwrap_or(&serde_json::Value::Null))
                        .into_iter()
                        .collect();
                form_tags.push(tags.iter().cloned().collect());
                if form.is_empty() {
                    continue;
                }
                if tags.contains("kanji") && !is_kana(form) {
                    if !keb.contains(&form.to_string()) {
                        keb.push(form.to_string());
                    }
                } else if is_kana(form)
                    && tags.contains("transliteration")
                    && is_reading_tagset(&tags)
                {
                    if !reb.contains(&form.to_string()) {
                        reb.push(form.to_string());
                    }
                } else if is_kana(form) && tags.contains("transliteration") {
                    let mut sorted: Vec<_> = tags.iter().cloned().collect();
                    sorted.sort();
                    let key = if sorted.len() == 1 {
                        format!("('{}',)", sorted[0])
                    } else {
                        format!(
                            "({})",
                            sorted
                                .iter()
                                .map(|t| format!("'{t}'"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    stats.bump_map("tag", key);
                }
            }
        }
        let entities: Vec<String>;
        if pos == "verb" {
            let classes = verb_classes(&form_tags);
            if classes.is_empty() {
                stats.verb_bare += 1;
            } else {
                stats.verb_derived += 1;
            }
            entities = classes;
        } else {
            match pos_entity(pos) {
                Some(entity) => entities = vec![entity.to_string()],
                None => {
                    stats.bump_map("pos", if pos.is_empty() { "?" } else { pos }.to_string());
                    entities = Vec::new();
                }
            }
        }
        let mut senses = Vec::new();
        if let Some(list) = e.get("senses").and_then(|v| v.as_array()) {
            for s in list {
                let glosses: Vec<String> = s
                    .get("glosses")
                    .and_then(|v| v.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|g| g.as_str())
                            .filter(|g| !g.trim().is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                if glosses.is_empty() {
                    continue;
                }
                senses.push((entities.clone(), glosses));
            }
        }
        if senses.is_empty() {
            stats.skip_glossless += 1;
            continue;
        }
        let mut syn_set = BTreeSet::new();
        if let Some(list) = e.get("synonyms").and_then(|v| v.as_array()) {
            for s in list {
                if let Some(w) = s.get("word").and_then(|v| v.as_str()) {
                    if !w.is_empty()
                        && *w != word
                        && !keb.iter().any(|k| k == w)
                        && !reb.iter().any(|r| r == w)
                    {
                        syn_set.insert(w.to_string());
                    }
                }
            }
        }
        stats.entries += 1;
        entries.push(KaikkiEntry {
            word,
            keb,
            reb,
            senses,
            synonyms: syn_set.into_iter().collect(),
        });
    }
    Ok(entries)
}

struct WordNet {
    /// lemma -> synset ids (document order, duplicates kept).
    lemma_synsets: HashMap<String, Vec<String>>,
    /// synset id -> record.
    synsets: HashMap<String, Synset>,
    /// sense id -> lemma (only when the Lemma precedes the Sense).
    sense_lemma: HashMap<String, String>,
}

struct Synset {
    pos: String,
    definition: String,
    hypernyms: Vec<String>,
    members: String,
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> String {
    e.attributes()
        .flatten()
        .find_map(|a| {
            if a.key.as_ref() == key {
                a.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                    .ok()
                    .map(|s| s.into_owned())
            } else {
                None
            }
        })
        .unwrap_or_default()
}

/// Entity refs (`&amp;`, …) expand like the reference importer —
/// predefined XML entities resolve, anything else stays literal
/// (the reference would crash; these fields never carry custom
/// entities in practice).
fn push_ref(out: &mut String, e: &quick_xml::events::BytesRef) {
    let name: &str = e;
    let full = if name.starts_with('&') {
        name.to_string()
    } else {
        format!("&{name};")
    };
    match quick_xml::escape::unescape(&full) {
        Ok(s) => out.push_str(&s),
        Err(_) => out.push_str(&full),
    }
}

fn parse_wordnet(path: &Path, stats: &mut Stats) -> Result<WordNet, String> {
    use quick_xml::events::Event;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = quick_xml::Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut wn = WordNet {
        lemma_synsets: HashMap::new(),
        synsets: HashMap::new(),
        sense_lemma: HashMap::new(),
    };
    let mut lex_lemma: Option<String> = None;
    let mut lex_syns: Vec<String> = Vec::new();
    let mut in_lex = false;
    let mut lex_depth = 0usize;
    let mut depth = 0usize;
    let mut syn_id = String::new();
    let mut syn_pos = String::new();
    let mut syn_def = String::new();
    let mut syn_hypers: Vec<String> = Vec::new();
    let mut syn_members = String::new();
    let mut in_syn = false;
    let mut syn_depth = 0usize;
    let mut in_definition = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                depth += 1;
                match e.name().as_ref() {
                    "LexicalEntry" => {
                        in_lex = true;
                        lex_depth = depth;
                        lex_lemma = None;
                        lex_syns.clear();
                    }
                    "Lemma" if in_lex && depth == lex_depth + 1 => {
                        // Later siblings overwrite, even with an empty
                        // form (document order, like the reference).
                        lex_lemma = Some(attr(&e, "writtenForm"));
                    }
                    "Sense" if in_lex && depth == lex_depth + 1 => {
                        let sid = attr(&e, "id");
                        let syn = attr(&e, "synset");
                        if let Some(lemma) = lex_lemma.as_ref() {
                            if !lemma.is_empty() && !sid.is_empty() {
                                wn.sense_lemma.insert(sid.clone(), lemma.clone());
                            }
                        }
                        if !syn.is_empty() {
                            lex_syns.push(syn);
                        }
                    }
                    "Synset" => {
                        in_syn = true;
                        syn_depth = depth;
                        syn_id = attr(&e, "id");
                        syn_pos = attr(&e, "partOfSpeech");
                        syn_def.clear();
                        syn_hypers.clear();
                        syn_members = attr(&e, "members");
                    }
                    "Definition" if in_syn && depth == syn_depth + 1 && syn_def.is_empty() => {
                        in_definition = true;
                    }
                    "SynsetRelation" if in_syn && depth == syn_depth + 1 => {
                        if attr(&e, "relType") == "hypernym" {
                            let target = attr(&e, "target");
                            if !target.is_empty() {
                                syn_hypers.push(target);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                match e.name().as_ref() {
                    "Lemma" if in_lex && depth == lex_depth => {
                        lex_lemma = Some(attr(&e, "writtenForm"));
                    }
                    "Sense" if in_lex && depth == lex_depth => {
                        let sid = attr(&e, "id");
                        let syn = attr(&e, "synset");
                        if let Some(lemma) = lex_lemma.as_ref() {
                            if !lemma.is_empty() && !sid.is_empty() {
                                wn.sense_lemma.insert(sid.clone(), lemma.clone());
                            }
                        }
                        if !syn.is_empty() {
                            lex_syns.push(syn);
                        }
                    }
                    "SynsetRelation" if in_syn && depth == syn_depth => {
                        if attr(&e, "relType") == "hypernym" {
                            let target = attr(&e, "target");
                            if !target.is_empty() {
                                syn_hypers.push(target);
                            }
                        }
                    }
                    "Synset" => {
                        // Empty-element synsets still count: record
                        // the bare attrs.
                        wn.synsets.insert(
                            attr(&e, "id"),
                            Synset {
                                pos: attr(&e, "partOfSpeech"),
                                definition: String::new(),
                                hypernyms: Vec::new(),
                                members: attr(&e, "members"),
                            },
                        );
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_definition {
                    syn_def.push_str(e.as_ref());
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if in_definition {
                    push_ref(&mut syn_def, &e);
                }
            }
            Ok(Event::End(e)) => {
                match e.name().as_ref() {
                    "Definition" => in_definition = false,
                    "LexicalEntry" if in_lex && depth == lex_depth => {
                        if let Some(lemma) = lex_lemma.take() {
                            if !lemma.is_empty() {
                                for syn in lex_syns.drain(..) {
                                    wn.lemma_synsets.entry(lemma.clone()).or_default().push(syn);
                                }
                            }
                        }
                        lex_syns.clear();
                        in_lex = false;
                    }
                    "Synset" if in_syn && depth == syn_depth => {
                        wn.synsets.insert(
                            std::mem::take(&mut syn_id),
                            Synset {
                                pos: std::mem::take(&mut syn_pos),
                                definition: std::mem::take(&mut syn_def),
                                hypernyms: std::mem::take(&mut syn_hypers),
                                members: std::mem::take(&mut syn_members),
                            },
                        );
                        in_syn = false;
                    }
                    _ => {}
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("{}: {e}", path.display())),
            _ => {}
        }
        buf.clear();
    }
    stats.wordnet_lemmas = wn.lemma_synsets.len() as u64;
    stats.wordnet_synsets = wn.synsets.len() as u64;
    Ok(wn)
}


fn related(
    wn: &WordNet,
    syn_members: &HashMap<String, Vec<String>>,
    headword: &str,
    own: &[String],
    synonym: bool,
) -> Vec<String> {
    let mut out = BTreeSet::new();
    for sid in own {
        if synonym {
            if let Some(members) = syn_members.get(sid) {
                out.extend(members.iter().cloned());
            }
        } else if let Some(syn) = wn.synsets.get(sid) {
            for target in &syn.hypernyms {
                if let Some(members) = syn_members.get(target) {
                    out.extend(members.iter().cloned());
                }
            }
        }
    }
    out.remove(headword);
    out.into_iter().take(REL_CAP).collect()
}

fn emit_senses(senses: &[(Vec<String>, Vec<String>)]) -> String {
    let mut parts = Vec::with_capacity(senses.len());
    for (entities, glosses) in senses {
        let pos = entities
            .iter()
            .map(|e| js(e))
            .collect::<Vec<_>>()
            .join(", ");
        let gloss = glosses
            .iter()
            .map(|g| {
                format!(
                    "{{\"lang\": \"ja\", \"gender\": null, \"typ\": null, \"text\": {}}}",
                    js(g)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!(
            "{{\"stagk\": [], \"stagr\": [], \"pos\": [{pos}], \"xref\": [], \"ant\": [], \
             \"field\": [], \"misc\": [], \"info\": [], \"sources\": [], \"glosses\": [{gloss}]}}"
        ));
    }
    parts.join(", ")
}

fn emit_entry(i: usize, m: &Merged) -> String {
    let kanji = m
        .keb
        .iter()
        .map(|k| format!("{{\"keb\": {}, \"info\": [], \"priorities\": []}}", js(k)))
        .collect::<Vec<_>>()
        .join(", ");
    let readings = m
        .reb
        .iter()
        .map(|r| {
            format!(
                "{{\"reb\": {}, \"nokanji\": false, \"restricted_to\": [], \"info\": [], \
                 \"priorities\": []}}",
                js(r)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let syn = m.syn.iter().map(|s| js(s)).collect::<Vec<_>>().join(", ");
    let hyp = m.hyp.iter().map(|s| js(s)).collect::<Vec<_>>().join(", ");
    format!(
        "{{\"ent_seq\": {i}, \"kanji\": [{kanji}], \"readings\": [{readings}], \
         \"senses\": [{}], \"translations\": [], \"synonyms\": [{syn}], \
         \"hypernyms\": [{hyp}], \"source\": {}}}",
        emit_senses(&m.senses),
        js(m.source)
    )
}

const WN_POS_N: &str = "&n;";

#[allow(clippy::too_many_arguments)]
pub fn run(kaikki: &Path, wordnet: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut stats = Stats::default();
    // Pass 1: Wiktionary.
    let mut merged: Vec<Merged> = Vec::new();
    let mut headwords = HashSet::new();
    for e in parse_kaikki(kaikki, &mut stats).map_err(|e| e.to_string())? {
        headwords.insert(e.word.clone());
        headwords.extend(e.keb.iter().cloned());
        headwords.extend(e.reb.iter().cloned());
        merged.push(Merged {
            word: e.word,
            keb: e.keb,
            reb: e.reb,
            senses: e.senses,
            syn: e.synonyms,
            hyp: Vec::new(),
            source: "wiktionary",
        });
    }
    // Pass 2: WordNet (definitions, members, relations).
    let wn = parse_wordnet(wordnet, &mut stats).map_err(|e| e.to_string())?;
    // Member sense-ids look like "synset-wNNNNN"; match by suffix.
    let mut by_suffix: HashMap<String, Vec<String>> = HashMap::new();
    let mut sense_ids: Vec<_> = wn.sense_lemma.keys().cloned().collect();
    sense_ids.sort();
    for sid in sense_ids {
        let suffix = sid
            .rsplit_once('-')
            .map(|(_, s)| s)
            .unwrap_or(&sid)
            .to_string();
        // Insertion order follows sorted sense ids; only membership
        // is consumed downstream, so any order works.
        by_suffix
            .entry(suffix)
            .or_default()
            .push(wn.sense_lemma[&sid].clone());
    }
    let mut syn_members: HashMap<String, Vec<String>> = HashMap::new();
    let mut syn_ids: Vec<_> = wn.synsets.keys().cloned().collect();
    syn_ids.sort();
    for sid in syn_ids {
        let syn = &wn.synsets[&sid];
        let mut members = BTreeSet::new();
        for mid in syn.members.split_whitespace() {
            let suffix = mid.rsplit_once('-').map(|(_, s)| s).unwrap_or(mid);
            if let Some(lemmas) = by_suffix.get(suffix) {
                members.extend(lemmas.iter().cloned());
            }
        }
        syn_members.insert(sid.clone(), members.into_iter().collect());
    }
    // Attach relations to Wiktionary entries; backfill missing lemmas.
    for m in &mut merged {
        let mut keys = BTreeSet::from([m.word.clone()]);
        keys.extend(m.keb.iter().cloned());
        keys.extend(m.reb.iter().cloned());
        let mut own = BTreeSet::new();
        for key in &keys {
            if let Some(syns) = wn.lemma_synsets.get(key) {
                own.extend(syns.iter().cloned());
            }
        }
        let own: Vec<_> = own.into_iter().collect();
        let mut syn_set: BTreeSet<String> = m.syn.iter().cloned().collect();
        syn_set.extend(related(&wn, &syn_members, &m.word, &own, true));
        syn_set.retain(|s| !keys.contains(s));
        m.syn = syn_set.into_iter().take(REL_CAP).collect();
        m.hyp = related(&wn, &syn_members, &m.word, &own, false)
            .into_iter()
            .filter(|h| !keys.contains(h))
            .collect();
    }
    let mut lemmas: Vec<_> = wn.lemma_synsets.keys().cloned().collect();
    lemmas.sort();
    for lemma in lemmas {
        if headwords.contains(&lemma) {
            continue;
        }
        let syns: BTreeSet<_> = wn.lemma_synsets[&lemma].iter().collect();
        let mut senses = Vec::new();
        for sid in syns {
            let Some(syn) = wn.synsets.get(sid) else {
                continue;
            };
            if syn.definition.is_empty() {
                continue;
            }
            let entities = if syn.pos == "n" {
                vec![WN_POS_N.to_string()]
            } else {
                Vec::new()
            };
            senses.push((entities, vec![syn.definition.clone()]));
        }
        if senses.is_empty() {
            stats.wordnet_nodef += 1;
            continue;
        }
        let (keb, reb) = if is_kana(&lemma) {
            (Vec::new(), vec![lemma.clone()])
        } else {
            (vec![lemma.clone()], Vec::new())
        };
        let own: Vec<_> = wn.lemma_synsets[&lemma]
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let syn = related(&wn, &syn_members, &lemma, &own, true);
        let hyp = related(&wn, &syn_members, &lemma, &own, false);
        merged.push(Merged {
            word: lemma,
            keb,
            reb,
            senses,
            syn,
            hyp,
            source: "wordnet",
        });
        stats.backfilled += 1;
    }
    // Deterministic order; ent_seq assigned 1..N.
    merged.sort_by(|a, b| a.word.cmp(&b.word));
    let mut lines = Vec::with_capacity(merged.len());
    for (i, m) in merged.iter().enumerate() {
        // Self-checks: schema keys are constructed, senses and glosses
        // filtered non-empty above, ent_seq assigned 1..N here.
        assert!(!m.senses.is_empty());
        for (entities, glosses) in &m.senses {
            for p in entities {
                assert!(Pos::from_entity(p).is_some(), "unknown pos {p}");
            }
            assert!(glosses.iter().any(|g| !g.trim().is_empty()));
        }
        lines.push(emit_entry(i + 1, m));
    }
    stats.emitted = merged.len() as u64;
    assert_eq!(stats.emitted as usize, merged.len());

    print!(
        "entries={} backfilled={} emitted={}\n",
        stats.entries,
        stats.backfilled,
        merged.len()
    );
    print!(
        "skip pos={} glossless={} empty={} wordnet_nodef={}\n",
        stats.skip_pos, stats.skip_glossless, stats.skip_empty, stats.wordnet_nodef
    );
    let unmapped = stats.top("pos", 10);
    print!(
        "pos unmapped: {{{}}}\n",
        unmapped
            .iter()
            .map(|(k, c)| format!("'{k}': {c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    print!(
        "verbs derived: {} bare: {}\n",
        stats.verb_derived, stats.verb_bare
    );
    let tagsets = stats.top("tag", 8);
    print!(
        "kana forms dropped by tagset: {{{}}}\n",
        tagsets
            .iter()
            .map(|(k, c)| format!("{k}: {c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    print!(
        "wordnet lemmas={} synsets={}\n",
        stats.wordnet_lemmas, stats.wordnet_synsets
    );
    print!("check: schema + entity refs + gloss presence + ent_seq 1..N — OK\n");
    Ok(lines)
}
