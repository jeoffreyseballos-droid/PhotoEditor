# Phase 2.1 implementation inventory

Development stayed inside PhotoEditor. PhotographerApp was not modified. Phase 2 architecture was inspected and extended, not replaced. No Phase 3 recipe history, trained styles, cloud/auth/API or production signing work was added.

## Phase 2.1 changed files

| Files                                                                                   | Change                                                                                                               |
| --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Cargo.toml, Cargo.lock                                                                  | Mask-helper workspace member and locked dependencies                                                                 |
| crates/photo-contracts/src/{development,lib,toolkit}.rs                                 | Additive schema-aware typed toolbox, local layers, source optics context and diagnostics                             |
| crates/photo-contracts/tests/toolkit.rs                                                 | Legacy loading, curves, bounds, local-geometry rejection                                                             |
| crates/photo-core/Cargo.toml                                                            | roxmltree database parser                                                                                            |
| crates/photo-core/src/rendering/{mod,pixels,tools,optics,masks}.rs                      | Same f32 renderer plus photographic stages, conservative database resolver, provider/cache/local blend/debug overlay |
| crates/photo-core/src/development.rs                                                    | Bounded mask orchestration, metadata context, availability-aware cache policy, saved diagnostics                     |
| crates/photo-core/src/{repository,models,external,metadata}.rs                          | Additive migration and optional source focus-distance extraction                                                     |
| crates/photo-core/migrations/004_toolkit.sql                                            | Small diagnostic JSON and mask-state metadata; no pixel arrays                                                       |
| crates/photo-core/tests/{toolkit,foundation,rendering}.rs                               | New renderer/model/optics/migration tests and updated version/request fixtures                                       |
| crates/photo-mask-helper/{Cargo.toml,src/main.rs}                                       | Isolated, cancellable CPU ONNX portrait alpha inference                                                              |
| scripts/prepare-toolkit.mjs, package.json                                               | Checksum-pinned model/runtime/database preparation and native build integration                                      |
| src-tauri/src/{lib,commands}.rs, src-tauri/tauri.conf.json                              | Runtime provider setup, background mask IPC, toolkit resources                                                       |
| src/{types,api,toolkit}.ts                                                              | Typed toolbox DTOs/defaults and mask commands; no pixel processing in React                                          |
| src/components/{DevelopmentPanel,ToolkitControls}.tsx, src/styles.css                   | Organized sections, independent layers/resets, aligned overlay, failure diagnostics                                  |
| src/test/development.test.tsx                                                           | Nested controls, local resets, mask generation/failure and export isolation                                          |
| README.md, docs/{ARCHITECTURE,IMPLEMENTATION,LIMITATIONS,VERIFICATION,MODEL-NOTICES}.md | Current implementation and evidence/limits                                                                           |

## New dependencies and assets

- ort = 2.0.0-rc.13 (only in the sidecar; std/load-dynamic/api-24), with ort-sys/libloading locked in Cargo.lock.
- ONNX Runtime 1.29.0, official CPU native distribution, MIT. Windows x64 ZIP and Apple Silicon archive have pinned SHA-256 checksums in prepare-toolkit.mjs. Only DLL/dylib runtime libraries and licenses/notices are packaged, not debug PDBs.
- MODNet FP32, 25,888,640 bytes; original ZHKKKe/MODNet and Xenova ONNX conversion, Apache-2.0. Conversion revision fa2fa546052fba4c08921230a26cc69a333fca12; model SHA-256 07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9. Original Apache license is bundled and hash-verified.
- Lensfun database revision 23e8cb8050d680c7a293edb3d48b600754665f05, unmodified XML and CC BY-SA 3.0 license/attribution. No Lensfun LGPL library/GLib dependency is linked. roxmltree 0.21.1 parses the database. Supported math is a deliberately bounded independent subset.

Asset source URLs, notices and revisions are in MODEL-NOTICES.md and prepare-toolkit.mjs. Downloads/extraction stay under .tools/native-src/phase21; packaged assets under .resources/toolkit. They are ignored build resources, not a large checked-in model blob. Preparation fails on checksum mismatch. There are no runtime model downloads or cloud inference. Commercial distribution still requires normal dependency/asset license review. The Windows toolkit resources total approximately 48 MB, including the 26 MB model.

## Current build / verification workflow

Use the Visual Studio x64 Native Tools environment (or dot-source scripts/activate-msvc.ps1) before Cargo. npm run prepare:native now prepares ExifTool, LibRaw and the local toolkit. The no-bundle executable remains target/release/photo-editor-desktop.exe with adjacent exiftool/, raw/ and toolkit/ directories.

For isolated verification on this workstation, CARGO_TARGET_DIR was temporarily set to the project-local .tools/verify-msvc in the testing shell. This avoided incompatible stale metadata in the older target directory; no global toolchain settings or user source files were removed. Native preparation/desktop build use the ordinary target directory.

The expanded defaults are neutral and schema-aware. SQLite 004 is additive; old adjustment JSON still loads, preserving Phase 2 sharpening/NR and export history. A model/provider failure is nonfatal to global rendering. UI defaults to explicit previews, offers optional 350 ms debounced Auto Preview with no failure retry loop, and generates masks explicitly. Full implementation semantics and pipeline order are in ARCHITECTURE.md; tested versus unverified results are in VERIFICATION.md.

## Preserved Phase 2 implementation record

