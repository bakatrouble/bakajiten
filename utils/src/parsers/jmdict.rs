//! Raw streaming parsers for `JMdict.xml` and `JMnedict.xml`.
//!
//! Both files share the entry skeleton (`ent_seq`, `k_ele*`,
//! `r_ele*`) and differ only in the payload: JMdict entries carry
//! `sense*` (parts of speech, glosses), JMnedict entries carry
//! `trans*` (name types, transliterations). One parser reads either;
//! the unused side simply stays empty.
//!
//! Raw means raw: no normalization, no entity expansion (DTD entity
//! refs like `&adj-i;` / `&n;` are kept literally — expansion is the
//! converter's job), no validation beyond well-formedness. Unknown
//! elements are skipped so format additions don't break the build.
//! Streaming (SAX-style) throughout: parsing never holds more than
//! one entry, so the 150 MB inputs stay at constant memory.
//!
//! Entry points: [`parse_file`] (stream from disk with a callback),
//! [`parse_str`] (small inputs, collects — tests).

use super::error::{Error, Result};
use dict::gloss::search::normalize_jap;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::io::BufRead;
use std::path::Path;

/// One dictionary entry (JMdict, JMnedict, or either).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Entry {
    /// Stable entry id (`ent_seq`), the cross-release join key.
    pub ent_seq: u32,
    /// Kanji forms (`k_ele*`).
    pub kanji: Vec<KanjiElement>,
    /// Readings (`r_ele*`).
    pub readings: Vec<ReadingElement>,
    /// Word senses (`sense*`, JMdict).
    pub senses: Vec<Sense>,
    /// Name translations (`trans*`, JMnedict).
    pub translations: Vec<Translation>,
}

impl Entry {
    pub fn jap_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for kanji in &self.kanji {
            keys.push(&kanji.keb);
        }
        for reading in &self.readings {
            keys.push(&reading.reb);
        }
        keys.iter().map(|k| normalize_jap(*k)).collect()
    }
}

/// One `k_ele`: kanji form plus priority/info tags.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct KanjiElement {
    /// Kanji form (`keb`).
    pub keb: String,
    /// Kanji info tags (`ke_inf*`, e.g. `&ateji;`).
    pub info: Vec<String>,
    /// Priority tags (`ke_pri*`, e.g. `ichi1`, `nf01`).
    pub priorities: Vec<String>,
}

/// One `r_ele`: reading plus its restrictions and tags.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ReadingElement {
    /// Reading (`reb`, kana).
    pub reb: String,
    /// True when `re_nokanji` is present (kana-only reading).
    pub nokanji: bool,
    /// Readings restricted to these kanji forms (`re_restr*`).
    pub restricted_to: Vec<String>,
    /// Reading info tags (`re_inf*`, e.g. `&sk;`).
    pub info: Vec<String>,
    /// Priority tags (`re_pri*`).
    pub priorities: Vec<String>,
}

/// One JMdict `sense`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Sense {
    /// Senses restricted to these kanji forms (`stagk*`).
    pub stagk: Vec<String>,
    /// Senses restricted to these readings (`stagr*`).
    pub stagr: Vec<String>,
    /// Parts of speech (`pos*`, entity refs kept raw, e.g. `&n;`).
    pub pos: Vec<String>,
    /// Cross-references (`xref*`, e.g. `見る・1`).
    pub xref: Vec<String>,
    /// Antonyms (`ant*`).
    pub ant: Vec<String>,
    /// Field-of-application tags (`field*`, e.g. `&comp;`).
    pub field: Vec<String>,
    /// Misc tags (`misc*`, e.g. `&uk;`).
    pub misc: Vec<String>,
    /// Sense notes (`s_inf*`).
    pub info: Vec<String>,
    /// Source languages (`lsource*`).
    pub sources: Vec<LangSource>,
    /// Glosses (`gloss*`).
    pub glosses: Vec<Gloss>,
}

/// One `lsource`: source-language attribution of a gloss.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LangSource {
    /// `xml:lang` (absent = English source).
    pub lang: Option<String>,
    /// Source text (may be empty: `<lsource xml:lang="dut"/>`).
    pub text: String,
}

/// One `gloss`: a single translation string plus its language.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Gloss {
    /// `xml:lang` (absent = English).
    pub lang: Option<String>,
    /// `g_gend` attribute, when present.
    pub gender: Option<String>,
    /// `g_type` attribute (`lit`, `fig`, `expl`), when present.
    pub typ: Option<String>,
    /// Gloss text.
    pub text: String,
}

/// One JMnedict `trans`: name types plus transliterations.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Translation {
    /// Name-type tags (`name_type*`, entity refs kept raw,
    /// e.g. `&surname;`, `&place;`).
    pub name_types: Vec<String>,
    /// Transliterations (`trans_det*`).
    pub details: Vec<TransDetail>,
}

