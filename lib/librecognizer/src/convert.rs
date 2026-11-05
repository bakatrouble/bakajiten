//! One-time model conversion: Google's download packs into the
//! runtime bundle this crate decodes from.
//!
//! The recognizer loads one file, `network.safetensors`: the
//! dequantized 5x144 BiLSTM + logits in tegaki's tensor layout
//! (`l{layer}.{forward,backward}.{w_in,w_rec,b}` + `logits.{w,b}`),
//! with the decoder's vocab riding in the safetensors header
//! metadata (`codepoints`, `char_classes`, `precontext_classes`)
//! — no sidecar, one atomic install, one size check.
//!
//! [`convert_model`] derives the bundle from the shipped packs:
//! the `.tflite` net (dequantized exactly like tegaki's
//! `tools/extract_weights.py`: hybrid kernels declared `UINT8`
//! with `zero_point == 0` hold *signed int8* bit patterns, i.e.
//! `float = scale * int8(byte)`; biases are plain float32) and the
//! recospec protobuf (charset + char-class table, as in tegaki's
//! `tools/extract_recospec.py`).
//!
//! Parsing lives in [`crate::pack`] (TFLite + recospec readers
//! ported from the pinned upstream revision) — the inference engine
//! no longer shares code with it, but the pack formats are
//! unchanged, so the ported readers stay the single source of truth
//! for them. Anything unexpected about the fixed Japanese
//! pack (layer count, widths, diagonal recurrence, nonzero initial
//! states, cell clip, class count) fails loudly rather than
//! producing a silently wrong bundle.

use crate::pack::{recospec::RecoSpec, tflite::NetworkWeights};

/// Fixed Japanese pack shape (5x144 full BiLSTM + 6760-class head).
pub const EXPECTED_LAYERS: usize = 5;
/// Hidden size per direction.
pub const EXPECTED_HIDDEN: usize = 144;
/// Feature dimension of the first layer's input.
pub const EXPECTED_INPUT_DIM: usize = 10;
/// TFLite output classes (6759 codepoints + CTC blank).
pub const EXPECTED_CLASSES: usize = 6760;
/// Codepoints (excludes blank).
pub const EXPECTED_CODEPOINTS: usize = 6759;
/// Cell clipping threshold the forward pass hardcodes (matches the
/// TFLite builtin LSTM options of this pack).
pub const EXPECTED_CELL_CLIP: f32 = 50.0;

/// Byte length of the converted `network.safetensors` (single
/// source of truth for cache checks trackside; asserted by the
/// samples test, so a serialization change fails loudly instead
/// of silently invalidating installs).
pub const EXPECTED_SAFETENSORS_LEN: usize = 16_567_648;

/// Converted runtime bundle: the net plus its vocab, in one
/// `network.safetensors` file (vocab rides the header metadata).
pub struct ConvertedModel {
    pub safetensors: Vec<u8>,
}

/// Failures converting download packs into the runtime bundle.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ConvertError {
    #[error("invalid tflite: {0}")]
    Tflite(String),
    #[error("invalid recospec: {0}")]
    Recospec(String),
    #[error("unexpected pack: {0}")]
    Unexpected(String),
    #[error("serialization failed: {0}")]
    Serialize(String),
}

/// Derive the runtime bundle from pack bytes: the `.tflite` net and
/// the recospec protobuf. Both rejections and shape surprises fail.
pub fn convert_model(tflite_bytes: &[u8], recospec_bytes: &[u8]) -> Result<ConvertedModel, ConvertError> {
    let weights = crate::pack::tflite::load_weights(tflite_bytes)
        .map_err(|e| ConvertError::Tflite(e.to_string()))?;
    check_weights(&weights)?;
    let spec =
        RecoSpec::parse(recospec_bytes).map_err(|e| ConvertError::Recospec(e.to_string()))?;
    check_spec(&spec)?;

    let safetensors = write_safetensors(&weights, &spec)?;
    Ok(ConvertedModel { safetensors })
}

