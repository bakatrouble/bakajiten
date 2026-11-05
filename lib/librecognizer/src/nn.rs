//! 5-layer bidirectional LSTM + logits, bit-faithful port of the validated
//! numpy reference (`tools/validate_nn.py`, max diff 0.3 vs the interpreter).
//!
//! Math per direction (TFLite `BIDIRECTIONAL_SEQUENCE_LSTM`, ifco gate order,
//! `fused_activation=TANH`, `cell_clip=50`, no peephole/projection/layer-norm):
//! ```text
//! i = sigm(Wxi x + Whi h + bi)    f = sigm(Wxf x + Whf h + bf)
//! g = tanh(Wxc x + Whc h + bc)    o = sigm(Wxo x + Who h + bo)
//! c = clip(f*c + i*g, -50, 50)    h = o * tanh(c)
//! ```
//! Weights are int8-quantized in the `.tflite` (`scale * int8(byte)`); the
//! `.safetensors` asset already stores dequantized float32.

use ndarray::{Array1, Array2};
use safetensors::SafeTensors;
use std::path::Path;
use thiserror::Error;

pub const HIDDEN: usize = 144;
pub const NUM_LAYERS: usize = 5;
pub const FEATURE_DIM: usize = 10;
const CELL_CLIP: f32 = 50.0;

#[derive(Debug, Error)]
pub enum NnError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("safetensors: {0}")]
    Safe(#[from] safetensors::SafeTensorError),
    #[error("missing tensor {0}")]
    Missing(String),
    #[error("bad shape for {0}: {1:?}")]
    Shape(String, Vec<usize>),
}

struct Dir {
    w_in: Array2<f32>,  // [576, in]
    w_rec: Array2<f32>, // [576, 144]
    b: Array1<f32>,     // [576]
}

struct Layer {
    fw: Dir,
    bw: Dir,
}

pub struct BiLstm {
    layers: Vec<Layer>,
    logits_w: Array2<f32>, // [6760, 288]
    logits_b: Array1<f32>, // [6760]
}

fn load_array<'a>(st: &SafeTensors<'a>, name: &str) -> Result<Array2<f32>, NnError> {
    let v = st.tensor(name).map_err(|_| NnError::Missing(name.into()))?;
    if v.dtype() != safetensors::Dtype::F32 {
        return Err(NnError::Shape(name.into(), v.shape().to_vec()));
    }
    let shape = v.shape().to_vec();
    if shape.len() != 2 {
        return Err(NnError::Shape(name.into(), shape));
    }
    let bytes = v.data();
    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Array2::from_shape_vec((shape[0], shape[1]), floats)
        .map_err(|_| NnError::Shape(name.into(), shape))
}

fn load_bias<'a>(st: &SafeTensors<'a>, name: &str) -> Result<Array1<f32>, NnError> {
    let v = st.tensor(name).map_err(|_| NnError::Missing(name.into()))?;
    let bytes = v.data();
    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok(Array1::from_vec(floats))
}

impl BiLstm {
    pub fn load(path: &Path) -> Result<Self, NnError> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NnError> {
        let st = SafeTensors::deserialize(bytes)?;
        let mut layers = Vec::with_capacity(NUM_LAYERS);
        for li in 0..NUM_LAYERS {
            let mut dirs = Vec::with_capacity(2);
            for d in ["forward", "backward"] {
                dirs.push(Dir {
                    w_in: load_array(&st, &format!("l{li}.{d}.w_in"))?,
                    w_rec: load_array(&st, &format!("l{li}.{d}.w_rec"))?,
                    b: load_bias(&st, &format!("l{li}.{d}.b"))?,
                });
            }
            let mut it = dirs.into_iter();
            layers.push(Layer { fw: it.next().unwrap(), bw: it.next().unwrap() });
        }
        Ok(BiLstm {
            layers,
            logits_w: load_array(&st, "logits.w")?,
            logits_b: load_bias(&st, "logits.b")?,
        })
    }

    fn run_dir(&self, dir: &Dir, xs: &[Array1<f32>]) -> Vec<Array1<f32>> {
        let hsz = HIDDEN;
        let (mut h, mut c) = (Array1::zeros(hsz), Array1::zeros(hsz));
        let mut ys = Vec::with_capacity(xs.len());
        for x in xs {
            let gi = dir.w_in.slice(ndarray::s![0..hsz, ..]).dot(x)
                + &dir.w_rec.slice(ndarray::s![0..hsz, ..]).dot(&h)
                + &dir.b.slice(ndarray::s![0..hsz]);
            let gf = dir.w_in.slice(ndarray::s![hsz..2 * hsz, ..]).dot(x)
                + &dir.w_rec.slice(ndarray::s![hsz..2 * hsz, ..]).dot(&h)
                + &dir.b.slice(ndarray::s![hsz..2 * hsz]);
            let gg = dir.w_in.slice(ndarray::s![2 * hsz..3 * hsz, ..]).dot(x)
                + &dir.w_rec.slice(ndarray::s![2 * hsz..3 * hsz, ..]).dot(&h)
                + &dir.b.slice(ndarray::s![2 * hsz..3 * hsz]);
            let go = dir.w_in.slice(ndarray::s![3 * hsz.., ..]).dot(x)
                + &dir.w_rec.slice(ndarray::s![3 * hsz.., ..]).dot(&h)
                + &dir.b.slice(ndarray::s![3 * hsz..]);
            let i: Array1<f32> = gi.mapv(|v| 1.0 / (1.0 + (-v).exp()));
            let f: Array1<f32> = gf.mapv(|v| 1.0 / (1.0 + (-v).exp()));
            let g: Array1<f32> = gg.mapv(|v| v.tanh());
            let o: Array1<f32> = go.mapv(|v| 1.0 / (1.0 + (-v).exp()));
            c = (&f * &c + &i * &g).mapv(|v| v.clamp(-CELL_CLIP, CELL_CLIP));
            h = &o * &c.mapv(|v| v.tanh());
            ys.push(h.clone());
        }
        ys
    }

    /// Forward pass over time-major features. Returns logits `[T][6760]`.
    pub fn forward(&self, feats: &[[f32; FEATURE_DIM]]) -> Vec<Vec<f32>> {
        let mut x: Vec<Array1<f32>> = feats.iter().map(|f| Array1::from_vec(f.to_vec())).collect();
        for layer in &self.layers {
            let fw = self.run_dir(&layer.fw, &x);
            let mut bw = self.run_dir(&layer.bw, &x.iter().rev().cloned().collect::<Vec<_>>());
            bw.reverse();
            x = fw
                .into_iter()
                .zip(bw)
                .map(|(a, b)| ndarray::concatenate![ndarray::Axis(0), a, b])
                .collect();
        }
        x.iter()
            .map(|h| (self.logits_w.dot(h) + &self.logits_b).to_vec())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_file_and_garbage_bytes() {
        assert!(matches!(
            BiLstm::load(Path::new("/tmp/opencode-no-such-model-xyz.safetensors")),
            Err(NnError::Io(_))
        ));
        assert!(matches!(BiLstm::from_bytes(b"not a safetensors file"), Err(_)));
    }
}
