//! KanjiVG parsing (build-time only).
//!
//! KanjiVG (https://kanjivg.tagaini.net) ships one SVG per character,
//! e.g. `00001.svg` containing `<g id="kvg:00001">` with one `<path>`
//! per stroke, plus an aggregate `kanjivg.xml` dump. Both layouts are
//! supported by [`load_path`].
//!
//! Only the `d` attribute of each `<path>` is interpreted. M/L/C/Q
//! segments are flattened by sampling; everything else is skipped.
//!
//! This module exists solely to build the recognition model file
//! (see [`recognizer::model`]) from a KanjiVG checkout. Runtime
//! recognition never touches XML: the UI loads only the packed model
//! (characters + strokes, no ids or stroke-type metadata).

use super::error::{Error, Result};
use dict::kanji::{Point, Stroke};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::path::Path;

/// One KanjiVG character: Unicode codepoint + ordered strokes.
///
/// Build-time intermediate — only `character` + `strokes` survive into
/// the model file. `id` orders entries deterministically (and decodes
/// the codepoint when no `kvg:element` label exists). The per-stroke
/// `kvg:type` nomenclature is ignored: the matcher never sees it.
#[derive(Debug, Clone)]
pub struct KanjiVgEntry {
    /// Unicode character, if the id/element encodes one.
    pub character: Option<char>,
    /// KanjiVG id, e.g. `00041` or `04e00`.
    pub id: String,
    /// Ordered pen strokes in 0..255 pen space (scaled from the
    /// native 109x109 at parse time).
    pub strokes: Vec<Stroke>,
}

impl KanjiVgEntry {
    pub fn new(character: Option<char>, id: String, strokes: Vec<Stroke>) -> Self {
        Self {
            character,
            id,
            strokes,
        }
    }

    /// True for the canonical file of its character: a bare glyph id
    /// (`04e00`, not `04e00-Kaisho`) that decodes to the entry's own
    /// character. Rejects style/order variants (`-Kaisho`, `-VtLst`,
    /// `-HzFst`, …) and mislabeled files (`06a45.svg` carries 機).
    pub fn is_canonical(&self) -> bool {
        let Some(ch) = self.character else {
            return false;
        };
        is_bare_glyph_id(&self.id) && id_to_char(&self.id) == Some(ch)
    }
}

/// Load every entry found at `path`: either a directory of `*.svg` files
/// or a single `kanjivg.xml` / `*.svg` / `*.xml` file. Calls
/// `on_file` once per file parsed (progress hook; use [`load_path`]
/// to ignore it).
pub fn load_path(path: &Path) -> Result<Vec<KanjiVgEntry>> {
    load_path_with(path, &mut || {})
}

/// [`load_path`] with a per-file progress callback.
pub fn load_path_with(path: &Path, on_file: &mut dyn FnMut()) -> Result<Vec<KanjiVgEntry>> {
    if path.is_dir() {
        load_dir_with(path, on_file)
    } else {
        let text = std::fs::read_to_string(path)?;
        on_file();
        parse_kanjivg_xml(&text)
    }
}

/// Load a directory of per-character `*.svg` files.
pub fn load_dir(dir: &Path) -> Result<Vec<KanjiVgEntry>> {
    load_dir_with(dir, &mut || {})
}

/// [`load_dir`] with a per-file progress callback.
pub fn load_dir_with(dir: &Path, on_file: &mut dyn FnMut()) -> Result<Vec<KanjiVgEntry>> {
    let mut out = Vec::new();
    let rd = std::fs::read_dir(dir)?;
    let mut files: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    files.sort_by_key(|e| e.file_name());
    for f in files {
        let p = f.path();
        if p.extension().and_then(|e| e.to_str()) != Some("svg") {
            continue;
        }
        let text = std::fs::read_to_string(&p)?;
        on_file();
        // Each file holds exactly one character; parse leniently and keep all.
        for e in parse_kanjivg_xml(&text)? {
            out.push(e);
        }
    }
    Ok(out)
}