fn check_weights(w: &NetworkWeights) -> Result<(), ConvertError> {
    let bad = |m: String| ConvertError::Unexpected(m);
    if w.layers.len() != EXPECTED_LAYERS {
        return Err(bad(format!("expected {EXPECTED_LAYERS} layers, got {}", w.layers.len())));
    }
    if w.num_classes != EXPECTED_CLASSES {
        return Err(bad(format!(
            "expected {EXPECTED_CLASSES} classes, got {}",
            w.num_classes
        )));
    }
    let mut width = EXPECTED_INPUT_DIM;
    for (li, layer) in w.layers.iter().enumerate() {
        for (name, dir) in [("forward", &layer.forward), ("backward", &layer.backward)] {
            if dir.diagonal {
                return Err(bad(format!("layer {li} {name}: diagonal recurrence is unsupported")));
            }
            if dir.hidden != EXPECTED_HIDDEN {
                return Err(bad(format!(
                    "layer {li} {name}: hidden {}, expected {EXPECTED_HIDDEN}",
                    dir.hidden
                )));
            }
            if dir.input != width {
                return Err(bad(format!(
                    "layer {li} {name}: input width {}, expected {width}",
                    dir.input
                )));
            }
            if dir.input_kernels.len() != 4 * EXPECTED_HIDDEN * width {
                return Err(bad(format!("layer {li} {name}: bad input kernel length")));
            }
            if dir.recurrent_kernels.len() != 4 * EXPECTED_HIDDEN * EXPECTED_HIDDEN {
                return Err(bad(format!("layer {li} {name}: bad recurrent kernel length")));
            }
            if dir.biases.len() != 4 * EXPECTED_HIDDEN {
                return Err(bad(format!("layer {li} {name}: bad bias length")));
            }
            if dir.initial_activation.iter().any(|&v| v != 0.0)
                || dir.initial_cell.iter().any(|&v| v != 0.0)
            {
                return Err(bad(format!("layer {li} {name}: nonzero initial state")));
            }
        }
        if layer.cell_clip != EXPECTED_CELL_CLIP {
            return Err(bad(format!(
                "layer {li}: cell clip {}, expected {EXPECTED_CELL_CLIP}",
                layer.cell_clip
            )));
        }
        width = 2 * EXPECTED_HIDDEN;
    }
    if w.fc_weights.len() != EXPECTED_CLASSES * width {
        return Err(bad(String::from("bad fully-connected weight length")));
    }
    if w.fc_bias.len() != EXPECTED_CLASSES {
        return Err(bad(String::from("bad fully-connected bias length")));
    }
    if w.input_size != EXPECTED_INPUT_DIM {
        return Err(bad(format!("input size {}, expected {EXPECTED_INPUT_DIM}", w.input_size)));
    }
    Ok(())
}

fn check_spec(spec: &RecoSpec) -> Result<(), ConvertError> {
    if spec.charset.len() != EXPECTED_CODEPOINTS {
        return Err(ConvertError::Unexpected(format!(
            "expected {EXPECTED_CODEPOINTS} charset entries, got {}",
            spec.charset.len()
        )));
    }
    if spec.decoder.char_class_weights.is_empty() {
        return Err(ConvertError::Unexpected(String::from("recospec has no char-class weights")));
    }
    if spec.decoder.char_class_table.is_none() {
        return Err(ConvertError::Unexpected(String::from("recospec has no char-class table")));
    }
    Ok(())
}

