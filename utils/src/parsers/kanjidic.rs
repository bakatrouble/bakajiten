//! Raw streaming parser for `kanjidic2.xml`.
//!
//! One `<character>` per kanji: literal, codepoints, radicals, misc
//! data (grade/strokes/frequency/JLPT), dictionary references, query
//! codes, readings, and meanings. Attribute metadata (`cp_type`,
//! `dr_type`, `m_vol`/`m_page`, `skip_misgrade`, `var_type`) is kept
//! alongside the text — nothing is interpreted beyond splitting text
//! from tags. Unknown elements are skipped.
//!
//! Entry points: [`parse_file`] (stream from disk with a callback),
//! [`parse_str`] (small inputs, collects — tests), [`creation_date`]
//! (the `<date_of_creation>` header, used as the source version).

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::io::BufRead;
use std::path::Path;

use super::error::{Error, Result};
use dict::kanji::{
    Deroo, FourCorner, KanjiRecord, Kuten, Nelson, Njecd, Oneill, ShDesc, Skip, Stroke,
};

/// One kanjidic2 `<character>`.
#[derive(Debug, Clone, Default)]
pub struct Character {
    /// The kanji itself (`literal`).
    pub literal: String,
    /// Codepoints (`cp_value*` with `cp_type`, e.g. `ucs`, `jis208`).
    pub codepoints: Vec<Codepoint>,
    /// Radical indexes (`rad_value*` with `rad_type`).
    pub radicals: Vec<Radical>,
    /// School grade (`grade*`, usually one).
    pub grade: Option<u32>,
    /// Stroke counts (`stroke_count*`, first is canonical, rest are
    /// commonly-seen miscounts).
    pub strokes: Vec<u32>,
    /// Frequency rank (`freq*`, usually one).
    pub freq: Option<u32>,
    /// Former JLPT level (`jlpt*`, usually one).
    pub jlpt: Option<u32>,
    /// Variants (`variant*` with `var_type`).
    pub variants: Vec<Variant>,
    /// Radical names (`rad_name*`).
    pub rad_names: Vec<String>,
    /// Dictionary references (`dic_ref*`).
    pub dic_refs: Vec<DicRef>,
    /// Query codes (`q_code*`, e.g. SKIP, four-corner).
    pub query_codes: Vec<QueryCode>,
    /// Readings (`reading*` inside `rmgroup*`).
    pub readings: Vec<Reading>,
    /// Meanings (`meaning*` inside `rmgroup*`; no `m_lang` = English).
    pub meanings: Vec<Meaning>,
    /// Nanori readings (`nanori*`).
    pub nanori: Vec<String>,
}

/// One `cp_value`: codepoint in a given coding (`cp_type`).
#[derive(Debug, Clone, Default)]
pub struct Codepoint {
    /// Coding (`cp_type`, e.g. `ucs`, `jis208`, `jis212`, `jis213`).
    pub typ: String,
    /// Codepoint value text (hex for `ucs`, `kuten`-style for JIS).
    pub value: String,
}

/// One `rad_value`: radical index in a given scheme (`rad_type`).
#[derive(Debug, Clone, Default)]
pub struct Radical {
    /// Scheme (`rad_type`, e.g. `classical`, `nelson_c`).
    pub typ: String,
    /// Radical number.
    pub value: u32,
}

/// One `variant`: an alternate form plus its relation (`var_type`,
/// e.g. `jis208`, `nelson_c`, `sps`).
#[derive(Debug, Clone, Default)]
pub struct Variant {
    pub typ: String,
    pub text: String,
}

/// One `dic_ref`: index into a paper dictionary.
#[derive(Debug, Clone, Default)]
pub struct DicRef {
    /// Dictionary (`dr_type`, e.g. `nelson_c`, `halpern_njecd`).
    pub typ: String,
    /// Index text (may carry suffixes like `135P`).
    pub text: String,
    /// `m_vol` (Morohashi volume), when present.
    pub volume: Option<u32>,
    /// `m_page` (Morohashi page), when present.
    pub page: Option<u32>,
}