/// One `trans_det`: a single transliteration plus its language.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct TransDetail {
    /// `xml:lang` (absent = English).
    pub lang: Option<String>,
    /// Transliteration text.
    pub text: String,
}

/// Stream-parse a file, calling `emit` once per entry. Constant
/// memory regardless of file size. Returns the `<!-- ... created:
/// YYYY-MM-DD -->` header version when the stream carries one.
pub fn parse_file(path: &Path, emit: impl FnMut(Entry)) -> Result<Option<String>> {
    let file = std::fs::File::open(path)?;
    parse_reader(std::io::BufReader::new(file), emit)
}

/// Stream-parse any buffered reader, calling `emit` once per entry.
/// Returns the header version when the stream carries one.
pub fn parse_reader<R: BufRead>(reader: R, mut emit: impl FnMut(Entry)) -> Result<Option<String>> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    let mut version = None;
    parse_inner(&mut reader, &mut emit, &mut version)?;
    Ok(version)
}

/// Parse a small XML string, collecting entries (unit tests). A
/// header version, if any, is discarded (use [`parse_reader`] for it).
pub fn parse_str(xml: &str) -> Result<Vec<Entry>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut version = None;
    parse_inner(&mut reader, &mut |e| out.push(e), &mut version)?;
    Ok(out)
}

