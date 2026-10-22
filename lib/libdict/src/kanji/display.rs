//! Kanji display mappings: classical radical glyphs, JIS X 0208
//! hex codes, and grade categories. Pure data (mirrors the Qt
//! `KanjiInfo` helpers, traces below); translations stay trackside
//! behind `GradeKind`.

/// Grade category for display (`8` reads `Jouyou`, like the mocks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GradeKind {
    Kyouiku,
    Jouyou,
    Jinmeiyou,
    Other,
}

/// Grade category: 1–6 kyouiku, 8 jouyou, 9–10 jinmeiyou.
pub fn grade_kind(grade: u32) -> GradeKind {
    match grade {
        1..=6 => GradeKind::Kyouiku,
        8 => GradeKind::Jouyou,
        9 | 10 => GradeKind::Jinmeiyou,
        _ => GradeKind::Other,
    }
}

/// Kangxi radical glyphs 1–214 in order (NFKC of U+2F00–U+2FD5).
const RADICAL_TABLE: &str = "一丨丶丿乙亅二亠人儿入八冂冖冫几凵刀力勹匕匚匸十卜卩厂厶又口囗土士夂夊夕大女子宀寸小尢尸屮山巛工己巾干幺广廴廾弋弓彐彡彳心戈戶手支攴文斗斤方无日曰月木欠止歹殳毋比毛氏气水火爪父爻爿片牙牛犬玄玉瓜瓦甘生用田疋疒癶白皮皿目矛矢石示禸禾穴立竹米糸缶网羊羽老而耒耳聿肉臣自至臼舌舛舟艮色艸虍虫血行衣襾見角言谷豆豕豸貝赤走足身車辛辰辵邑酉釆里金長門阜隶隹雨靑非面革韋韭音頁風飛食首香馬骨高髟鬥鬯鬲鬼魚鳥鹵鹿麥麻黃黍黑黹黽鼎鼓鼠鼻齊齒龍龜龠";

/// Kangxi radical glyph for a 1–214 number.
/// Empty when out of range.
pub fn radical_glyph(number: u32) -> String {
    if number < 1 {
        return String::new();
    }
    RADICAL_TABLE
        .chars()
        .nth(number as usize - 1)
        .map(|c| c.to_string())
        .unwrap_or_default()
}

/// JIS X 0208 code for kuten: 0x20 offset per byte (`24-62` →
/// `0x385e`, as the dialog mock shows for 五).
pub fn jis_hex(ku: u8, ten: u8) -> String {
    format!("0x{:04x}", (ku as u32 + 0x20) * 256 + (ten as u32 + 0x20))
}