/// One `q_code`: lookup-system code (SKIP, four-corner, …).
#[derive(Debug, Clone, Default)]
pub struct QueryCode {
    /// System (`qc_type`, e.g. `skip`, `four_corner`, `deroo`).
    pub typ: String,
    /// Code text.
    pub text: String,
    /// `skip_misgrade="true"` (SKIP code is a known miscount).
    pub misgrade: bool,
}

/// One `reading`: kana reading of a given type.
#[derive(Debug, Clone, Default)]
pub struct Reading {
    /// `r_type` (`ja_on`, `ja_kun`, …; absent on malformed input).
    pub typ: Option<String>,
    /// Reading text (kana, `.` marks okurigana splits in kun).
    pub text: String,
}

/// One `meaning`: gloss in some language.
#[derive(Debug, Clone, Default)]
pub struct Meaning {
    /// `m_lang` (absent = English).
    pub lang: Option<String>,
    /// Meaning text.
    pub text: String,
}

/// Stream-parse a file, calling `emit` once per character. Constant
/// memory regardless of file size.
pub fn parse_file(path: &Path, emit: impl FnMut(Character)) -> Result<()> {
    let file = std::fs::File::open(path)?;
    parse_reader(std::io::BufReader::new(file), emit)
}

/// Read the `<date_of_creation>` header (the source version stamp).
pub fn creation_date(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = Reader::from_reader(std::io::BufReader::new(file));
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().into_inner() == "date_of_creation" => {
                return read_text(&mut reader);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Err(Error::Bad("kanjidic2: no <date_of_creation> found".into()))
}

/// Build a [`KanjiRecord`] from a parsed character plus its SVG
/// strokes. Malformed variant-tagged values are skipped (the pack
/// tool counts schema drift separately); unparseable readings and
/// codes never enter the record.
pub fn to_record(character: &Character, strokes: Vec<Stroke>, parts: &[char]) -> KanjiRecord {
    let mut record = KanjiRecord::bare(
        character.literal.chars().next().unwrap_or('\u{FFFD}'),
        strokes,
    );
    for r in &character.readings {
        let slot = match r.typ.as_deref() {
            Some("ja_kun") => &mut record.kun,
            Some("ja_on") => &mut record.on,
            Some("pinyin") => &mut record.chinese,
            Some("korean_r") | Some("korean_h") => &mut record.korean,
            Some("vietnam") => &mut record.vietnamese,
            _ => continue,
        };
        slot.push(r.text.clone());
    }
    for m in &character.meanings {
        if m.lang.is_none() {
            record.meanings_en.push(m.text.clone());
        }
    }
    for q in &character.query_codes {
        match q.typ.as_str() {
            "skip" => {
                if let Some(code) = Skip::parse(&q.text) {
                    record.skip.push(code);
                }
                if q.misgrade {
                    record.skip_misgrade = true;
                }
            }
            "four_corner" => {
                if let Some(code) = FourCorner::parse(&q.text) {
                    record.four_corner.push(code);
                }
            }
            "deroo" => {
                if let Some(code) = Deroo::parse(&q.text) {
                    record.deroo.push(code);
                }
            }
            "sh_desc" => {
                if let Some(code) = ShDesc::parse(&q.text) {
                    record.sh_desc.push(code);
                }
            }
            _ => {}
        }
    }
    for v in &character.variants {
        match v.typ.as_str() {
            "jis208" => {
                if let Some(code) = Kuten::parse(&v.text) {
                    record.jis208.push(code);
                }
            }
            "jis212" => {
                if let Some(code) = Kuten::parse(&v.text) {
                    record.jis212.push(code);
                }
            }
            "jis213" => {
                if let Some(code) = Kuten::parse(&v.text) {
                    record.jis213.push(code);
                }
            }
            "deroo" => {
                if let Some(code) = Deroo::parse(&v.text) {
                    record.deroo_variant.push(code);
                }
            }
            "nelson_c" => {
                if let Some(code) = Nelson::parse(&v.text) {
                    record.nelson_c.push(code);
                }
            }
            "njecd" => {
                if let Some(code) = Njecd::parse(&v.text) {
                    record.njecd.push(code);
                }
            }
            "oneill" => {
                if let Some(code) = Oneill::parse(&v.text) {
                    record.oneill.push(code);
                }
            }
            "s_h" => record.s_h.push(v.text.clone()),
            _ => {}
        }
    }
    record.grade = character.grade;
    record.jlpt = character.jlpt;
    record.freq = character.freq;
    record.parts = parts.to_vec();
    // Own JIS X 0208 codepoint (`plane-ku-ten`, e.g. `1-24-62`):
    // the first well-formed plane-1 one wins, like
    // strokes/freq/jlpt. Anything else is a different coding and
    // must not render as X 0208.
    record.jis208_code = character
        .codepoints
        .iter()
        .filter(|c| c.typ == "jis208")
        .filter_map(|c| {
            let mut it = c.value.split('-');
            let plane: u8 = it.next()?.parse().ok()?;
            let ku: u8 = it.next()?.parse().ok()?;
            let ten: u8 = it.next()?.parse().ok()?;
            (plane == 1 && it.next().is_none()).then_some([ku, ten])
        })
        .next();
    // Classical Kangxi number only (nelson_c is a different
    // scheme); first wins, like strokes/freq/jlpt.
    record.radical = character
        .radicals
        .iter()
        .find(|r| r.typ == "classical")
        .map(|r| r.value);
    record
}

/// Stream-parse any buffered reader, calling `emit` once per character.
pub fn parse_reader<R: BufRead>(reader: R, mut emit: impl FnMut(Character)) -> Result<()> {
    let mut reader = Reader::from_reader(reader);
    reader.config_mut().trim_text(true);
    parse_inner(&mut reader, &mut emit)
}

/// Parse a small XML string, collecting characters (unit tests).
pub fn parse_str(xml: &str) -> Result<Vec<Character>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    parse_inner(&mut reader, &mut |c| out.push(c))?;
    Ok(out)
}