fn parse_inner<R: BufRead>(
    reader: &mut Reader<R>,
    emit: &mut dyn FnMut(Entry),
    version: &mut Option<String>,
) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == "entry" => {
                emit(read_entry(reader)?);
            }
            Ok(Event::Comment(ref e)) if version.is_none() => {
                // Header version (`<!-- JMdict created: YYYY-MM-DD -->`):
                // first `created:` wins, later comments are data.
                let text: &str = e;
                if let Some(rest) = text.split("created:").nth(1) {
                    if let Some(token) = rest.split_whitespace().next() {
                        if !token.is_empty() {
                            *version = Some(token.to_owned());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One `<entry>…</entry>` (reader sits just past `<entry>`).
fn read_entry<R: BufRead>(reader: &mut Reader<R>) -> Result<Entry> {
    let mut entry = Entry::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                if name.as_ref() == "ent_seq" {
                    entry.ent_seq = read_text(reader)?
                        .parse()
                        .map_err(|_| Error::Bad("ent_seq is not a number".into()))?;
                } else if name.as_ref() == "k_ele" {
                    entry.kanji.push(read_kanji(reader)?);
                } else if name.as_ref() == "r_ele" {
                    entry.readings.push(read_reading(reader)?);
                } else if name.as_ref() == "sense" {
                    entry.senses.push(read_sense(reader)?);
                } else if name.as_ref() == "trans" {
                    entry.translations.push(read_translation(reader)?);
                }
                // Anything else (info blocks, future tags): skipped —
                // the loop only reacts to known children.
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == "entry" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(entry)
}

/// One `<k_ele>…</k_ele>`.
fn read_kanji<R: BufRead>(reader: &mut Reader<R>) -> Result<KanjiElement> {
    let mut out = KanjiElement::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                if name.as_ref() == "keb" {
                    out.keb = read_text(reader)?;
                } else if name.as_ref() == "ke_inf" {
                    out.info.push(read_text(reader)?);
                } else if name.as_ref() == "ke_pri" {
                    out.priorities.push(read_text(reader)?);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == "k_ele" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(out)
}

/// One `<r_ele>…</r_ele>`.
fn read_reading<R: BufRead>(reader: &mut Reader<R>) -> Result<ReadingElement> {
    let mut out = ReadingElement::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => reading_leaf(reader, e, false, &mut out)?,
            Ok(Event::Empty(ref e)) => reading_leaf(reader, e, true, &mut out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "r_ele" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(out)
}

/// One leaf inside `<r_ele>`. `empty` mirrors Start vs Empty: an
/// empty tag carries no text, so reading past it would swallow the
/// next sibling's text.
fn reading_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut ReadingElement,
) -> Result<()> {
    let name = e.name();
    // Lazily read: unknown leaves must not consume anything, and
    // empty leaves have nothing to consume.
    let mut text = || {
        if empty {
            Ok(String::new())
        } else {
            read_text(reader)
        }
    };
    if name.as_ref() == "reb" {
        out.reb = text()?;
    } else if name.as_ref() == "re_nokanji" {
        out.nokanji = true;
    } else if name.as_ref() == "re_restr" {
        out.restricted_to.push(text()?);
    } else if name.as_ref() == "re_inf" {
        out.info.push(text()?);
    } else if name.as_ref() == "re_pri" {
        out.priorities.push(text()?);
    }
    Ok(())
}

/// One `<sense>…</sense>` (JMdict).
fn read_sense<R: BufRead>(reader: &mut Reader<R>) -> Result<Sense> {
    let mut out = Sense::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => sense_leaf(reader, e, false, &mut out)?,
            Ok(Event::Empty(ref e)) => sense_leaf(reader, e, true, &mut out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "sense" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(out)
}

/// One leaf inside `<sense>` (see [`reading_leaf`] for `empty`).
fn sense_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Sense,
) -> Result<()> {
    let name = e.name();
    let mut text = || {
        if empty {
            Ok(String::new())
        } else {
            read_text(reader)
        }
    };
    if name.as_ref() == "gloss" {
        out.glosses.push(Gloss {
            lang: attr(e, "xml:lang"),
            gender: attr(e, "g_gend"),
            typ: attr(e, "g_type"),
            text: text()?,
        });
    } else if name.as_ref() == "lsource" {
        out.sources.push(LangSource {
            lang: attr(e, "xml:lang"),
            text: text()?,
        });
    } else if name.as_ref() == "stagk" {
        out.stagk.push(text()?);
    } else if name.as_ref() == "stagr" {
        out.stagr.push(text()?);
    } else if name.as_ref() == "pos" {
        out.pos.push(text()?);
    } else if name.as_ref() == "xref" {
        out.xref.push(text()?);
    } else if name.as_ref() == "ant" {
        out.ant.push(text()?);
    } else if name.as_ref() == "field" {
        out.field.push(text()?);
    } else if name.as_ref() == "misc" {
        out.misc.push(text()?);
    } else if name.as_ref() == "s_inf" {
        out.info.push(text()?);
    }
    Ok(())
}

/// One `<trans>…</trans>` (JMnedict).
fn read_translation<R: BufRead>(reader: &mut Reader<R>) -> Result<Translation> {
    let mut out = Translation::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => trans_leaf(reader, e, false, &mut out)?,
            Ok(Event::Empty(ref e)) => trans_leaf(reader, e, true, &mut out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "trans" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(out)
}

/// One leaf inside `<trans>` (see [`reading_leaf`] for `empty`).
fn trans_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Translation,
) -> Result<()> {
    let name = e.name();
    let mut text = || {
        if empty {
            Ok(String::new())
        } else {
            read_text(reader)
        }
    };
    if name.as_ref() == "name_type" {
        out.name_types.push(text()?);
    } else if name.as_ref() == "trans_det" {
        out.details.push(TransDetail {
            lang: attr(e, "xml:lang"),
            text: text()?,
        });
    }
    Ok(())
}

/// Text content of the element just opened (consumes through its end
/// tag). Adjacent text runs concatenate. Entity refs (`&n;`,
/// `&amp;`, …) arrive as their own events and rebuild literally —
/// raw means raw, no expansion.
fn read_text<R: BufRead>(reader: &mut Reader<R>) -> Result<String> {
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                out.push_str(e);
            }
            // Entity refs (`&n;`, `&amp;`, …) arrive as their own
            // event, splitting the surrounding text. Rebuild the
            // literal spelling — raw means raw, no expansion.
            Ok(Event::GeneralRef(ref e)) => {
                let name: &str = e;
                if name.starts_with('&') {
                    out.push_str(name);
                } else {
                    out.push('&');
                    out.push_str(name);
                    out.push(';');
                }
            }
            Ok(Event::End(_)) => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(out)
}

/// String attribute of a start/empty tag (`None` when absent).
fn attr(e: &BytesStart<'_>, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.as_ref() == key {
            a.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                .ok()
                .map(|s| s.into_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<JMdict>
<entry>
<ent_seq>1000000</ent_seq>
<k_ele><keb>日本語</keb><ke_pri>ichi1</ke_pri></k_ele>
<r_ele><reb>にほんご</reb><re_pri>ichi1</re_pri></r_ele>
<sense><pos>&n;</pos><gloss>Japanese (language)</gloss><gloss xml:lang="deu">Japanisch</gloss></sense>
</entry>
</JMdict>"#;

    #[test]
    fn word_entry_with_priorities_and_lang_gloss() {
        let entries = parse_str(WORD).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.ent_seq, 1000000);
        assert_eq!(e.kanji.len(), 1);
        assert_eq!(e.kanji[0].keb, "日本語");
        assert_eq!(e.kanji[0].priorities, vec!["ichi1"]);
        assert_eq!(e.readings.len(), 1);
        assert_eq!(e.readings[0].reb, "にほんご");
        assert!(!e.readings[0].nokanji);
        assert_eq!(e.senses.len(), 1);
        // Entity refs survive literally (expansion is conversion's job).
        assert_eq!(e.senses[0].pos, vec!["&n;"]);
        assert_eq!(e.senses[0].glosses.len(), 2);
        assert_eq!(e.senses[0].glosses[0].lang, None);
        assert_eq!(e.senses[0].glosses[0].text, "Japanese (language)");
        assert_eq!(e.senses[0].glosses[1].lang.as_deref(), Some("deu"));
        assert!(e.translations.is_empty());
    }

    const KANA_ONLY: &str = r#"<JMdict><entry>
<ent_seq>1000010</ent_seq>
<r_ele><reb>あ</reb><re_nokanji/><re_restr>亜</re_restr><re_inf>&sk;</re_inf></r_ele>
<sense><stagk>亜</stagk><stagr>あ</stagr><pos>&n;</pos><xref>亜・1</xref><ant>非</ant>
<field>&comp;</field><misc>&uk;</misc><s_inf>usually kana</s_inf>
<lsource xml:lang="dut">bron</lsource><lsource xml:lang="por"/>
<gloss g_type="lit">second</gloss></sense>
</entry></JMdict>"#;

    #[test]
    fn kana_entry_with_restrictions_and_lsource() {
        let entries = parse_str(KANA_ONLY).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert!(e.kanji.is_empty());
        assert!(e.readings[0].nokanji);
        assert_eq!(e.readings[0].restricted_to, vec!["亜"]);
        assert_eq!(e.readings[0].info, vec!["&sk;"]);
        let s = &e.senses[0];
        assert_eq!(s.stagk, vec!["亜"]);
        assert_eq!(s.stagr, vec!["あ"]);
        assert_eq!(s.xref, vec!["亜・1"]);
        assert_eq!(s.ant, vec!["非"]);
        assert_eq!(s.field, vec!["&comp;"]);
        assert_eq!(s.misc, vec!["&uk;"]);
        assert_eq!(s.info, vec!["usually kana"]);
        // lsource with text and empty lsource (lang only).
        assert_eq!(s.sources.len(), 2);
        assert_eq!(s.sources[0].lang.as_deref(), Some("dut"));
        assert_eq!(s.sources[0].text, "bron");
        assert_eq!(s.sources[1].lang.as_deref(), Some("por"));
        assert_eq!(s.sources[1].text, "");
        assert_eq!(s.glosses[0].typ.as_deref(), Some("lit"));
        assert_eq!(s.glosses[0].text, "second");
    }

    const NAME: &str = r#"<JMnedict><entry>
<ent_seq>5000000</ent_seq>
<k_ele><keb>田中</keb></k_ele>
<r_ele><reb>たなか</reb></r_ele>
<trans><name_type>&surname;</name_type><trans_det>Tanaka</trans_det><trans_det xml:lang="fre">Tanaka (nom)</trans_det></trans>
</entry></JMnedict>"#;

    #[test]
    fn name_entry_with_translations() {
        let entries = parse_str(NAME).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.ent_seq, 5000000);
        assert!(e.senses.is_empty());
        assert_eq!(e.translations.len(), 1);
        assert_eq!(e.translations[0].name_types, vec!["&surname;"]);
        assert_eq!(e.translations[0].details.len(), 2);
        assert_eq!(e.translations[0].details[0].text, "Tanaka");
        assert_eq!(e.translations[0].details[0].lang, None);
        assert_eq!(e.translations[0].details[1].lang.as_deref(), Some("fre"));
    }

    #[test]
    fn header_version_comes_out_of_the_parse() {
        let xml = "<!-- JMdict created: 2026-10-10 --><JMdict><entry><ent_seq>1</ent_seq>\
            <r_ele><reb>あ</reb></r_ele></entry></JMdict>";
        let version = parse_reader(xml.as_bytes(), |_| {}).unwrap();
        assert_eq!(version.as_deref(), Some("2026-10-10"));
    }

    #[test]
    fn missing_header_version_is_none() {
        let version = parse_reader(WORD.as_bytes(), |_| {}).unwrap();
        assert_eq!(version, None);
    }

    #[test]
    fn multiple_entries_and_unknown_tags() {
        // Unknown/future tags are skipped, never fatal.
        let entries = parse_str(
            r#"<JMdict><entry><ent_seq>1</ent_seq><r_ele><reb>あ</reb></r_ele>
            <sense><pos>&n;</pos><gloss>a</gloss></sense><future_tag><nested>x</nested></future_tag></entry>
            <entry><ent_seq>2</ent_seq><r_ele><reb>い</reb></r_ele></entry></JMdict>"#,
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].ent_seq, 1);
        assert_eq!(entries[1].readings[0].reb, "い");
        assert!(entries[1].senses.is_empty());
    }
}
