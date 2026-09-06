# Phase 7 implementation inventory — trained styles / adaptive AI editing

All Phase 7 development remains inside PhotoEditor. It consumes the existing Phase 4 PhotoAnalysis, Phase 6 BatchContext, Phase 3 EditRecipe and deterministic renderer; it does not modify PhotographerApp, source pixels or built-in preset behavior.

- `crates/photo-contracts/src/trained_style.rs`, `tests/trained_style.rs`: strict versioned TrainedStyle package, `style_features_v1`, packaged linear model, safe bounds, missingness, prediction diagnostics, integrity and replaceable `StyleResolver` boundary.
- `crates/photo-core/src/trained_styles/{features,package,resolver,storage,mod}.rs`, migration `010_trained_styles.sql`, core `lib.rs`/`repository.rs`: canonical package loading/checksums, typed feature building from PhotoAnalysis + AssetBatchContext, adaptive linear inference, per-asset recipe conversion/provenance, idempotent replacement, SQLite progress/inference persistence, stale detection, cancellation and per-asset failure continuation.
- `styles/adaptive-natural-development/`, `scripts/prepare-styles.mjs`, `package.json`, `src-tauri/tauri.conf.json`: clearly labeled development-only Adaptive Natural package with style/model/rules/metadata/checksum files, copied into native resources without network or licensing claims.
- `src/trained-styles.ts`, `src/api.ts`, `src/components/StyleInferenceInspector.tsx`, `src/screens/PresetEditingScreen.tsx`, `src/styles.css`, Tauri `commands.rs`/`lib.rs`: AI style listing/state/apply/progress/cancel IPC, separate AI-style chooser, exact persisted-selection guard, simple progress/cancel UI, recipe-rendered previews and collapsed inference details. Built-in POP/WARM/BLACK & WHITE remain available and Export All follows the existing recipe path.
- `crates/photo-core/tests/trained_styles.rs`, `src/test/presets.test.tsx`: adaptive dark/bright and warm/cool behavior, BatchContext directionality, three-frame consistency, bounds/provenance/idempotence, exact selected-only processing, preview updates and cancellation coverage.
- `docs/TRAINED_STYLES.md`, this inventory, `ARCHITECTURE.md`, `VERIFICATION.md`, `LIMITATIONS.md`: runtime contract, package format, evidence, performance and Phase 8 boundary.

The v1 resolver is a deterministic packaged linear model, not a hardcoded preset and not a training pipeline. The development package is explicitly not trained from the photographer's photographs.

# Phase 6 implementation inventory — batch context

All Phase 6 development remains inside PhotoEditor. It adds source-batch context without changing PhotoAnalysis, culling ratings, EditRecipe, renderer behavior, built-in presets, PhotographerApp, or network behavior.

- `crates/photo-contracts/src/batch_context.rs`, contract `lib.rs`, `tests/batch_context.rs`: authoritative schema 1, independent scene/lighting/sequence groups, ranked references, per-asset availability/relative source relationships, diagnostics, canonical bounded JSON and strict validation.
- `crates/photo-core/src/batch_context/{mod,storage}.rs`, migration `009_batch_context.sql`, core `lib.rs`/`repository.rs`: exact selection/source/analysis/culling identity, current-selection-only loading, 64-anchor scene candidates, 27-neighbor lighting index, Phase 5 sequence reuse, real-estate bracket classification, reference uncertainty, SQLite cache/history, background progress, cancellation and startup interruption recovery.
- `src/batch-context.ts`, `src/components/BatchContextInspector.tsx`, `src/api.ts`, `src/screens/PresetEditingScreen.tsx`, `src/styles.css`, Tauri `commands.rs`/`lib.rs`: typed desktop commands plus a collapsed editing inspector for cache state, grouping progress/cancel, per-asset context and scene-member validation. It does not apply edits or run automatically.
- `crates/photo-core/tests/batch_context.rs`, core storage unit coverage, `src/test/{batch-context-fixture,batch-context.test}.tsx`: scene/burst separation, cross-scene lighting, bracket handling, relative exposure/color, weak/equivalent references, photo-type timing, RAW/JPEG companions, selection/source identity, recipe independence, unavailable evidence, cancellation, cache history and bounded 1,000/3,000-item regressions.
- `crates/photo-core/examples/{batch_context_benchmark,real_batch_context_acceptance}.rs`: release scaling/SQLite timings and production-service validation on the local portrait set without committing photographs.
- `docs/{BATCH_CONTEXT,ARCHITECTURE,IMPLEMENTATION,VERIFICATION,LIMITATIONS}.md`: contract, methods, evidence and explicit limitations.

