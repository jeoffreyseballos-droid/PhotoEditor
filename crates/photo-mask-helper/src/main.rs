//! Isolated CPU portrait-matting worker. No creative image processing or network access.
use ort::{session::Session, value::Tensor};
use serde::Deserialize;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Request {
    runtime: PathBuf,
    model: PathBuf,
    input: PathBuf,
    output: PathBuf,
    width: usize,
    height: usize,
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin().lock().take(64 * 1024))?;
    let (w, h) = (request.width, request.height);
    if !(32..=1024).contains(&w) || !(32..=1024).contains(&h) || w % 32 != 0 || h % 32 != 0 {
        return Err("Invalid bounded model input dimensions".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&request.input)?
        .take((w * h * 12 + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() != w * h * 12 {
        return Err("Invalid model input length".into());
    }
    let mut input = vec![0f32; w * h * 3];
    for (i, b) in bytes.as_chunks::<4>().0.iter().enumerate() {
        let v = f32::from_le_bytes(*b);
        if !v.is_finite() {
            return Err("Nonfinite model input".into());
        }
        input[(i % 3) * w * h + i / 3] = v.clamp(-1., 1.);
    }
    ort::init_from(&request.runtime)?.commit();
    let mut session = Session::builder()?
        .with_intra_threads(2)?
        .with_inter_threads(1)?
        .commit_from_file(&request.model)?;
    let tensor = Tensor::from_array(([1usize, 3, h, w], input.into_boxed_slice()))?;
    let outputs = session.run(ort::inputs!["input"=>tensor])?;
    let (shape, values) = outputs["output"].try_extract_tensor::<f32>()?;
    if shape.as_ref() != [1, 1, h as i64, w as i64]
        || values.len() != w * h
        || values
            .iter()
            .any(|v| !v.is_finite() || *v < -0.001 || *v > 1.001)
    {
        return Err("Invalid matting output shape/range".into());
    }
    let mut output = std::io::BufWriter::new(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&request.output)?,
    );
    for value in values {
        output.write_all(&value.clamp(0., 1.).to_le_bytes())?;
    }
    output.flush()?;
    println!("{{\"width\":{w},\"height\":{h}}}");
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Portrait mask unavailable: {error}");
        std::process::exit(1);
    }
}
