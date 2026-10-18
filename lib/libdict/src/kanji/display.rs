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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_traces() {
        // Spot-checked against the Unicode Kangxi block names.
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
}