## Preserved Phase 5 implementation inventory — culling and preset editing UX

All Phase 5 development remains in PhotoEditor. Production culling code, desktop/UI integration, fixtures, tests and documentation changed. Existing Phase 4 work is preserved; no PhotographerApp, cloud AI, Git commits or pushes.

- `crates/photo-contracts/src/culling.rs`, contract `lib.rs`, `tests/culling.rs`: authoritative bounded v2 assessments, explicit exact/near/burst/similar/unique semantics, nested exact-family and visual relationships, content/generation/membership bindings, structured reasons, explicit read-only v1 upgrade and validation.
- `crates/photo-core/src/culling/{mod,content,features,score,similarity,storage}.rs`: orchestration, streaming full-file SHA-256 with safe OS-generation reuse, job-wide exact buckets and stable-ID canonical, bounded visual grouping, exact-only 1★ downgrade, calibrated reliable face-detail thresholds, modest near/burst ranking, connected-group invalidation and atomic publication. The overview derives photographer-facing blurry/closed-eye issue types only from confident engine reasons and reports provider availability. A bounded atomic asset-ID snapshot command implements Select Shown exactly. Similar Composition remains uncollapsed. Model/eye providers, rating thresholds and image-processing algorithms are unchanged in this UX pass.
- `crates/photo-core/migrations/{007_culling,008_duplicate_content}.sql`, core `lib.rs`/`repository.rs`, `tests/foundation.rs`: migration 8 adds only the content cache; relationships remain in existing immutable assessment JSON. User rating/selection remain separate. Windows uses the existing windows crate's additional FileSystem feature; no new dependency package for hashing.
- `crates/photo-core/tests/culling.rs`: synthetic/structured photographic logic, real local YuNet smoke, integration/cache/override/selection/rollback/cancellation/recipe-independent persistence and release timings.
- `crates/photo-face-helper/{Cargo.toml,src/main.rs}`, root Cargo workspace/lock: bounded isolated CPU YuNet runner, using the existing exact ort dependency; no new third-party package version updates.
- `scripts/prepare-culling.mjs`, `package.json`: pinned model/license preparation plus release-helper compilation/copy into existing bundled toolkit resources.
- `src/culling.ts`, `src/api.ts`, `src/screens/{CullingScreen,JobScreen}.tsx`, `src/styles.css`: v2 DTOs and background commands; primary All/5★/4★+/3★+, duplicate Show/Hide, issue toggles and Show All; quiet BEST/DUPLICATE/SIMILAR/BLURRY/CLOSED EYES cards; prominent showing/selection counts; filter-derived exact selection replacement with optimistic checkbox state; explicit Clear Selection; and the persisted-only Run for Editing boundary. Duplicate hiding keeps and selects only the exact canonical and one deterministic Near/Burst display representative—even when immutable scoring evidence retains a tie—while never hiding Similar Composition alone. Advanced relationship/exact-star/selected-only filters, counts, progress/hash/source diagnostics, rating overrides and provenance remain in the inspector/development details.
- `crates/photo-core/src/presets.rs`, `crates/photo-core/tests/presets.rs`, `src/presets.ts`, `src/screens/PresetEditingScreen.tsx`, `src/components/Thumbnail.tsx`: typed built-in POP/WARM/BLACK & WHITE definitions and resolver, explicit persisted-selection scope guard, per-selected-asset validated recipe persistence, strict provenance, objective-field preservation, idempotent replacement, automatic cached-or-generated Subject masks for POP, isolated per-asset failures, progressive cancellable recipe rendering, recipe-keyed edited contact-sheet previews, sequential Export All through the existing full-resolution renderer and the existing DevelopmentPanel entry point. POP is global-neutral and subject-only +0.35 EV; WARM is relative +500 K via the 6500-neutral renderer convention.
- `src/test/{culling-fixture.json,culling.test.tsx}`: complete shared Phase 5 fixture validated by Rust; photographer filters, simple cards, issue availability, automatic exact selection snapshots, optimistic/manual checkbox state, Clear/filter-reset behavior, persistence, keyboard/error/cancellation/inspector/reopen regressions.
- `src-tauri/src/{commands,lib}.rs`: culling service configuration plus local preset definitions/state/apply commands, including bounded exact-ID selection snapshots, shared source/analysis services and no runtime network.
- `docs/{AI_CULLING,PRESET_EDITING,ARCHITECTURE,IMPLEMENTATION,VERIFICATION,LIMITATIONS,MODEL-NOTICES}.md`: design, preset contract, scoring/model/source notices, evidence and explicit gaps.

