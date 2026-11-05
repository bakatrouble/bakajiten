//! Real-ink validation: convert the cached Google packs, decode
//! the vendored sample inks through the app path (pen points, no
//! timestamps), and require the top hypothesis to read as the
//! filename. Skips gracefully when the packs are absent (no
//! network in this run); the Qt `main_window_flows` suite covers
//! the same ground live.

use std::path::PathBuf;

use recognizer::convert::{ConvertError, convert_model};
use recognizer::ffi::{Recognizer, StrokePoint};

/// Sample ink files: filename stem is the expected transcript.
const SAMPLES: &[&str] = &["か", "た", "自", "行"];

fn data_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("BAKAJITEN_DATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("data")
}

fn pack_paths() -> (PathBuf, PathBuf) {
    let root = data_root().join("mlkit-ink").join("ja");
    (root.join("recospec"), root.join("network.tflite"))
}

fn load_packs() -> Option<(Vec<u8>, Vec<u8>)> {
    let (recospec, tflite) = pack_paths();
    let recospec = std::fs::read(&recospec).ok()?;
    let tflite = std::fs::read(&tflite).ok()?;
    if recospec.is_empty() || tflite.is_empty() {
        return None;
    }
    Some((tflite, recospec))
}

fn sample_strokes(name: &str) -> Vec<Vec<StrokePoint>> {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path).expect("sample ink reads");
    let v: serde_json::Value = serde_json::from_str(&text).expect("sample ink parses");
    v["strokes"]
        .as_array()
        .expect("strokes array")
        .iter()
        .map(|s| {
            s.as_array()
                .expect("stroke array")
                .iter()
                .map(|p| StrokePoint {
                    x: p[0].as_f64().expect("x") as f32,
                    y: p[1].as_f64().expect("y") as f32,
                })
                .collect()
        })
        .collect()
}

fn converted() -> Option<Vec<u8>> {
    let (tflite, recospec) = load_packs()?;
    let bundle = recognizer::ffi::convert_model(tflite, recospec).expect("packs convert");
    Some(bundle.safetensors)
}

#[test]
fn conversion_is_deterministic() {
    let Some((tflite, recospec)) = load_packs() else {
        eprintln!("SKIP conversion_is_deterministic: no cached packs (set BAKAJITEN_DATA_DIR)");
        return;
    };
    let a = recognizer::ffi::convert_model(tflite.clone(), recospec.clone()).expect("convert a");
    let b = recognizer::ffi::convert_model(tflite, recospec).expect("convert b");
    assert_eq!(a.safetensors, b.safetensors, "bundle bytes are deterministic");
    // Cache-size contract (mirrored by the Qt/qtbridge install
    // checks): a serialization change must update all three.
    assert_eq!(a.safetensors.len(), recognizer::convert::EXPECTED_SAFETENSORS_LEN);
    // The vocab rides the header metadata: spot-check the table
    // the decoder will rebuild (6759 codepoints, space first).
    let (codepoints, classes, precontext) =
        recognizer::convert::read_vocab(&a.safetensors).expect("vocab metadata reads");
    assert_eq!(codepoints.len(), 6759);
    assert_eq!(codepoints[0], " ");
    assert_eq!(classes.len(), 13);
    assert!(precontext.contains("kanji"));
}

#[test]
fn truncated_packs_fail_loudly() {
    let Some((tflite, recospec)) = load_packs() else {
        eprintln!("SKIP truncated_packs_fail_loudly: no cached packs (set BAKAJITEN_DATA_DIR)");
        return;
    };
    // A cut-off flatbuffer names the offending offset (never a
    // panic, never Ok on truncated input).
    assert!(
        matches!(
            recognizer::pack::tflite::load_weights(&tflite[..64]),
            Err(error) if error.to_string().contains("byte ")
        ),
        "truncated tflite names the offset"
    );
    // A cut-off recospec maps to the Recospec variant.
    assert!(
        matches!(
            convert_model(&tflite, &recospec[..16]),
            Err(ConvertError::Recospec(_))
        ),
        "truncated recospec maps to Recospec"
    );
}

#[test]
fn samples_decode_to_filenames() {
    let Some(safetensors) = converted() else {
        eprintln!("SKIP samples_decode_to_filenames: no cached packs (set BAKAJITEN_DATA_DIR)");
        return;
    };
    let r = Recognizer::new(safetensors).expect("converted bundle loads");
    for name in SAMPLES {
        let strokes = sample_strokes(name);
        assert!(!strokes.iter().all(|s| s.is_empty()), "{name}: sample has ink");
        let hits = r.recognize(strokes.clone(), 5).expect("sample decodes");
        assert!(!hits.is_empty(), "{name}: no candidates");
        assert_eq!(hits[0].text, *name, "{name}: top-1 mismatch");
        // The greedy best path agrees on these clear single-kanji inks.
        let greedy = r.recognize_greedy(strokes).expect("greedy decodes");
        assert_eq!(greedy.text, *name, "{name}: greedy mismatch");
    }
}