fn f32le(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn write_safetensors(w: &NetworkWeights, spec: &RecoSpec) -> Result<Vec<u8>, ConvertError> {
    let mut named: Vec<(String, Vec<usize>, Vec<u8>)> = Vec::new();
    let mut push = |name: String, shape: Vec<usize>, values: &[f32]| {
        named.push((name, shape, f32le(values)));
    };
    for (li, layer) in w.layers.iter().enumerate() {
        for (d, dir) in [("forward", &layer.forward), ("backward", &layer.backward)] {
            let in_dim = dir.input;
            push(format!("l{li}.{d}.w_in"), vec![4 * EXPECTED_HIDDEN, in_dim], &dir.input_kernels);
            push(
                format!("l{li}.{d}.w_rec"),
                vec![4 * EXPECTED_HIDDEN, EXPECTED_HIDDEN],
                &dir.recurrent_kernels,
            );
            push(format!("l{li}.{d}.b"), vec![4 * EXPECTED_HIDDEN], &dir.biases);
        }
    }
    push(String::from("logits.w"), vec![EXPECTED_CLASSES, 2 * EXPECTED_HIDDEN], &w.fc_weights);
    push(String::from("logits.b"), vec![EXPECTED_CLASSES], &w.fc_bias);
    let tensors: Vec<(String, safetensors::tensor::TensorView<'_>)> = named
        .iter()
        .map(|(name, shape, data)| {
            safetensors::tensor::TensorView::new(safetensors::Dtype::F32, shape.clone(), data)
                .map(|v| (name.clone(), v))
                .map_err(|e| ConvertError::Serialize(e.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // The decoder's vocab rides the header metadata: one file, one
    // atomic install, one size check. Values are JSON (metadata only
    // carries strings); a single `vocab` entry keeps the header
    // byte-deterministic (a multi-entry map would serialize in hash
    // order).
    let classes: Vec<CharClass> = spec
        .decoder
        .char_class_weights
        .iter()
        .map(|(class, weight)| CharClass { class: class.clone(), weight: *weight })
        .collect();
    let vocab = serde_json::to_string(&serde_json::json!({
        "codepoints": spec.charset,
        "char_classes": classes,
        "precontext_classes": spec.decoder.char_class_table.clone().unwrap_or_default(),
    }))
    .map_err(|e| ConvertError::Serialize(e.to_string()))?;
    let meta = std::collections::HashMap::from([(String::from("vocab"), vocab)]);
    safetensors::serialize(tensors, &Some(meta))
        .map_err(|e| ConvertError::Serialize(e.to_string()))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CharClass {
    class: String,
    weight: f64,
}

/// Read the vocab back out of a converted bundle's header (the
/// load path; also what the equivalence test checks).
pub fn read_vocab(
    safetensors: &[u8],
) -> Result<(Vec<String>, Vec<(String, f64)>, String), ConvertError> {
    let (_, header) = safetensors::SafeTensors::read_metadata(safetensors)
        .map_err(|e| ConvertError::Serialize(e.to_string()))?;
    let vocab = header
        .metadata()
        .as_ref()
        .and_then(|m| m.get("vocab"))
        .ok_or_else(|| ConvertError::Unexpected(String::from("bundle has no vocab metadata")))?;
    let v: serde_json::Value =
        serde_json::from_str(vocab).map_err(|e| ConvertError::Serialize(e.to_string()))?;
    let get = |k: &str| v.get(k).ok_or_else(|| ConvertError::Unexpected(format!("bundle lacks {k}")));
    let codepoints: Vec<String> = serde_json::from_value(get("codepoints")?.clone())
        .map_err(|e| ConvertError::Serialize(e.to_string()))?;
    let classes: Vec<CharClass> = serde_json::from_value(get("char_classes")?.clone())
        .map_err(|e| ConvertError::Serialize(e.to_string()))?;
    let precontext = get("precontext_classes")?
        .as_str()
        .ok_or_else(|| ConvertError::Unexpected(String::from("bundle lacks precontext_classes")))?;
    Ok((
        codepoints,
        classes.into_iter().map(|c| (c.class, c.weight)).collect(),
        precontext.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::tflite::{DirectionWeights, LayerWeights};

    #[test]
    fn rejects_garbage_packs() {
        assert!(matches!(
            convert_model(b"not a tflite", b"not a recospec"),
            Err(ConvertError::Tflite(_))
        ));
    }

    #[test]
    fn rejects_empty_packs() {
        // Empty input must fail as invalid, never panic (packs arrive
        // over the network; the FFI boundary never sees a panic).
        assert!(matches!(convert_model(b"", b""), Err(ConvertError::Tflite(_))));
    }

    /// Hand-built weights with exactly the expected pack shape: the
    /// baseline every rejection case mutates.
    fn valid_weights() -> NetworkWeights {
        fn dir(input: usize) -> DirectionWeights {
            DirectionWeights {
                input_kernels: vec![0.0; 4 * EXPECTED_HIDDEN * input],
                recurrent_kernels: vec![0.0; 4 * EXPECTED_HIDDEN * EXPECTED_HIDDEN],
                diagonal: false,
                biases: vec![0.0; 4 * EXPECTED_HIDDEN],
                initial_activation: vec![0.0; EXPECTED_HIDDEN],
                initial_cell: vec![0.0; EXPECTED_HIDDEN],
                hidden: EXPECTED_HIDDEN,
                input,
            }
        }
        NetworkWeights {
            layers: (0..EXPECTED_LAYERS)
                .map(|li| {
                    let input = if li == 0 { EXPECTED_INPUT_DIM } else { 2 * EXPECTED_HIDDEN };
                    LayerWeights {
                        forward: dir(input),
                        backward: dir(input),
                        cell_clip: EXPECTED_CELL_CLIP,
                        custom_options: vec![],
                    }
                })
                .collect(),
            fc_weights: vec![0.0; EXPECTED_CLASSES * 2 * EXPECTED_HIDDEN],
            fc_bias: vec![0.0; EXPECTED_CLASSES],
            input_size: EXPECTED_INPUT_DIM,
            num_classes: EXPECTED_CLASSES,
        }
    }

    fn assert_rejects_shape(mutate: impl FnOnce(&mut NetworkWeights), want: &str, case: &str) {
        let mut weights = valid_weights();
        mutate(&mut weights);
        match check_weights(&weights) {
            Err(ConvertError::Unexpected(message)) if message.contains(want) => {}
            other => panic!("{case}: expected Unexpected containing {want:?}, got {other:?}"),
        }
    }

    #[test]
    fn check_weights_rejects_shape_surprises() {
        assert!(check_weights(&valid_weights()).is_ok());
        assert_rejects_shape(|w| { w.layers.pop(); }, "expected 5 layers", "layer count");
        assert_rejects_shape(|w| w.num_classes = 0, "expected 6760 classes", "class count");
        assert_rejects_shape(
            |w| w.layers[0].forward.diagonal = true,
            "diagonal recurrence",
            "diagonal",
        );
        assert_rejects_shape(|w| w.layers[1].backward.hidden = 1, "expected 144", "hidden");
        assert_rejects_shape(|w| w.layers[0].forward.input = 1, "expected 10", "input width");
        assert_rejects_shape(
            |w| { w.layers[0].forward.input_kernels.pop(); },
            "bad input kernel length",
            "input kernels",
        );
        assert_rejects_shape(
            |w| w.layers[3].backward.recurrent_kernels.truncate(1),
            "bad recurrent kernel length",
            "recurrent kernels",
        );
        assert_rejects_shape(
            |w| w.layers[4].forward.biases.clear(),
            "bad bias length",
            "biases",
        );
        assert_rejects_shape(
            |w| w.layers[2].forward.initial_activation[0] = 1.0,
            "nonzero initial state",
            "initial activation",
        );
        assert_rejects_shape(
            |w| w.layers[2].backward.initial_cell[0] = 1.0,
            "nonzero initial state",
            "initial cell",
        );
        assert_rejects_shape(|w| w.layers[0].cell_clip = 0.0, "cell clip", "cell clip");
        assert_rejects_shape(
            |w| { w.fc_weights.pop(); },
            "bad fully-connected weight length",
            "fc weights",
        );
        assert_rejects_shape(
            |w| w.fc_bias.clear(),
            "bad fully-connected bias length",
            "fc bias",
        );
        assert_rejects_shape(|w| w.input_size = 0, "input size", "input size");
    }

    #[test]
    fn check_spec_rejects_bad_specs() {
        assert!(matches!(
            check_spec(&RecoSpec::default()),
            Err(ConvertError::Unexpected(message)) if message.contains("charset entries")
        ));
        let mut spec = RecoSpec::default();
        spec.charset = vec![String::from("a"); EXPECTED_CODEPOINTS];
        assert!(matches!(
            check_spec(&spec),
            Err(ConvertError::Unexpected(message)) if message.contains("no char-class weights")
        ));
        spec.decoder.char_class_weights = vec![(String::from("x"), 1.0)];
        assert!(matches!(
            check_spec(&spec),
            Err(ConvertError::Unexpected(message)) if message.contains("no char-class table")
        ));
        spec.decoder.char_class_table = Some(String::from("x a"));
        assert!(check_spec(&spec).is_ok());
    }

    /// Minimal bundle with the given header metadata (no packs needed).
    fn bundle_with_meta(meta: Option<std::collections::HashMap<String, String>>) -> Vec<u8> {
        let tensors = vec![(
            String::from("t"),
            safetensors::tensor::TensorView::new(safetensors::Dtype::F32, vec![1], &[0u8; 4])
                .expect("test tensor builds"),
        )];
        safetensors::serialize(tensors, &meta).expect("test bundle serializes")
    }

    #[test]
    fn read_vocab_rejects_malformed_bundles() {
        assert!(matches!(read_vocab(b""), Err(ConvertError::Serialize(_))));
        assert!(matches!(read_vocab(b"not a safetensors"), Err(ConvertError::Serialize(_))));
        assert!(matches!(
            read_vocab(&bundle_with_meta(None)),
            Err(ConvertError::Unexpected(message)) if message.contains("no vocab metadata")
        ));
        for (vocab, want) in [
            (String::from("{oops"), "Serialize"),
            (String::from(r#"{"codepoints": []}"#), "char_classes"),
            (
                String::from(r#"{"codepoints": [], "char_classes": []}"#),
                "precontext_classes",
            ),
            (
                String::from(
                    r#"{"codepoints": [], "char_classes": [], "precontext_classes": 42}"#,
                ),
                "precontext_classes",
            ),
        ] {
            let meta = Some(std::collections::HashMap::from([(String::from("vocab"), vocab)]));
            match read_vocab(&bundle_with_meta(meta)) {
                Err(ConvertError::Serialize(_)) if want == "Serialize" => {}
                Err(ConvertError::Unexpected(message)) if message.contains(want) => {}
                other => panic!("vocab {want}: expected rejection, got {other:?}"),
            }
        }
    }
}
