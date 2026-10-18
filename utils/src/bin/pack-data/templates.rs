//! Precomputed template bank (`KTT1`): every labeled KanjiVG
//! drawing, including style/order variants (`-Kaisho`, `-VtLst`, …)
//! — the matcher scores every drawing of a character.

use super::progress;
use recognizer::model::{
    build_templates, save_templates, templates_from_bytes, templates_to_bytes, ModelEntry,
};
use utils::parsers::kanjivg::KanjiVgEntry;

/// Build the bank from parsed entries, verify the roundtrip, save.
pub(super) fn pack(
    parsed: &[KanjiVgEntry],
    out: &std::path::Path,
    stamp: u64,
    version: String,
) -> Result<super::Summary, Box<dyn std::error::Error>> {
    let entries = filter(parsed);
    let spin = progress::spinner("build templates");
    let bank = build_templates(&entries);
    spin.finish_with_message(format!("build templates ({} drawings)", bank.len()));
    verify(&bank, stamp)?;
    save_templates(out, &bank, stamp)?;
    let bytes = templates_to_bytes(&bank, stamp);
    Ok(super::Summary {
        artifact: "templates".into(),
        path: out.to_path_buf(),
        records: bank.len(),
        bytes: bytes.len(),
        version,
    })
}

/// Keep exactly the recognition data: labeled entries with at least
/// one stroke. Ids, strokeless entries, unlabeled entries, and
/// stroke types stay behind.
fn filter(parsed: &[KanjiVgEntry]) -> Vec<ModelEntry> {
    // Crate-boundary translation (dict strokes → recognizer
    // strokes): the two twins are deliberately distinct types (see
    // `dict::kanji::record`), so the pack tool converts point by
    // point at the one place they meet.
    fn to_recognizer(stroke: &dict::kanji::Stroke) -> recognizer::Stroke {
        recognizer::Stroke::new(
            stroke
                .0
                .iter()
                .map(|p| recognizer::Point::new(p.x, p.y))
                .collect(),
        )
    }
    let bar = super::progress::count_bar("filter templates", parsed.len() as u64);
    let entries: Vec<ModelEntry> = parsed
        .iter()
        .filter_map(|e| {
            bar.inc(1);
            let ch = e.character?;
            if e.strokes.is_empty() {
                return None;
            }
            Some(ModelEntry {
                character: ch,
                strokes: e.strokes.iter().map(to_recognizer).collect(),
            })
        })
        .collect();
    bar.finish_with_message(format!("filter templates ({} kept)", entries.len()));
    println!(
        "keeping {} labeled templates ({} dropped: strokeless or unlabeled)",
        entries.len(),
        parsed.len() - entries.len(),
    );
    entries
}

/// Decode roundtrip plus idempotence (u8 points are lossy vs the f64
/// build, so the file round-trips through itself exactly instead of
/// matching the build stroke-for-stroke).
fn verify(
    bank: &[recognizer::rank::Template],
    stamp: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let spin = super::progress::spinner("encode templates");
    let bytes = templates_to_bytes(bank, stamp);
    let (back, back_stamp) = templates_from_bytes(&bytes)?;
    assert_eq!(back.len(), bank.len(), "template roundtrip lost templates");
    assert_eq!(
        templates_to_bytes(&back, back_stamp),
        bytes,
        "template roundtrip not idempotent"
    );
    spin.finish_with_message("encode templates");
    Ok(())
}
