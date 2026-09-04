# Phase 2.1 verification record

Date: 2026-09-03, Windows 11 x64. Rust 1.98.1 MSVC, Visual Studio x64 C++ Build Tools, Node 22.20.0. All source changes, downloads, caches and generated tests stayed inside PhotoEditor. PhotographerApp was not modified.

## Current executed checks

| Check                                                   | Result                                                                                                                          |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| npm run format:check                                    | Passed                                                                                                                          |
| npm run lint                                            | Passed, zero warnings                                                                                                           |
| npm test                                                | 22 passed: 4 format, 11 existing UI, 7 development-panel tests                                                                  |
| npm run build                                           | Passed TypeScript and Vite production build                                                                                     |
| cargo fmt --all -- --check                              | Passed                                                                                                                          |
| cargo test -p photo-core -p photo-contracts --locked    | 88 passed; one heavy case intentionally excluded from default run; doc tests passed                                             |
| cargo clippy --workspace --all-targets -- -D warnings   | Passed across desktop, helpers, contracts, core and test targets                                                                |
| Real local CPU MODNet inference                         | Passed twice on generated 96×64 non-portrait pixels: 768×512 alpha, finite [0,1], repeatable output                             |
| Real pinned Lensfun XML database                        | Parsed and resolved exact Canon EOS 650D / EF-S 10–22mm f/3.5–4.5 USM calibration at 10 mm, f/3.5, 10 m, using synthetic pixels |
| 42 MP neutral TIFF export                               | Separately passed in release mode: 6000×7000 RGB16 output; test body 2.27 seconds                                               |
| Windows desktop build                                   | Unsigned x64 MSVC no-bundle build passed; executable plus raw/, exiftool/ and toolkit/ resources                                |
| Real Canon/Sony RAWs, portraits and lens photographs    | Not run; no local source folder supplied                                                                                        |
| Native interactive desktop / external-viewer acceptance | Not run                                                                                                                         |
| Apple Silicon build/runtime                             | Not run                                                                                                                         |
| Installer/signing/notarization                          | Out of scope                                                                                                                    |

The Rust suite comprises 5 contract-toolkit + 1 process + 22 foundation + 16 ingestion + 19 standard rendering + 25 toolkit integration tests. The separate large-image run adds one executed test (89 total distinct executed Rust tests). The real-model test is already included in the 25 toolkit tests, not counted twice. An additional timed rerun of two model inferences took 897 ms total; this is a synthetic fixture observation, not a portrait or production throughput benchmark. Peak RAM and a full-resolution all-tools photographic benchmark were not measured.

Verification used a fresh project-local .tools/verify-msvc Cargo target directory after the old target/debug produced a stale transitive-metadata error in rustdoc. The full suite, including doc tests, then passed without disabling tests. The ordinary target directory was retained for native preparation and desktop builds. Frontend checks/builds used normal tooling access after the sandbox's ancestor-directory restriction affected esbuild in earlier runs.

## What the new tests establish

- Neutral curve/HSL/presence/detail/vignette behavior; deterministic curve serialization and validation; future-schema rejection; invalid detail/layer bounds; no local geometry/optics fields.
- Hue, saturation and luminance selectivity; continuous overlap and red wrap; distinct texture/clarity/dehaze; finite expanded detail; centered creative vignette.
- Soft subject/background complement, layer disable/invert/strength, masked-only exposure and WB, deterministic global-plus-local composition, stale/custom layer rejection.
- Mask content identity and PNG/JSON persistence; generation reuse after exposure edits; regeneration after disposable cache removal; small SQLite metadata without image arrays.
- Nonfatal failed inference/missing optics; overlay bytes never change exports; identical TIFF pixels through preview/export flags when the fixture resolution is identical and all toolkit stages are active.
- Actual database resolution on synthetic pixels, disabled/unknown/unavailable/unsupported fallback, exact zero-strength identity, deterministic distortion and shared mask coordinates through optics/rotation/crop.
- Real Phase 2 SQLite v3 → v4 migration preserves prior parameters, revisions, export paths and checkpoints; legacy jobs without masks receive neutral defaults.
- UI: nested toolkit persistence, independent global/subject/background resets, generate/show/hide overlays, before/after separation, stale overlay removal, nonfatal model failure, export request isolation and 350 ms auto-preview debounce without failure retry loops.

No generated fixture establishes real Canon/Sony compatibility or visual lens accuracy. No actual lens photograph is marked verified.