/// Parse KanjiVG XML text (single glyph or full dump) into entries.
pub fn parse_kanjivg_xml(xml: &str) -> Result<Vec<KanjiVgEntry>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    // char id -> list of (stroke_order, d_attr). Stroke order
    // is document order unless the path id ends with `-sN`.
    let mut glyphs: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    // char id -> kvg:element character seen on its <g> (authoritative label;
    // covers old decimal-serial files like 00021.svg whose id is not a codepoint).
    let mut elements: BTreeMap<String, char> = BTreeMap::new();
    let mut order_counter: BTreeMap<String, usize> = BTreeMap::new();
    // stack of enclosing <g>/<kanji> ids (bare id without "kvg:" prefix)
    let mut gstack: Vec<Option<String>> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                let name = name.as_ref();
                if name == "g" || name == "kanji" {
                    let mut gid: Option<String> = None;
                    let mut element: Option<String> = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            "id" => {
                                let v = attr
                                    .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                                    .map(|s| s.into_owned())
                                    .unwrap_or_default();
                                // ids look like "kvg:04e00" or "kvg:04e00-g1"
                                if let Some(stripped) = v.strip_prefix("kvg:") {
                                    gid = Some(stripped.to_string());
                                }
                            }
                            "kvg:element" => {
                                element = attr
                                    .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                                    .map(|s| s.into_owned())
                                    .ok();
                            }
                            _ => {}
                        }
                    }
                    if let (Some(g), Some(el)) = (gid.clone(), element) {
                        // Record single-character element labels on bare glyph
                        // groups only (skip StrokePaths_/StrokeNumbers_ wrappers
                        // and -gN sub-component groups).
                        if is_bare_glyph_id(&g) {
                            let mut chars = el.chars();
                            if let (Some(ch), None) = (chars.next(), chars.next()) {
                                elements.entry(g).or_insert(ch);
                            }
                        }
                    }
                    if e.is_empty() {
                        // empty <g/> : nothing to push
                    } else {
                        gstack.push(gid);
                    }
                } else if name == "path" {
                    let mut d: Option<String> = None;
                    let mut pid: Option<String> = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            "d" => {
                                d = attr
                                    .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                                    .map(|s| s.into_owned())
                                    .ok();
                            }
                            "id" => {
                                pid = attr
                                    .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                                    .map(|s| s.into_owned())
                                    .ok();
                            }
                            _ => {}
                        }
                    }
                    if let Some(d) = d {
                        // Owning glyph = nearest enclosing bare glyph id
                        // (skips StrokePaths_/StrokeNumbers_ wrappers and
                        // -gN sub-component groups), else path id prefix.
                        let mut owner: Option<String> = None;
                        for g in gstack.iter().rev().flatten() {
                            if is_bare_glyph_id(g) {
                                owner = Some(g.clone());
                                break;
                            }
                        }
                        if owner.is_none() {
                            if let Some(pid) = &pid {
                                if let Some(stripped) = pid.strip_prefix("kvg:") {
                                    let base = stripped.split("-s").next().unwrap_or(stripped);
                                    owner = Some(base.to_string());
                                }
                            }
                        }
                        if let Some(owner) = owner {
                            let order =
                                stroke_order_from_path_id(pid.as_deref()).unwrap_or_else(|| {
                                    let c = order_counter.entry(owner.clone()).or_insert(0);
                                    *c += 1;
                                    *c
                                });
                            glyphs.entry(owner).or_default().push((order, d));
                        }
                    }
                }
                // Empty events need no stack handling; Start already pushed.
                if e.is_empty() {
                    // nothing
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == "g" || e.name().as_ref() == "kanji" {
                    gstack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    let mut entries = Vec::new();
    for (id, mut strokes) in glyphs {
        strokes.sort_by_key(|(o, _)| *o);
        let mut parsed = Vec::new();
        for (_, d) in strokes {
            if let Some(s) = parse_path_d(&d) {
                if !s.is_empty() {
                    // KanjiVG's native 109-box → pen space (0..255).
                    parsed.push(Stroke::new(
                        s.0.iter()
                            .map(|p| Point::new(p.x * 255.0 / 109.0, p.y * 255.0 / 109.0))
                            .collect(),
                    ));
                }
            }
        }
        if parsed.is_empty() {
            continue;
        }
        // Prefer the kvg:element label recorded on the glyph group (works
        // for old decimal-serial ids too); fall back to hex id decoding.
        let character = elements.get(&id).copied().or_else(|| id_to_char(&id));
        entries.push(KanjiVgEntry::new(character, id, parsed));
    }
    // Deterministic order by id for reproducible label maps.
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(entries)
}

/// True for bare glyph group ids: `04e00`, `00021` (no StrokePaths_/
/// StrokeNumbers_ wrappers, no -gN sub-component suffix, no -sN stroke suffix).
fn is_bare_glyph_id(id: &str) -> bool {
    if id.contains('_') || id.contains("-g") || id.contains("-s") {
        return false;
    }
    let base = id.split('-').next().unwrap_or(id);
    // Hex codepoints (04e00) or decimal serials (00021): 4-6 alnum chars.
    (4..=6).contains(&base.len()) && base.chars().all(|c| c.is_ascii_hexdigit())
}

/// Extract trailing `-sN` stroke number from a path id like `kvg:04e00-s3`.
fn stroke_order_from_path_id(pid: Option<&str>) -> Option<usize> {
    let pid = pid?;
    let after = pid.rsplit("-s").next()?;
    // make sure there really was a "-s" separator
    if !pid.contains("-s") {
        return None;
    }
    after.parse::<usize>().ok()
}

/// Map a KanjiVG id to a char when it encodes a Unicode codepoint.
/// Accepts 4-6 hex digits (`04e00` -> U+4E00). Decimal ids like `00001`
/// (old JIS-style serials) return None.
fn id_to_char(id: &str) -> Option<char> {
    let base = id.split('-').next().unwrap_or(id);
    if base.len() < 4 || base.len() > 6 || !base.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // Heuristic: KanjiVG codepoint ids are lowercase hex in practice;
    // all-decimal 5-digit serials (00001..) are not codepoints.
    if base.len() == 5 && base.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    u32::from_str_radix(base, 16).ok().and_then(char::from_u32)
}

/// Parse an SVG path `d` string into a polyline by sampling curves.
///
/// Supports M/L/H/V/C/S/Q/T/A (A approximated as line to endpoint) and
/// relative variants. Numbers may be comma/space separated, including
/// compact forms like `M10-20` (handled via tokenizer).
pub fn parse_path_d(d: &str) -> Option<Stroke> {
    let tokens = tokenize_path(d);
    if tokens.is_empty() {
        return None;
    }
    let mut pts: Vec<Point> = Vec::new();
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut sx = 0.0f32; // subpath start
    let mut sy = 0.0f32;
    let mut i = 0;
    let mut cmd = 'M';

    while i < tokens.len() {
        let t = &tokens[i];
        if t.len() == 1 && t.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            cmd = t.chars().next().unwrap();
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                // close: line back to subpath start
                pts.push(Point::new(sx, sy));
                cx = sx;
                cy = sy;
            }
            continue;
        }
        let rel = cmd.is_ascii_lowercase();
        match cmd.to_ascii_uppercase() {
            'M' => {
                if i + 1 >= tokens.len() {
                    break;
                }
                let (x, y) = (num(&tokens[i]), num(&tokens[i + 1]));
                i += 2;
                let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                pts.push(Point::new(x, y));
                // subsequent implicit pairs are linetos
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                if i + 1 >= tokens.len() {
                    break;
                }
                let (x, y) = (num(&tokens[i]), num(&tokens[i + 1]));
                i += 2;
                let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                cx = x;
                cy = y;
                pts.push(Point::new(x, y));
            }
            'H' => {
                let x = num(&tokens[i]);
                i += 1;
                cx = if rel { cx + x } else { x };
                pts.push(Point::new(cx, cy));
            }
            'V' => {
                let y = num(&tokens[i]);
                i += 1;
                cy = if rel { cy + y } else { y };
                pts.push(Point::new(cx, cy));
            }
            'C' => {
                if i + 5 >= tokens.len() {
                    break;
                }
                let mut c = [0.0f32; 6];
                for k in 0..6 {
                    c[k] = num(&tokens[i + k]);
                }
                i += 6;
                let (x1, y1, x2, y2, x, y) = if rel {
                    (
                        cx + c[0],
                        cy + c[1],
                        cx + c[2],
                        cy + c[3],
                        cx + c[4],
                        cy + c[5],
                    )
                } else {
                    (c[0], c[1], c[2], c[3], c[4], c[5])
                };
                sample_cubic(&mut pts, (cx, cy), (x1, y1), (x2, y2), (x, y), 12);
                cx = x;
                cy = y;
            }
            'S' => {
                // smooth cubic: reflect previous control point — we don't track
                // it, so treat first control == current point.
                if i + 3 >= tokens.len() {
                    break;
                }
                let mut c = [0.0f32; 4];
                for k in 0..4 {
                    c[k] = num(&tokens[i + k]);
                }
                i += 4;
                let (x2, y2, x, y) = if rel {
                    (cx + c[0], cy + c[1], cx + c[2], cy + c[3])
                } else {
                    (c[0], c[1], c[2], c[3])
                };
                sample_cubic(&mut pts, (cx, cy), (cx, cy), (x2, y2), (x, y), 12);
                cx = x;
                cy = y;
            }
            'Q' => {
                if i + 3 >= tokens.len() {
                    break;
                }
                let mut c = [0.0f32; 4];
                for k in 0..4 {
                    c[k] = num(&tokens[i + k]);
                }
                i += 4;
                let (x1, y1, x, y) = if rel {
                    (cx + c[0], cy + c[1], cx + c[2], cy + c[3])
                } else {
                    (c[0], c[1], c[2], c[3])
                };
                sample_quad(&mut pts, (cx, cy), (x1, y1), (x, y), 10);
                cx = x;
                cy = y;
            }
            'T' => {
                if i + 1 >= tokens.len() {
                    break;
                }
                let (x, y) = (num(&tokens[i]), num(&tokens[i + 1]));
                i += 2;
                let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                sample_quad(&mut pts, (cx, cy), (cx, cy), (x, y), 6);
                cx = x;
                cy = y;
            }
            'A' => {
                // Arc: skip radii/rotation/flags, line to endpoint.
                if i + 6 >= tokens.len() {
                    break;
                }
                let (x, y) = (num(&tokens[i + 5]), num(&tokens[i + 6]));
                i += 7;
                let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                cx = x;
                cy = y;
                pts.push(Point::new(x, y));
            }
            _ => {
                i += 1; // unknown command: skip one token
            }
        }
    }
    if pts.is_empty() {
        None
    } else {
        Some(Stroke(pts))
    }
}