Actual eye inference is deferred; structured blink tests do not disguise that limitation. See [AI_CULLING.md](AI_CULLING.md).

Completion regressions cover content identity across names/folders, tiny nonidentical images, known burst versus later similar composition, deterministic canonical/preferred ranking, exact-only downgrade, unavailable identity and undecodable exact bytes, override/selection/restart/recipe independence, same-size/mtime edits, overlapping stale relationships, incremental exact/near discovery, cancelled/resumed exact batches, zero-byte hash reuse and 500/1,000/3,000 structured grouping. Calibration follow-ups cover realistic 5/4/3/2/1 portrait combinations, measured face detail 0.154 → 1★ even when preferred, five severe frames with a relative winner but all 1★, sharp subject plus low global detail and focused/unknown-eye 5★ without an eye claim. UX follow-ups cover Show All, exact/Near/Burst hiding without Similar Composition suppression, one display representative despite technical ties, blurry default/filter/label, closed-eye availability, effective-rating filters, cross-page automatic selection, Clear Selection, immediate manual checkbox refinement, filter-driven reselection, persisted-only Run for Editing state and quiet cards. Preset regressions inspect actual JPEG and supported-RAW preview pixels, cache replacement, automatic mask reuse/generation, subject-only POP exposure and safe individual failures. `crates/photo-core/examples/real_culling_acceptance.rs` runs the production services against the local recursive portrait set using workspace-contained temporary state. Earlier synthetic near-group fixtures intentionally differ by a pixel instead of accidentally being exact byte copies. Reusable frontend fixtures follow the authoritative v2 contract. See [VERIFICATION.md](VERIFICATION.md) for executed counts and real-job results.

## Historical Phase 4 implementation inventory

All Phase 4 work is inside PhotoEditor. No PhotographerApp changes or automatic Git push. No new ML model or edit behavior is introduced.

- `crates/photo-contracts/src/analysis.rs`, `src/lib.rs`, `tests/analysis.rs`: authoritative PhotoAnalysis v1, PhotoType, observations, safe bounded loading/validation; replace unused untyped analysis placeholder.
- Root `Cargo.toml`: enable serde_json's existing float_roundtrip feature for exact saved measurement reloads; no new dependency package.
- `crates/photo-core/src/analysis/{mod,measure,storage}.rs`: objective measurements, photo-type composition, provider isolation/reuse, bounded/cancellable service, cache/invalidation, transactional persistence and safe JSON export.
- `crates/photo-core/migrations/006_photo_analysis.sql`, `src/{repository,lib}.rs`: independent analysis records/status, filtering indexes and startup recovery.
- `crates/photo-core/src/rendering/analysis_input.rs`, `rendering/{mod,masks}.rs`: read-only source proxy/mask access and provider identity; unchanged creative pixel algorithms.
- `crates/photo-core/tests/analysis.rs`, `tests/support/synthetic.rs`, `tests/{rendering,foundation}.rs`: new measurement/service/integration tests; share existing synthetic DNG generator unchanged; migration expectation now 6.
- `src/analysis.ts`, `src/api.ts`, `src/components/{AnalysisInspector,DevelopmentPanel}.tsx`, `src/styles.css`: view-only DTOs/commands, lazy source inspector, optional geometry diagram, JSON inspection/export.
- `src/test/{analysis.test.tsx,analysis-fixture.json}`: inspector tests; synthetic Rust-produced JSON also validated by Rust tests.
- `src-tauri/src/{commands,lib}.rs`: isolated service/cache configuration and five background/control commands.
- `docs/{PHOTO_ANALYSIS,ARCHITECTURE,IMPLEMENTATION,VERIFICATION,LIMITATIONS}.md`: Phase 4 contract, measurements, architecture, evidence and known gaps.