/// One kanji detail table, compiled once for all frontends: stable
/// key plus preformatted value per row, desktop dialog order.
/// Unifies Android `KanjiInfoTable`, web `KanjiInfo.vue`, and Qt
/// `KanjiDialog::setCharacter` (which map the key to their own
/// captions — Qt translates them — and previously diverged):
/// meanings join with `; ` (Qt used `, `), JLPT carries its `N`
/// prefix, Unicode renders `U+XXXX` (Qt used `0x…`), JIS X 0208
/// renders kuten (`ku-ten`; Qt used the hex code, tooltips keep
/// that), and the standalone Radical row is new to the Qt dialog.
/// Grade names stay English (no translations ship anywhere).
pub fn kanji_details(entry: &crate::ffi::records::KanjiEntry) -> Vec<crate::ffi::records::KanjiDetailRow> {
    use crate::ffi::records::{KanjiDetailKey as Key, KanjiDetailRow as Row};
    fn dash(items: &[String], sep: &str) -> String {
        if items.is_empty() {
            "-".to_string()
        } else {
            items.join(sep)
        }
    }
    let grade = entry.grade.map_or_else(|| "-".to_string(), |g| {
        let kind = match grade_kind(g) {
            GradeKind::Kyouiku => "Kyouiku",
            GradeKind::Jouyou => "Jouyou",
            GradeKind::Jinmeiyou => "Jinmeiyou",
            GradeKind::Other => "Other",
        };
        format!("{kind} {g}")
    });
    let jlpt = entry
        .jlpt
        .map_or_else(|| "-".to_string(), |j| format!("N{j}"));
    let radical = entry.radical.map_or("-".to_string(), |n| {
        format!("{} ({})", radical_glyph(n), n)
    });
    let unicode = entry
        .character
        .chars()
        .next()
        .map(|c| format!("U+{:04X}", c as u32))
        .unwrap_or_else(|| "-".to_string());
    let jis0208 = entry
        .jis208
        .as_ref()
        .map(|k| format!("{}-{}", k.ku, k.ten))
        .unwrap_or_else(|| "-".to_string());
    let skip_joined = entry.skip.join(", ");
    let skip = if skip_joined.is_empty() {
        "-".to_string()
    } else if entry.skip_misgrade {
        format!("{skip_joined} (miscount)")
    } else {
        skip_joined
    };
    let row = |key: Key, label: &str, value: String| Row {
        key,
        label: label.to_string(),
        value,
    };
    vec![
        row(Key::Meaning, "Meaning", dash(&entry.meanings, "; ")),
        row(Key::On, "ON", dash(&entry.on, "、")),
        row(Key::Kun, "Kun", dash(&entry.kun, "、")),
        row(Key::Nanori, "Nanori", dash(&entry.nanori, "、")),
        row(
            Key::Chinese,
            "Chinese",
            dash(
                &entry
                    .chinese
                    .iter()
                    .map(|r| crate::pinyin::to_marked(r))
                    .collect::<Vec<_>>(),
                ", ",
            ),
        ),
        row(Key::Korean, "Korean", dash(&entry.korean, ", ")),
        row(Key::Vietnamese, "Vietnamese", dash(&entry.vietnamese, ", ")),
        row(Key::Strokes, "Strokes", entry.strokes.to_string()),
        row(Key::Grade, "Grade", grade),
        row(Key::Jlpt, "JLPT", jlpt),
        row(
            Key::Frequency,
            "Frequency",
            entry.freq.map(|f| f.to_string()).unwrap_or_else(|| "-".to_string()),
        ),
        row(Key::Radical, "Radical", radical),
        row(Key::RadicalNames, "Radical names", dash(&entry.rad_names, ", ")),
        row(Key::Unicode, "Unicode", unicode),
        row(Key::Jis0208, "JIS X0208", jis0208),
        row(Key::Jis0212, "JIS X0212", dash(&entry.jis212, ", ")),
        row(Key::Jis0213, "JIS X0213", dash(&entry.jis213, ", ")),
        row(Key::Jis0208Refs, "JIS X0208 refs", dash(&entry.jis208_refs, ", ")),
        row(Key::Skip, "SKIP", skip),
        row(Key::FourCorner, "Four corner", dash(&entry.four_corner, ", ")),
        row(Key::Deroo, "De Roo", dash(&entry.deroo, ", ")),
        row(Key::ShDesc, "SH", dash(&entry.sh_desc, ", ")),
        row(Key::Nelson, "Nelson", dash(&entry.nelson_c, ", ")),
        row(Key::Njecd, "NJECD", dash(&entry.njecd, ", ")),
        row(Key::Oneill, "O'Neill", dash(&entry.oneill, ", ")),
        row(Key::DerooVariant, "De Roo refs", dash(&entry.deroo_variant, ", ")),
        row(Key::ShRefs, "SH refs", dash(&entry.s_h, ", ")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_traces() {        // Spot-checked against the Unicode Kangxi block names.
        assert_eq!(radical_glyph(1), "一");
        assert_eq!(radical_glyph(7), "二");
        assert_eq!(radical_glyph(138), "艮");
        assert_eq!(radical_glyph(139), "色");
        assert_eq!(radical_glyph(157), "足");
        assert_eq!(radical_glyph(184), "食");
        assert_eq!(radical_glyph(214), "龠");
        assert_eq!(radical_glyph(0), "");
        assert_eq!(radical_glyph(215), "");
        // Table length is exactly the 214 radicals.
        assert_eq!(RADICAL_TABLE.chars().count(), 214);
        assert_eq!(jis_hex(24, 62), "0x385e");
        assert_eq!(jis_hex(16, 1), "0x3021");
        assert_eq!(grade_kind(1), GradeKind::Kyouiku);
        assert_eq!(grade_kind(6), GradeKind::Kyouiku);
        assert_eq!(grade_kind(8), GradeKind::Jouyou);
        assert_eq!(grade_kind(9), GradeKind::Jinmeiyou);
        assert_eq!(grade_kind(10), GradeKind::Jinmeiyou);
        assert_eq!(grade_kind(0), GradeKind::Other);
        assert_eq!(grade_kind(7), GradeKind::Other);
    }

    fn sun_entry() -> crate::ffi::records::KanjiEntry {
        use crate::ffi::records::{JisKuten, KanjiEntry};
        KanjiEntry {
            character: "日".to_string(),
            on: vec!["ニチ".to_string(), "ジツ".to_string()],
            kun: vec!["ひ".to_string(), "か".to_string()],
            meanings: vec!["day".to_string(), "sun".to_string()],
            grade: Some(1),
            jlpt: Some(5),
            freq: Some(1),
            strokes: 4,
            radical: Some(72),
            parts: vec![],
            jis208: Some(JisKuten { ku: 16, ten: 1 }),
            nanori: vec!["あき".to_string()],
            rad_names: vec!["ひ".to_string()],
            chinese: vec!["ri4".to_string()],
            korean: vec!["il".to_string()],
            vietnamese: vec![],
            skip: vec!["4-4-3".to_string()],
            skip_misgrade: false,
            four_corner: vec![],
            deroo: vec![],
            sh_desc: vec![],
            jis208_refs: vec![],
            jis212: vec![],
            jis213: vec![],
            deroo_variant: vec![],
            nelson_c: vec![],
            njecd: vec![],
            oneill: vec![],
            s_h: vec![],
        }
    }

    fn value_of(rows: &[crate::ffi::records::KanjiDetailRow], key: crate::ffi::records::KanjiDetailKey) -> &str {
        rows.iter().find(|r| r.key == key).map(|r| r.value.as_str()).unwrap_or("<missing>")
    }

    #[test]
    fn details_table_matches_consumers() {
        use crate::ffi::records::KanjiDetailKey as Key;        let rows = kanji_details(&sun_entry());
        assert_eq!(rows.len(), 27);
        // Row order is the desktop dialog order.
        let keys: Vec<Key> = rows.iter().map(|r| r.key).collect();
        assert_eq!(keys.first(), Some(&Key::Meaning));
        assert_eq!(keys[7], Key::Strokes);
        assert_eq!(keys.last(), Some(&Key::ShRefs));
        // Labels stay English; Qt maps the keys to tr() captions.
        assert_eq!(rows[0].label, "Meaning");
        // Values follow the Android/web consensus (documented
        // normalizations over the old Qt dialog strings).
        assert_eq!(value_of(&rows, Key::Meaning), "day; sun");
        assert_eq!(value_of(&rows, Key::On), "ニチ、ジツ");
        assert_eq!(value_of(&rows, Key::Chinese), "rì");
        assert_eq!(value_of(&rows, Key::Grade), "Kyouiku 1");
        assert_eq!(value_of(&rows, Key::Jlpt), "N5");
        assert_eq!(value_of(&rows, Key::Radical), "日 (72)");
        assert_eq!(value_of(&rows, Key::Unicode), "U+65E5");
        assert_eq!(value_of(&rows, Key::Jis0208), "16-1");
        assert_eq!(value_of(&rows, Key::Skip), "4-4-3");
        assert_eq!(value_of(&rows, Key::Vietnamese), "-");
    }

    #[test]
    fn details_miscount_and_absent() {
        let mut e = sun_entry();
        e.skip_misgrade = true;
        e.skip = vec![];
        e.grade = None;
        e.radical = None;
        e.jis208 = None;
        let rows = kanji_details(&e);
        // Empty skip list: no miscount note, plain fallback.
        assert_eq!(value_of(&rows, crate::ffi::records::KanjiDetailKey::Skip), "-");
        assert_eq!(value_of(&rows, crate::ffi::records::KanjiDetailKey::Grade), "-");
        assert_eq!(value_of(&rows, crate::ffi::records::KanjiDetailKey::Radical), "-");
        assert_eq!(value_of(&rows, crate::ffi::records::KanjiDetailKey::Jis0208), "-");
        e.skip = vec!["1-2-3".to_string()];
        let rows = kanji_details(&e);
        assert_eq!(value_of(&rows, crate::ffi::records::KanjiDetailKey::Skip), "1-2-3 (miscount)");
    }
}