fn num(s: &str) -> f32 {
    s.parse::<f32>().unwrap_or(0.0)
}

fn tokenize_path(d: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars = d.chars().peekable();
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        if !cur.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    for c in chars {
        if c.is_ascii_alphabetic() {
            flush(&mut cur, &mut out);
            out.push(c.to_string());
        } else if c == ',' || c.is_whitespace() {
            flush(&mut cur, &mut out);
        } else if c == '-' || c == '+' {
            // sign starts a new number unless it directly follows e/E
            // or is the first char of the current number
            let is_exp = cur.ends_with('e') || cur.ends_with('E');
            if !cur.is_empty() && !is_exp {
                flush(&mut cur, &mut out);
            }
            cur.push(c);
        } else if c == '.' {
            if cur.contains('.') && !cur.to_lowercase().contains('e') {
                flush(&mut cur, &mut out);
            }
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    flush(&mut cur, &mut out);
    out
}

fn sample_cubic(
    pts: &mut Vec<Point>,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    n: usize,
) {
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let mt = 1.0 - t;
        let x = mt * mt * mt * p0.0
            + 3.0 * mt * mt * t * p1.0
            + 3.0 * mt * t * t * p2.0
            + t * t * t * p3.0;
        let y = mt * mt * mt * p0.1
            + 3.0 * mt * mt * t * p1.1
            + 3.0 * mt * t * t * p2.1
            + t * t * t * p3.1;
        pts.push(Point::new(x, y));
    }
}

fn sample_quad(pts: &mut Vec<Point>, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), n: usize) {
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let mt = 1.0 - t;
        let x = mt * mt * p0.0 + 2.0 * mt * t * p1.0 + t * t * p2.0;
        let y = mt * mt * p0.1 + 2.0 * mt * t * p1.1 + t * t * p2.1;
        pts.push(Point::new(x, y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINGLE: &str = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="109" height="109" viewBox="0 0 109 109">
<g id="kvg:04e00">
<path id="kvg:04e00-s1" d="M54.5,10 C60,30 65,50 63,99"/>
<path id="kvg:04e00-s2" d="M10,54.5 L99,54.5"/>
</g>
</svg>"#;

    #[test]
    fn parses_single_glyph() {
        let entries = parse_kanjivg_xml(SINGLE).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].character, Some('一'));
        assert_eq!(entries[0].strokes.len(), 2);
    }

    #[test]
    fn parses_path_commands() {
        let s = parse_path_d("M10,10 L20,20 l5,0 H30 V40 z").unwrap();
        assert!(s.len() >= 4);
        let c = parse_path_d("M54.5,10 C60,30 65,50 63,99").unwrap();
        assert!(c.len() > 5);
        let q = parse_path_d("M0,0 Q10,10 20,0").unwrap();
        assert!(q.len() > 2);
    }

    #[test]
    fn id_to_char_mapping() {
        assert_eq!(id_to_char("04e00"), Some('一'));
        assert_eq!(id_to_char("00001"), None);
    }

    #[test]
    fn parses_real_file_layout_with_wrappers_and_element() {
        // Mirrors actual KanjiVG per-glyph files: StrokePaths_/StrokeNumbers_
        // wrappers, nested -gN component groups, and the authoritative
        // kvg:element label (incl. old decimal-serial ids).
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 109 109">
<g id="kvg:StrokePaths_04e01">
<g id="kvg:04e01" kvg:element="丁">
<g id="kvg:04e01-g1" kvg:element="一">
<path id="kvg:04e01-s1" d="M14,24 L90,20"/>
</g>
<g id="kvg:04e01-g2" kvg:element="亅">
<path id="kvg:04e01-s2" d="M55,23 L55,88"/>
</g>
</g>
</g>
<g id="kvg:StrokeNumbers_04e01"><text>1</text></g>
</svg>"#;
        let entries = parse_kanjivg_xml(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "04e01");
        assert_eq!(entries[0].character, Some('丁'));
        assert_eq!(entries[0].strokes.len(), 2);
    }

    #[test]
    fn parses_old_serial_id_via_element() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 109 109">
<g id="kvg:StrokePaths_00021">
<g id="kvg:00021" kvg:element="!">
<path id="kvg:00021-s1" d="M54.5,15 L54.5,76"/>
<path id="kvg:00021-s2" d="M54.5,88 c-0.83,0 -1.5,0.67 -1.5,1.5"/>
</g>
</g>
</svg>"#;
        let entries = parse_kanjivg_xml(xml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].character, Some('!'));
        assert_eq!(entries[0].strokes.len(), 2);
    }

    #[test]
    fn parses_compact_relative_curves() {
        // Real KanjiVG curve style: relative c with compact negatives.
        let s = parse_path_d("M54.5,15.79c0,6.07-0.29,55.49-0.29,60.55").unwrap();
        assert!(s.len() > 5);
        let last = s.0.last().unwrap();
        assert!((last.x - 54.21).abs() < 0.5, "x={}", last.x);
        assert!((last.y - 76.34).abs() < 0.5, "y={}", last.y);
    }
}