No face identity, sky model, semantic scene clustering or complete consumer wizard. Phase 7's local adaptive style resolver is documented above; existing recipe/development assertions remain intact.

## Historical Phase 3 implementation inventory

All development stayed inside PhotoEditor. No PhotographerApp changes, Git commits or pushes. Photographic algorithms and native model assets were preserved.

## Phase 3 changed files

| Files                                                                                 | Change                                                                                                                                                                                  |
| ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| crates/photo-contracts/src/{recipe,lib}.rs, Cargo.toml; Cargo.lock                    | Typed recipe v1, grouping, validation/upgrades/canonical hashing/diff/template; reuse existing SHA-256/chrono dependencies                                                              |
| crates/photo-contracts/tests/recipe.rs                                                | 12 recipe validation, bridge, normalization, hash, diff and template tests                                                                                                              |
| crates/photo-core/migrations/005_recipes.sql                                          | Current recipe, indexed revision history and recovery archives                                                                                                                          |
| crates/photo-core/src/{recipes,repository,development,lib}.rs                         | Lazy migration, transactional optimistic saves/snapshots/restores/import/export and authoritative orchestration                                                                         |
| crates/photo-core/src/rendering/{recipe,mod,optics}.rs                                | Effective recipe resolution; actual mask/profile cache dependencies; unchanged photographic pipeline                                                                                    |
| crates/photo-core/tests/recipes.rs; tests/{foundation,toolkit}.rs                     | Recipe persistence, rollback/recovery/retention/portability/render/cache/3,000-asset tests; schema version 5 expectation; pixel comparisons exclude time-varying ICC container metadata |
| src/{recipe,api,types}.ts                                                             | Typed recipe DTOs, control-view adapter and recipe IPC                                                                                                                                  |
| src/components/{DevelopmentPanel,ToolkitControls,RecipeInspector}.tsx; src/styles.css | Recipe state authority, reset commit points, JSON/history/diff/restore tools                                                                                                            |
| src/test/{development.test.tsx,recipe-fixture.ts}                                     | Recipe API regression tests, generation-safe saves, Inspector/import/restore/reset/recovery                                                                                             |
| src-tauri/src/{commands,lib}.rs                                                       | Background recipe commands and registration                                                                                                                                             |
| README.md; docs/{ARCHITECTURE,IMPLEMENTATION,LIMITATIONS,VERIFICATION,EDIT_RECIPE}.md | Contract, lifecycle, scope, evidence and remaining acceptance                                                                                                                           |

No new segmentation model or photographic control was introduced. No .gitignore changes were needed: generated files remain in ignored .tools, .resources, target, dist and node_modules directories. Full recipe storage/hashing/history/import behavior is documented in [EDIT_RECIPE.md](EDIT_RECIPE.md). Verification results and manual gaps are in [VERIFICATION.md](VERIFICATION.md).

## Phase 8 Training Studio implementation

Phase 8 adds `photo-contracts::training` and `photo-core::training` as a local supervised recipe-learning boundary. Migration 011 persists datasets, target-cache entries, run progress/recovery, and correction-feedback capacity. The service reuses PhotoAnalysis, Phase 6 grouping, Phase 7 features/package loader/resolver, and the deterministic renderer. It adds conservative pairing, same-photo/geometry diagnostics, a staged photographic target optimizer, confidence-weighted regularized linear training, deterministic scene-aware holdout, recipe/rendered metrics, neutral and mean baselines, atomic versioned package export, cancellation, previews, and validation feedback. The desktop Training Studio exposes that workflow separately from normal Editing and installs successful packages into AI Styles immediately. Full semantics are in [TRAINING_STUDIO.md](TRAINING_STUDIO.md).

## Historical Phase 2.1 implementation inventory

The following inventory records the earlier Phase 2.1 delivery. Its then-deferred recipe work is implemented above; AI, cloud/auth/API and signing remain deferred.

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