All source changes, downloaded build inputs, generated fixtures and build artifacts are inside Documents/Projects/PhotoEditor. PhotographerApp was not modified. No Git repository, commit, remote, service, cloud deployment or later-phase workflow was created.

## Files changed

| Files                                                                                 | Purpose                                                                                                                         |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Cargo.toml, Cargo.lock                                                                | Add the native RAW helper workspace crate; lock rendering dependencies                                                          |
| crates/photo-contracts/src/development.rs                                             | Validated neutral adjustments, crop, output formats, cancellation and structured processing errors                              |
| crates/photo-contracts/src/lib.rs, formats.rs                                         | Concrete ProcessingEngine request/result and separate development capability descriptors                                        |
| crates/photo-core/src/rendering/{mod,decode,pixels,output}.rs                         | Replaceable LibRaw adapter, ICC raster path, f32 CPU stages, preview intermediate, JPEG/TIFF encoders, no-clobber output naming |
| crates/photo-core/src/development.rs                                                  | Bounded background orchestration, preview cache, save/load and job checkpoints                                                  |
| crates/photo-core/migrations/003_development.sql, src/repository.rs                   | Additive parameter/result persistence and interrupted-render recovery                                                           |
| crates/photo-core/src/external.rs, process.rs                                         | Export-only EXIF allowlist, cancellable bounded helper transport                                                                |
| crates/photo-core/Cargo.toml, src/lib.rs                                              | TIFF decode feature, Little CMS and module exports                                                                              |
| crates/photo-raw-helper/{Cargo.toml,build.rs,src/main.rs,src/bridge.cpp}              | Static LibRaw build, one-request native worker and Unicode-safe transport                                                       |
| scripts/prepare-libraw.mjs, activate-msvc.ps1                                         | Pinned source/checksum, helper build/resource copy and process-local MSVC setup                                                 |
| package.json, src-tauri/tauri.conf.json                                               | Prepare and package both native helpers and LibRaw licensing/source                                                             |
| src-tauri/src/{lib,commands}.rs                                                       | Engine/resource setup, load/save/render/cancel background IPC                                                                   |
| src/{api,types}.ts                                                                    | Typed development DTOs                                                                                                          |
| src/components/DevelopmentPanel.tsx, src/screens/JobScreen.tsx, src/styles.css        | Selected-photo controls, saved parameters, before/after, cancellation, full export                                              |
| crates/photo-core/tests/{rendering,foundation}.rs, src/test/{development,ui}.test.tsx | Rendering/native/metadata/persistence/UI tests and additive schema assertion                                                    |
| docs/{ARCHITECTURE,LIMITATIONS,VERIFICATION,IMPLEMENTATION}.md, README.md             | Current behavior, build instructions and evidence versus manual acceptance                                                      |

Ingestion/discovery and source-preview implementations were not rewritten. Existing job membership, output pruning, metadata diagnostics and scan recovery remain in place.

## Added dependencies and build inputs

- LibRaw **0.22.2**, official unmodified source archive; SHA-256 de86b035655accff8d4010f1a221fdf50d353cb7b1422ba26f14a0db92612cfa.
- lcms2 **6.2.0**, static feature; lcms2-sys **4.0.7**, bundled Little CMS native build.
- Existing image **0.25.10** now enables TIFF decoding. Existing tiff **0.11.3** writes strip-based RGB16 output.
- New helper uses existing serde/serde_json and build dependencies cc (parallel compilation) / walkdir. Cargo.lock captures transitive dependencies.
- Existing ExifTool **13.59.2** npm runtime remains pinned; export metadata uses a new fixed allowlist.

LibRaw is dual-licensed LGPL-2.1/CDDL. Resources include its original source archive, COPYRIGHT and both license files; proprietary distribution must review license obligations and application notices before release. No production signing/distribution work was done. Little CMS is built statically through its crate; preserve dependency license notices for eventual distribution.

## Build procedure

Windows: install Rust x64 MSVC, Visual Studio C++ Build Tools and Windows SDK. From this project in an x64 Native Tools shell, run npm ci then npm run prepare:native. The preparation script downloads the pinned archive only if absent, verifies it, extracts under .tools/native-src, builds photo-raw-helper and copies it into .resources/raw.

For this workstation, scripts/activate-msvc.ps1 imports VsDevCmd's x64 environment, prioritizes the MSVC linker and Rust MSVC toolchain, and places Cargo cache/test temp files under .tools. Its environment changes are process-local; it does not install tools or alter permanent PATH. It handles duplicate PATH/Path variables emitted by the sandbox launcher.

macOS: use Apple Silicon Rust and Xcode command-line tools; npm run prepare:native builds the same helper with Clang. The existing CI matrix prepares resources before tests/Clippy/desktop build. CI configuration is not evidence of a Mac execution.

Run npm run desktop for development, or npm run desktop:build -- --no-bundle for the executable. The no-bundle Windows output is target/release/photo-editor-desktop.exe, accompanied by raw/ and exiftool/. Do not distribute the executable alone.

## Deliberate choices

Quality/correctness first: mature sensor processing, linear/high-precision edits, explicit ICC and source immutability. CPU-only with one render at a time; native decoding isolated for practical cancellation. Preview and export share creative code but use different source resolutions. Recipes/AI and production polish were not expanded into this phase.

Generated fixtures contain no copyrighted camera collection. Synthetic DNG tests prove the native integration only. The large TIFF test is separately invoked in release mode because it intentionally exercises substantial memory and disk.