## Reproduce Phase 2.1 checks

Run from PhotoEditor. Native preparation needs the normal MSVC target location; set the isolated target only after preparing resources if desired.

```powershell
. ./scripts/activate-msvc.ps1
npm run prepare:native
npm run format:check
npm run lint
npm test
npm run build
cargo fmt --all -- --check
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) '.tools/verify-msvc'
cargo test -p photo-core -p photo-contracts --locked
cargo clippy --workspace --all-targets -- -D warnings
Remove-Item Env:CARGO_TARGET_DIR
cargo test -p photo-core --release --locked --test rendering large_tiff_full_resolution_export_uses_original_dimensions -- --ignored --nocapture
npm run desktop:build -- --no-bundle
```

The last command creates target/release/photo-editor-desktop.exe. Keep all three resource directories beside it. The toolkit is approximately 48 MB including the 26 MB model and licenses. Source builds download pinned assets once, verify SHA-256 and never fetch models during application use.

## Required photographic acceptance

Use existing Canon and Sony source folders read-only, with all test exports under PhotoEditor. For each image record camera, firmware/compression, lens, focal/aperture/focus-distance metadata, profile state and applied components.

1. Open a real portrait RAW; generate Subject / Background masks, then Update Preview. Inspect both overlays at hair, body, clothing, limb gaps and transparent/soft regions.
2. Raise subject EV, lower background EV, warm subject and cool background independently. Inspect spill and edge halos; disable/invert layers and compare. Global editing/export must survive unsuitable or failed non-portrait masks.
3. Apply global HSL/curves/presence/detail. Toggle profile corrections and inspect distortion, peripheral illumination and CA for plausible behavior. Missing exact calibrations should be visibly skipped; test manual fallback separately.
4. Rotate/crop with optics and local layers active; check overlays still align. Check original/edited comparison, all/global/subject/background resets, restart persistence, cache regeneration, cancellation and debounced auto preview.
5. Export full-resolution JPEG and RGB16 TIFF; inspect externally at 100%, compare preview intent, confirm ICC/orientation/metadata and unchanged source hashes/mtime. Export twice to verify suffixing.
6. Repeat with a non-portrait, both Canon and Sony files, and a 16 GB target. Record peak RAM, responsiveness, inference/preview/export time and artifact quality. Repeat build/resource/runtime acceptance on an actual Apple Silicon Mac.

## Historical Phase 2 verification record

Date: 2026-09-03. Windows 11 x64, Rust 1.98.1 x86_64-pc-windows-msvc, Visual Studio Build Tools 2026 x64 compiler/linker, Node 22.20.0. Development, caches and test artifacts remain inside PhotoEditor. No Canon/Sony source folder was supplied during this pass.

## Executed checks

All applicable final checks below passed. Standard Rust suite: 58 passed, 1 intentionally ignored large-image test; that large test was separately executed in release mode and passed. Total executed Rust tests: 59. Frontend: 18 passed.

| Check                                                   | Result                                                                                      |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Frontend unit/UI tests                                  | 18 passed, including three new development-panel tests                                      |
| Rendering integration suite                             | 19 passed; separate large TIFF case explicitly run in release mode                          |
| Existing core regression suites                         | 22 foundation + 16 ingestion tests passed during implementation                             |
| Real native LibRaw integration                          | Generated 128×96 Bayer DNG decoded successfully, including a Unicode Windows path           |
| Export metadata                                         | Real bundled ExifTool camera-tag allowlist passed for JPEG and TIFF; source bytes unchanged |
| Windows desktop MSVC build                              | Passed final unsigned no-bundle rebuild after all code fixes; x64 MSVC linker verified      |
| npm format:check / lint / build                         | All passed                                                                                  |
| cargo fmt / full-workspace Clippy -D warnings           | Both passed                                                                                 |
| cargo test core/contracts --locked                      | 58 passed; 1 heavy test excluded from default run                                           |
| 42 MP TIFF release acceptance                           | Passed separately: RGB16 output 6000×7000; test body 2.28 seconds                           |
| Real Canon/Sony cameras                                 | Not run: no source folder supplied                                                          |
| Native interactive desktop / external-viewer acceptance | Not run                                                                                     |
| Apple Silicon build/runtime                             | Not run                                                                                     |
| Installer/signing/notarization                          | Out of scope; not run                                                                       |

