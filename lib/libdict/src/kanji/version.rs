//! Artifact version provenance: KanjiVG release tags (`r20260714`
//! in `version.txt`), kanjidic2 creation dates, and the single date
//! version stamped per artifact. Dates are `YYYY-MM-DD` throughout;
//! comparisons are lexicographic over valid dates only — anything
//! else counts as unknown and loses.

/// KanjiVG release tag to ISO date (`r20260714` → `2026-07-14`).
/// Anything else (missing file markers, garbage) is `None`: pack
/// stamps `unknown` instead of a lie.
pub fn kanjivg_release_date(tag: &str) -> Option<String> {
    let digits = tag.trim().strip_prefix('r')?;
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (&digits[0..4], &digits[4..6], &digits[6..8]);
    let month: u32 = m.parse().ok()?;
    let day: u32 = d.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{y}-{m}-{d}"))
}

/// `YYYY-MM-DD` as unix epoch (midnight UTC), for the templates
/// stamp (Howard Hinnant days-from-civil; Gregorian only, which is
/// all artifact dates are).
pub fn iso_date_epoch(date: &str) -> Option<u64> {
    let b = date.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = date[0..4].parse().ok()?;
    let m: i64 = date[5..7].parse().ok()?;
    let d: i64 = date[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    if days < 0 {
        return None;
    }
    Some(days as u64 * 86400)
}

/// Valid stamped date: exactly `YYYY-MM-DD` digits.
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && s[..4].bytes().all(|c| c.is_ascii_digit())
        && s[5..7].bytes().all(|c| c.is_ascii_digit())
        && s[8..].bytes().all(|c| c.is_ascii_digit())
}

/// Latest of two optional source dates (`YYYY-MM-DD` only —
/// anything else is ignored).
pub fn latest_date(a: Option<&str>, b: Option<&str>) -> Option<String> {
    [a, b]
        .into_iter()
        .flatten()
        .filter(|s| is_date(s))
        .max()
        .map(str::to_string)
}

/// Single kanji date version: the latest of the KanjiVG release and
/// kanjidic2 creation stamps. Unknown sides lose; both unknown
/// stays `unknown`.
pub fn kanji_version(kanjivg: &str, kanjidic2: &str) -> String {
    latest_date(Some(kanjivg), Some(kanjidic2)).unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tag_parses() {
        assert_eq!(kanjivg_release_date("r20260714"), Some("2026-07-14".to_string()));
        assert_eq!(kanjivg_release_date("  r20260714\n"), Some("2026-07-14".to_string()));
        assert_eq!(kanjivg_release_date("unknown"), None);
        assert_eq!(kanjivg_release_date("20260714"), None);
        assert_eq!(kanjivg_release_date("r20261301"), None);
        assert_eq!(kanjivg_release_date("r20260001"), None);
        assert_eq!(kanjivg_release_date("r20260732"), None);
        assert_eq!(kanjivg_release_date("r2026071"), None);
        assert_eq!(kanjivg_release_date("r2026071x"), None);
    }

    #[test]
    fn epoch_matches_civil_date() {
        // 2026-07-14 00:00 UTC.
        assert_eq!(iso_date_epoch("2026-07-14"), Some(1783987200));
        assert_eq!(iso_date_epoch("1970-01-01"), Some(0));
        assert_eq!(iso_date_epoch("r20260714"), None);
        assert_eq!(iso_date_epoch("2026-7-14"), None);
        assert_eq!(iso_date_epoch("2026-13-01"), None);
    }

    #[test]
    fn version_takes_latest_known() {
        assert_eq!(kanji_version("2026-07-14", "2026-10-10"), "2026-10-10");
        assert_eq!(kanji_version("2026-10-10", "2026-07-14"), "2026-10-10");
        assert_eq!(kanji_version("unknown", "2026-10-10"), "2026-10-10");
        assert_eq!(kanji_version("2026-07-14", "unknown"), "2026-07-14");
        assert_eq!(kanji_version("unknown", "unknown"), "unknown");
        assert_eq!(kanji_version("r20260714", "2026-10-10"), "2026-10-10");
    }

    #[test]
    fn latest_date_skips_garbage() {
        assert_eq!(latest_date(Some("2026-10-20"), Some("2026-09-01")), Some("2026-10-20".to_string()));
        assert_eq!(latest_date(None, Some("2026-09-01")), Some("2026-09-01".to_string()));
        assert_eq!(latest_date(None, None), None);
        assert_eq!(latest_date(Some("garbage"), None), None);
    }
}
