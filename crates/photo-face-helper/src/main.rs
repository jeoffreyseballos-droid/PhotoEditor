//! Isolated CPU YuNet face geometry; no identity templates, eye-state claims or network.
use ort::{session::Session, value::Tensor};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
};
const EDGE: usize = 640;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    runtime: PathBuf,
    model: PathBuf,
    input: PathBuf,
    output: PathBuf,
}
#[derive(Clone, Debug, Serialize)]
struct Face {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    confidence: f32,
}
fn overlap(a: &Face, b: &Face) -> f32 {
    let w = ((a.x + a.width).min(b.x + b.width) - a.x.max(b.x)).max(0.);
    let h = ((a.y + a.height).min(b.y + b.height) - a.y.max(b.y)).max(0.);
    w * h / (a.width * a.height + b.width * b.height - w * h).max(0.0001)
}
fn suppress(mut faces: Vec<Face>) -> Vec<Face> {
    faces.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then(a.y.total_cmp(&b.y))
            .then(a.x.total_cmp(&b.x))
    });
    faces.truncate(5000);
    let mut kept = Vec::new();
    for f in faces {
        if kept.iter().all(|k| overlap(&f, k) < 0.3) {
            kept.push(f);
            if kept.len() >= 64 {
                break;
            }
        }
    }
    kept
}
fn decode(
    stride: usize,
    cls: &[f32],
    obj: &[f32],
    boxes: &[f32],
) -> Result<Vec<Face>, Box<dyn std::error::Error>> {
    let n = EDGE / stride;
    let count = n * n;
    if cls.len() != count
        || obj.len() != count
        || boxes.len() != count * 4
        || cls.iter().chain(obj).chain(boxes).any(|v| !v.is_finite())
    {
        return Err("Invalid face model output shape/range".into());
    }
    let mut faces = Vec::new();
    for i in 0..count {
        let score = (cls[i].clamp(0., 1.) * obj[i].clamp(0., 1.)).sqrt();
        if score < 0.9 {
            continue;
        }
        let width = boxes[i * 4 + 2].exp() * stride as f32;
        let height = boxes[i * 4 + 3].exp() * stride as f32;
        let x = ((i % n) as f32 + boxes[i * 4]) * stride as f32 - width / 2.;
        let y = ((i / n) as f32 + boxes[i * 4 + 1]) * stride as f32 - height / 2.;
        if [x, y, width, height].iter().all(|v| v.is_finite())
            && width >= 5.
            && height >= 5.
            && width < EDGE as f32 * 2.
            && height < EDGE as f32 * 2.
        {
            faces.push(Face {
                x,
                y,
                width,
                height,
                confidence: score,
            });
        }
    }
    Ok(faces)
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let r: Request = serde_json::from_reader(std::io::stdin().lock().take(64 * 1024))?;
    let mut bytes = Vec::new();
    std::fs::File::open(&r.input)?
        .take((EDGE * EDGE * 3 + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() != EDGE * EDGE * 3 {
        return Err("Invalid face input length".into());
    }
    let mut input = vec![0f32; bytes.len()];
    for (i, v) in bytes.into_iter().enumerate() {
        input[(i % 3) * EDGE * EDGE + i / 3] = v as f32;
    }
    ort::init_from(&r.runtime)?.commit();
    let mut session = Session::builder()?
        .with_intra_threads(2)?
        .with_inter_threads(1)?
        .commit_from_file(&r.model)?;
    let outputs = session.run(
        ort::inputs!["input"=>Tensor::from_array(([1usize,3,EDGE,EDGE],input.into_boxed_slice()))?],
    )?;
    let mut faces = Vec::new();
    for stride in [8, 16, 32] {
        let cls = format!("cls_{stride}");
        let obj = format!("obj_{stride}");
        let bbox = format!("bbox_{stride}");
        let (_, c) = outputs[cls.as_str()].try_extract_tensor::<f32>()?;
        let (_, o) = outputs[obj.as_str()].try_extract_tensor::<f32>()?;
        let (_, b) = outputs[bbox.as_str()].try_extract_tensor::<f32>()?;
        faces.extend(decode(stride, c, o, b)?);
    }
    let faces = suppress(faces);
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(r.output)?;
    serde_json::to_writer(&mut file, &faces)?;
    file.flush()?;
    println!("{{\"faces\":{}}}", faces.len());
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Local face detection unavailable: {e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoding_and_nms_are_bounded() {
        let mut cls = vec![0.; 6400];
        let mut obj = cls.clone();
        let mut b = vec![0.; 25600];
        cls[81] = 0.99;
        obj[81] = 0.99;
        b[81 * 4 + 2] = 2.;
        b[81 * 4 + 3] = 2.;
        let faces = decode(8, &cls, &obj, &b).unwrap();
        assert_eq!(faces.len(), 1);
        assert!(faces[0].width > 50.);
        let f = faces[0].clone();
        assert_eq!(suppress(vec![f.clone(), f]).len(), 1);
        assert!(decode(8, &[], &[], &[]).is_err());
    }
    #[test]
    fn invalid_and_empty_outputs_fail_or_return_zero() {
        assert!(decode(8, &vec![f32::NAN; 6400], &vec![0.; 6400], &vec![0.; 25600]).is_err());
        assert!(
            decode(8, &vec![0.; 6400], &vec![0.; 6400], &vec![0.; 25600])
                .unwrap()
                .is_empty()
        );
    }
}