Native resources accompany target/release/photo-editor-desktop.exe in raw/ and exiftool/. LibRaw source/licenses are present. This is a build result, not a claim of installed-package or interactive desktop acceptance.

## What automated tests establish

- Neutral pixel stability; EV doubling and float headroom; finite/range validation; temperature/tint direction and baseline; non-global tone zones; saturation/vibrance selectivity.
- Crop bounds, rotation normalization, deterministic quarter-turn geometry, conservative spatial stages and cancellation checks.
- JPEG and RGB16 TIFF export with embedded ICC; 16-bit TIFF re-import; source bytes unchanged; existing destination/source overwrite refused.
- Embedded linear RGB ICC honored instead of incorrectly assuming sRGB; EXIF orientation applied once.
- RAW proxy cache reused after parameter changes; full export calls full-resolution decode. The mock backend checks orchestration, not camera quality.
- Cache key includes source/parameters/backend/version; missing previews regenerate; saved parameters reload through a reopened repository.
- Atomic filename suffix behavior, successfully published export checkpoints, failed-render asset retention and interrupted-render recovery.
- Bounded replacement-preview queue; explicit cancellation; child-process kill/reap test.
- Real LibRaw sidecar successfully develops a generated Bayer DNG. Real ExifTool copies allowlisted camera tags and omits ImageDescription/GPS; JPEG and TIFF retain output ICC.
- Existing format/file-only discovery, nested-output pruning, metadata/source-thumbnail handling, migration, warnings, pagination and resource-detection regressions remain covered.
- UI: load/save, preview request, before/after, full-resolution export request, reset, errors/cancellation, HEIC capability messaging.

The separately invoked large-image test writes a 6000×7000 RGB8 TIFF (over 126 MB), develops the original and exports RGB16 TIFF at the same 42 MP dimensions (over 252 MB). It passed in release mode in 2.28 seconds (test body; not a camera decode or UI performance benchmark). It is intentionally excluded from the fast default suite.

## Reproduce

In the project folder on this workstation:

```powershell
. ./scripts/activate-msvc.ps1
npm run prepare:native
npm run format:check
npm run lint
npm test
npm run build
cargo fmt --all -- --check
cargo test -p photo-core -p photo-contracts --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p photo-core --test rendering --release --locked large_tiff_full_resolution_export_uses_original_dimensions -- --ignored --nocapture
npm run desktop:build -- --no-bundle
```

The sandbox blocked esbuild ancestor-directory inspection. Frontend build/tests passed when rerun with normal filesystem access. Native compiler discovery initially saw duplicate PATH/Path entries; the process-local setup script now preserves the Visual Studio path. These environment issues were not hidden by disabling checks.

## Canon and Sony manual acceptance still required

Use a job whose input is a read-only copy or existing source folder and whose output is a separate test folder.

1. Select a real Canon CR3/CR2; inspect original metadata/preview and open Develop selected photo.
2. Update Preview at neutral. Compare orientation, white balance, tonal detail and obvious demosaic artifacts with a trusted RAW viewer; expect camera picture-style differences.
3. Apply exposure +1 EV, then warmer/cooler temperature and tint. Check highlights/shadows, contrast, saturation/vibrance and black/white behavior.
4. Apply slight rotation (for example 1.2°), then a valid normalized crop. Inspect corners and composition.
5. Compare original/source and edited previews. Test reset, persistence after restart, cancel and preview regeneration after deleting only cache.
6. Export JPEG quality 95 and RGB16 TIFF. Open both in an external color-managed editor/viewer. Verify full dimensions after geometry/crop, sRGB ICC, orientation, color and file integrity; assess sharpening/NR at 100%.
7. Export again and confirm suffixing, not overwrite. Verify source hashes/mtime remain unchanged and exported metadata excludes GPS/paths/history.
8. Repeat with a real Sony ARW, recording exact model, firmware and compression mode. Record pass/fail/visual issues separately for each camera.
9. Repeat JPEG, the user's large TIFF and representative profiles/compressions. Observe UI responsiveness, peak RAM, elapsed time and disk use on the 16 GB minimum / 32 GB development targets.
10. Rescan with output nested inside input; exported files must remain excluded. Test permission denied, disconnected sources, corrupt files and interrupted jobs.

Repeat platform/build/resource discovery and color-managed output viewing on an actual Apple Silicon Mac. No Mac claim should be inferred from source compatibility or CI configuration.