fn parse_inner<R: BufRead>(reader: &mut Reader<R>, emit: &mut dyn FnMut(Character)) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == "character" => {
                emit(read_character(reader)?);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One `<character>…</character>` (reader sits just past `<character>`).
fn read_character<R: BufRead>(reader: &mut Reader<R>) -> Result<Character> {
    let mut out = Character::default();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                if name.as_ref() == "literal" {
                    out.literal = read_text(reader)?;
                } else if name.as_ref() == "codepoint" {
                    read_codepoint(reader, &mut out)?;
                } else if name.as_ref() == "radical" {
                    read_radical(reader, &mut out)?;
                } else if name.as_ref() == "misc" {
                    read_misc(reader, &mut out)?;
                } else if name.as_ref() == "dic_number" {
                    read_dic_number(reader, &mut out)?;
                } else if name.as_ref() == "query_code" {
                    read_query_code(reader, &mut out)?;
                } else if name.as_ref() == "reading_meaning" {
                    read_reading_meaning(reader, &mut out)?;
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == "character" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(out)
}

/// `<codepoint>` children: `cp_value*` with `cp_type`.
fn read_codepoint<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => codepoint_leaf(reader, e, false, out)?,
            Ok(Event::Empty(ref e)) => codepoint_leaf(reader, e, true, out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "codepoint" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One leaf inside `<codepoint>` (empty tags carry no text — see
/// `reading_leaf` in `jmdict.rs` for why the split matters).
fn codepoint_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Character,
) -> Result<()> {
    if e.name().as_ref() == "cp_value" {
        out.codepoints.push(Codepoint {
            typ: attr(e, "cp_type").unwrap_or_default(),
            value: leaf_text(reader, empty)?,
        });
    }
    Ok(())
}

/// `<radical>` children: `rad_value*` with `rad_type`.
fn read_radical<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => radical_leaf(reader, e, false, out)?,
            Ok(Event::Empty(ref e)) => radical_leaf(reader, e, true, out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "radical" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One leaf inside `<radical>`. An empty tag carries no number, so
/// there is nothing to push; anything else must parse.
fn radical_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Character,
) -> Result<()> {
    if e.name().as_ref() == "rad_value" && !empty {
        out.radicals.push(Radical {
            typ: attr(e, "rad_type").unwrap_or_default(),
            value: leaf_text(reader, empty)?
                .parse()
                .map_err(|_| Error::Bad("rad_value is not a number".into()))?,
        });
    }
    Ok(())
}

/// `<misc>` children: grade, strokes, variants, freq, names, jlpt.
fn read_misc<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => misc_leaf(reader, e, false, out)?,
            Ok(Event::Empty(ref e)) => misc_leaf(reader, e, true, out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "misc" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One leaf inside `<misc>`. Numeric leaves on empty tags are
/// skipped (no number to parse); variants carry attributes either way.
fn misc_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Character,
) -> Result<()> {
    let name = e.name();
    if name.as_ref() == "grade" && !empty {
        out.grade = Some(
            leaf_text(reader, empty)?
                .parse()
                .map_err(|_| Error::Bad("grade is not a number".into()))?,
        );
    } else if name.as_ref() == "stroke_count" && !empty {
        out.strokes.push(
            leaf_text(reader, empty)?
                .parse()
                .map_err(|_| Error::Bad("stroke_count is not a number".into()))?,
        );
    } else if name.as_ref() == "variant" {
        out.variants.push(Variant {
            typ: attr(e, "var_type").unwrap_or_default(),
            text: leaf_text(reader, empty)?,
        });
    } else if name.as_ref() == "freq" && !empty {
        out.freq = Some(
            leaf_text(reader, empty)?
                .parse()
                .map_err(|_| Error::Bad("freq is not a number".into()))?,
        );
    } else if name.as_ref() == "rad_name" {
        out.rad_names.push(leaf_text(reader, empty)?);
    } else if name.as_ref() == "jlpt" && !empty {
        out.jlpt = Some(
            leaf_text(reader, empty)?
                .parse()
                .map_err(|_| Error::Bad("jlpt is not a number".into()))?,
        );
    }
    Ok(())
}

/// `<dic_number>` children: `dic_ref*` with `dr_type` (+ Morohashi
/// `m_vol`/`m_page` on the matching refs).
fn read_dic_number<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => dic_leaf(reader, e, false, out)?,
            Ok(Event::Empty(ref e)) => dic_leaf(reader, e, true, out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "dic_number" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One leaf inside `<dic_number>`.
fn dic_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Character,
) -> Result<()> {
    if e.name().as_ref() == "dic_ref" {
        out.dic_refs.push(DicRef {
            typ: attr(e, "dr_type").unwrap_or_default(),
            text: leaf_text(reader, empty)?,
            volume: attr_num(e, "m_vol"),
            page: attr_num(e, "m_page"),
        });
    }
    Ok(())
}

/// `<query_code>` children: `q_code*` with `qc_type` (+ SKIP
/// `skip_misgrade`).
fn read_query_code<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => qcode_leaf(reader, e, false, out)?,
            Ok(Event::Empty(ref e)) => qcode_leaf(reader, e, true, out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "query_code" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One leaf inside `<query_code>`.
fn qcode_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Character,
) -> Result<()> {
    if e.name().as_ref() == "q_code" {
        out.query_codes.push(QueryCode {
            typ: attr(e, "qc_type").unwrap_or_default(),
            text: leaf_text(reader, empty)?,
            misgrade: attr(e, "skip_misgrade").as_deref() == Some("true"),
        });
    }
    Ok(())
}

/// `<reading_meaning>` children: `rmgroup*` (readings + meanings) and
/// `nanori*`.
fn read_reading_meaning<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                if name.as_ref() == "rmgroup" {
                    read_rmgroup(reader, out)?;
                } else if name.as_ref() == "nanori" {
                    out.nanori.push(read_text(reader)?);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == "reading_meaning" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One `rmgroup`: `reading*` (with `r_type`) and `meaning*` (with
/// optional `m_lang`).
fn read_rmgroup<R: BufRead>(reader: &mut Reader<R>, out: &mut Character) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => rmgroup_leaf(reader, e, false, out)?,
            Ok(Event::Empty(ref e)) => rmgroup_leaf(reader, e, true, out)?,
            Ok(Event::End(ref e)) if e.name().as_ref() == "rmgroup" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
    }
    Ok(())
}

/// One leaf inside `rmgroup`.
fn rmgroup_leaf<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
    empty: bool,
    out: &mut Character,
) -> Result<()> {
    let name = e.name();
    if name.as_ref() == "reading" {
        out.readings.push(Reading {
            typ: attr(e, "r_type"),
            text: leaf_text(reader, empty)?,
        });
    } else if name.as_ref() == "meaning" {
        out.meanings.push(Meaning {
            lang: attr(e, "m_lang"),
            text: leaf_text(reader, empty)?,
        });
    }
    Ok(())
}

/// Text content of a leaf element: empty tags yield `""` without
/// consuming anything; start tags read through the end tag. Entity
/// refs rebuild literally (see `read_text`).
fn leaf_text<R: BufRead>(reader: &mut Reader<R>, empty: bool) -> Result<String> {
    if empty {
        Ok(String::new())
    } else {
        read_text(reader)
    }
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

/// Numeric attribute (`None` when absent or not a number).
fn attr_num(e: &BytesStart<'_>, key: &str) -> Option<u32> {
    attr(e, key)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KANJI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kanjidic2>
<character>
<literal>亜</literal>
<codepoint><cp_value cp_type="ucs">4e9c</cp_value><cp_value cp_type="jis208">1-16-01</cp_value></codepoint>
<radical><rad_value rad_type="classical">7</rad_value><rad_value rad_type="nelson_c">2</rad_value></radical>
<misc><grade>8</grade><stroke_count>7</stroke_count><stroke_count>8</stroke_count>
<variant var_type="jis208">亞</variant><freq>1509</freq><rad_name>二</rad_name><jlpt>1</jlpt></misc>
<dic_number><dic_ref dr_type="nelson_c">43</dic_ref>
<dic_ref dr_type="moro" m_vol="1" m_page="0291">1024</dic_ref></dic_number>
<query_code><q_code qc_type="skip" skip_misgrade="true">1-2-5</q_code>
<q_code qc_type="four_corner">1010.6</q_code></query_code>
<reading_meaning><rmgroup>
<reading r_type="ja_on">ア</reading><reading r_type="ja_kun">つ.ぐ</reading>
<meaning>Asia</meaning><meaning m_lang="fr">Asie</meaning>
</rmgroup><nanori>つぎ</nanori></reading_meaning>
</character>
</kanjidic2>"#;

    #[test]
    fn full_character() {
        let chars = parse_str(KANJI).unwrap();
        assert_eq!(chars.len(), 1);
        let c = &chars[0];
        assert_eq!(c.literal, "亜");
        assert_eq!(c.codepoints.len(), 2);
        assert_eq!(
            (c.codepoints[0].typ.as_str(), c.codepoints[0].value.as_str()),
            ("ucs", "4e9c")
        );
        assert_eq!(
            (c.codepoints[1].typ.as_str(), c.codepoints[1].value.as_str()),
            ("jis208", "1-16-01")
        );
        assert_eq!(c.radicals.len(), 2);
        assert_eq!(c.radicals[0].value, 7);
        assert_eq!(c.grade, Some(8));
        // First stroke count is canonical, rest are miscounts.
        assert_eq!(c.strokes, vec![7, 8]);
        assert_eq!(c.freq, Some(1509));
        assert_eq!(c.jlpt, Some(1));
        assert_eq!(c.variants.len(), 1);
        assert_eq!(c.variants[0].typ, "jis208");
        assert_eq!(c.rad_names, vec!["二"]);
        assert_eq!(c.dic_refs.len(), 2);
        assert_eq!(c.dic_refs[0].typ, "nelson_c");
        assert_eq!(c.dic_refs[0].text, "43");
        assert_eq!(c.dic_refs[0].volume, None);
        assert_eq!(c.dic_refs[1].typ, "moro");
        assert_eq!(c.dic_refs[1].volume, Some(1));
        assert_eq!(c.dic_refs[1].page, Some(291));
        assert_eq!(c.query_codes.len(), 2);
        assert!(c.query_codes[0].misgrade);
        assert_eq!(c.query_codes[0].text, "1-2-5");
        assert!(!c.query_codes[1].misgrade);
        assert_eq!(c.readings.len(), 2);
        assert_eq!(c.readings[0].typ.as_deref(), Some("ja_on"));
        assert_eq!(c.readings[1].text, "つ.ぐ");
        assert_eq!(c.meanings.len(), 2);
        // No m_lang = English.
        assert_eq!(c.meanings[0].lang, None);
        assert_eq!(c.meanings[0].text, "Asia");
        assert_eq!(c.meanings[1].lang.as_deref(), Some("fr"));
        assert_eq!(c.nanori, vec!["つぎ"]);
    }

    #[test]
    fn record_from_character() {
        use dict::kanji::{Point, Stroke};

        let chars = parse_str(KANJI).unwrap();
        let strokes = vec![Stroke::new(vec![Point::new(10.0, 20.0)])];
        let record = to_record(&chars[0], strokes.clone(), &['｜', '一', '口']);
        assert_eq!(record.character, '亜');
        assert_eq!(record.strokes, strokes);
        assert_eq!(record.on, vec!["ア"]);
        assert_eq!(record.kun, vec!["つ.ぐ"]);
        assert_eq!(record.meanings_en, vec!["Asia"]);
        assert_eq!(record.skip.len(), 1);
        assert!(record.skip_misgrade);
        assert_eq!(record.four_corner.len(), 1);
        assert_eq!(record.grade, Some(8));
        // Classical radical only (nelson_c is another scheme).
        assert_eq!(record.radical, Some(7));
        // `jis208` variant here names a character, not kuten: skipped.
        assert!(record.jis208.is_empty());
        // kradfile parts ride along verbatim; own JIS kuten parses.
        assert_eq!(record.parts, vec!['｜', '一', '口']);
        assert_eq!(record.jis208_code, Some([16, 1]));
    }

    #[test]
    fn sparse_character_with_unknown_tags() {
        // Missing optional sections and future tags must not fail.
        let chars = parse_str(
            r#"<kanjidic2><character><literal>一</literal>
            <misc><stroke_count>1</stroke_count></misc>
            <reading_meaning><rmgroup><reading r_type="ja_kun">ひと</reading></rmgroup></reading_meaning>
            <future_section><x>1</x></future_section></character></kanjidic2>"#,
        )
        .unwrap();
        assert_eq!(chars.len(), 1);
        let c = &chars[0];
        assert_eq!(c.literal, "一");
        assert_eq!(c.grade, None);
        assert_eq!(c.strokes, vec![1]);
        assert!(c.meanings.is_empty());
        assert!(c.nanori.is_empty());
    }
}
