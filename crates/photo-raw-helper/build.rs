use std::path::PathBuf;
fn main() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.tools/native-src/LibRaw-0.22.2");
    assert!(
        root.join("libraw/libraw.h").exists(),
        "Run node scripts/prepare-libraw.mjs first"
    );
    let mut sources: Vec<_> = walkdir::WalkDir::new(root.join("src"))
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "cpp")
                && !e.file_name().to_string_lossy().ends_with("_ph.cpp")
        })
        .map(|e| e.into_path())
        .collect();
    sources.sort();
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++14")
        .include(&root)
        .define("LIBRAW_NODLL", None)
        .define("LIBRAW_BUILDLIB", None)
        .define("NO_JASPER", None)
        .warnings(false)
        .files(sources)
        .file("src/bridge.cpp");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        build.define("WIN32", None);
    }
    build.compile("photo_libraw");
    println!("cargo:rerun-if-changed=src/bridge.cpp");
    println!("cargo:rerun-if-changed={}", root.display());
}
