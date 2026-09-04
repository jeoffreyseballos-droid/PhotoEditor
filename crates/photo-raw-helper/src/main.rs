//! Disposable, one-request native decoder. No source writes, no network, no shell.
use serde::Deserialize;
use std::{
    ffi::{c_char, c_void, CStr, CString},
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Request {
    source: PathBuf,
    destination: PathBuf,
    half_size: bool,
    max_pixels: u64,
}
unsafe extern "C" {
    fn pe_decode(
        path: *const c_char,
        half: i32,
        max_pixels: u64,
        owner: *mut *mut c_void,
        pixels: *mut *const u8,
        width: *mut u32,
        height: *mut u32,
        bytes: *mut u32,
        warnings: *mut u32,
    ) -> i32;
    fn pe_free(owner: *mut c_void);
    fn pe_error(code: i32) -> *const c_char;
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().take(65537).read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    let path = CString::new(
        request
            .source
            .to_str()
            .ok_or("Source path is not Unicode")?,
    )?;
    let (mut owner, mut pixels) = (std::ptr::null_mut(), std::ptr::null());
    let (mut width, mut height, mut bytes, mut warnings) = (0, 0, 0, 0);
    // The bridge validates LibRaw's output type. The allocation remains alive until pe_free.
    let code = unsafe {
        pe_decode(
            path.as_ptr(),
            i32::from(request.half_size),
            request.max_pixels,
            &mut owner,
            &mut pixels,
            &mut width,
            &mut height,
            &mut bytes,
            &mut warnings,
        )
    };
    if code != 0 {
        let message = unsafe { CStr::from_ptr(pe_error(code)) }.to_string_lossy();
        println!("{}", serde_json::json!({"code":code,"message":message}));
        return Err("LibRaw decode failed".into());
    }
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        if u64::from(bytes) != u64::from(width) * u64::from(height) * 6 {
            return Err("Invalid native buffer".into());
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&request.destination)?;
        output.write_all(b"PERAW001")?;
        output.write_all(&width.to_le_bytes())?;
        output.write_all(&height.to_le_bytes())?;
        output.write_all(&warnings.to_le_bytes())?;
        let buffer = unsafe { std::slice::from_raw_parts(pixels, bytes as usize) };
        // Both supported desktop targets are little endian.
        output.write_all(buffer)?;
        output.sync_all()?;
        Ok(())
    })();
    unsafe { pe_free(owner) };
    result
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
