# Phase 6 batch context verification

Date: 2026-09-05, Windows x64 MSVC, Rust 1.98.1, Node 22.20.0. All Phase 6 work and generated acceptance state stayed inside PhotoEditor. No PhotographerApp changes, commits or pushes. Existing source photographs were read but not modified or committed.

## Phase 6 completion checks

| Check                                                   | Result                                                                                                                     |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `npm run format:check`                                  | Passed                                                                                                                     |
| `npm run lint`                                          | Passed, no warnings                                                                                                        |
| `npm test`                                              | **78 passed**, 7 files: 4 format + 3 batch context + 6 analysis + 10 preset + 11 UI + 14 development + 30 culling          |
| `npm run build`                                         | Passed, TypeScript + Vite; `index-D0LGPus7.js` and `index-Jzo15UQZ.css`                                                    |
| `cargo fmt --all -- --check`                            | Passed                                                                                                                     |
| `cargo test -p photo-core -p photo-contracts --locked`  | **207 passed**, no failures; 1 separate 42 MP case intentionally ignored in the default run                                |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed                                                                                                                     |
| `cargo test -p photo-face-helper --locked`              | **2 passed**                                                                                                               |
| Separate release 42 MP TIFF regression                  | **1 passed**, preserving original full-resolution dimensions                                                               |
| Windows x64 MSVC `npm run desktop:build -- --no-bundle` | Passed; release executable built successfully                                                                              |
| `git diff --check`                                      | Passed; line-ending notices only, no whitespace errors                                                                     |
| Production-service portrait validation                  | Passed on 52 distinct local photographs for both the exact current six-photo selection and an expanded 52-photo inspection |

Distinct Rust total: **210** = 207 default photo-core/photo-contracts tests + 2 face-helper tests + 1 explicit release 42 MP test. No remaining failures.

Final executable: `target/release/photo-editor-desktop.exe`, **21,058,560 bytes**, SHA-256 `8D921037EFF721B82AD176ED88D70A27ACA617A323A2E2D0EA7917D39717D22D`, rebuilt **2026-09-05 16:04:16 local**. This is a Windows no-bundle executable build, not installer, signing, distribution, macOS or interactive UI acceptance. Keep the prepared native resource folders and licenses with the executable.

## Phase 6 evidence

- Contract/core focused tests pass for conservative scene grouping, clearly separate scenes, lighting shared across scenes, explicit real-estate brackets, darker/warmer relative source context, weak/no references, stable equivalent references, photo-type timing, same-capture RAW/JPEG companions, deterministic order-independent identity, selection/source invalidation, recipe independence, nonfatal unavailable analysis, cancellation, SQLite cache history and bounded 1,000/3,000-item work.
- Three frontend inspector tests pass alongside all ten built-in-preset tests. They cover cached summaries and group navigation, stale-selection rebuild/progress/cancellation, and unavailable source context. Existing preset behavior is unchanged.
- The final release benchmark completed 100/500/1,000/3,000 structured assets in 13/23/35/151 ms including input materialization, validated context creation and SQLite persistence. Stage details are in [BATCH_CONTEXT.md](BATCH_CONTEXT.md). The 3,000 case made 54,829 bounded comparisons, not an all-pairs pass.
- The final production-service portrait pass used the exact current 5★/Duplicates Hide/Hide Blurry result: six selected photos (`IMG_3804.CR3`, `IMG_3824.JPG`, both `IMG_3909` variants, `IMG_4093.JPG`, `IMG_4161.JPG`), six scene groups, one lighting group, zero sequences, six unique reference assets, and no unavailable analysis. Loading/current-identity checks plus grouping and persistence took 137 ms after cached Phase 4/5 evidence existed.
- Expanded 52-photo inspection produced 31 conservative scene groups, two lighting groups, ten sequences, 40 unique reference assets and no unavailable analysis in 714 ms. It recognized the IMG_4161–IMG_4164 JPG burst and the continuous nine-photo IMG_4224–IMG_4232 bridge series; printed multi-photo memberships showed no obvious cross-location collapse. The earlier pass motivated the same-capture camera/lens/orientation RAW/JPEG fallback now covered by regression. This is one portrait shoot, not broad photographic accuracy evidence.

## Historical Phase 5 culling → preset editing MVP verification

Date: 2026-09-05, Windows x64 MSVC (Visual Studio 18 / compiler 14.51), Rust 1.98.1, Node 22.20.0. All development edits, generated fixtures, caches and builds are inside PhotoEditor. No PhotographerApp changes, commits or pushes. The production engine was run recursively against `test-photos/Portraits`: 53 assets representing 52 distinct photographs because `IMG_4161.CR3` is intentionally present in both Blurry and Duplicates. The calibrated scorer and source photographs were not modified.

## Completion checks

| Check                                                   | Result                                                                                                  |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `npm run format:check`                                  | Passed                                                                                                  |
| `npm run lint`                                          | Passed, no warnings                                                                                     |
| `npm test`                                              | **75 passed**, 6 files: 4 format + 11 UI + 14 development + 6 analysis + 30 culling + 10 preset editing |
| `npm run build`                                         | Passed, TypeScript + Vite                                                                               |
| `cargo fmt --all -- --check`                            | Passed                                                                                                  |
| `cargo test -p photo-core -p photo-contracts --locked`  | **193 passed**, no failures; 42 MP case intentionally opt-in                                            |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed                                                                                                  |
| `cargo test -p photo-face-helper --locked`              | **2 passed**                                                                                            |
| Separate release 42 MP TIFF regression                  | **1 passed**, 2.37 s test body                                                                          |
| Windows x64 MSVC `npm run desktop:build -- --no-bundle` | Passed; release profile 31.22 s, unsigned no-bundle executable and resources                            |
| `git diff --check`                                      | Passed; line-ending notices only, no whitespace errors                                                  |
| Recursive real portrait acceptance                      | **53/53**, 0 failures; 10×5★, 37×4★, 6×1★ including one exact blurry copy                               |
| Real mixed JPEG/CR3 preset/export acceptance            | **5/5** B&W, WARM and POP; **5 selected / 5 exported / 0 failed**, exactly 5 output files               |
| Interactive desktop / macOS                             | Not run; no claim                                                                                       |

## Preset editing regression

The built-in preset suite verifies the corrected contract: POP has neutral global creative controls and one Subject layer at +0.35 EV; missing masks are generated, valid masks are reused, and failed masks disable only that local layer; WARM is relative +500 K (7000 on the 6500-neutral renderer); BLACK & WHITE is saturation -100; selected assets each receive a valid recipe with provenance; repeated POP is idempotent; POP → WARM removes the Subject layer; objective optics/geometry survive; and reopening restores selection and applied preset. Pixel assertions cover monochrome JPEG and supported-RAW previews, WARM changes versus a neutral source render, recipe-hash cache replacement, subject-only POP changes and unchanged background/failure output. Frontend coverage verifies filter-derived selection replacement, optimistic and persisted checkbox state, exact editing handoff, edited rather than source contact-sheet images, progressive mask/render status, per-asset failure continuation, cancellation, preset change without stale previews, empty-selection protection, Back to Culling, neutral-recipe export, selected-only sequential Export All progress, per-file failure continuation and active-export cancellation.

Distinct Rust total: **196** = 193 default workspace tests + 2 face-helper tests + 1 explicit 42 MP test. The selection/export fix added **1 preset-core test** and **4 frontend preset tests**; this auto-selection follow-up adds **2 frontend culling tests** and updates the earlier explicit-selection assertions without removing their coverage. Production React culling workflow code and the real acceptance harness changed; selection persistence, preset definitions, recipe validation, cache-key architecture, source editing and culling scoring remain intact. No remaining failures.

Final executable: `target/release/photo-editor-desktop.exe`, **20,468,736 bytes**, SHA-256 `2624F97FBD6087C1118974FE6CFD4FA370C65AAA6D855254666EDD76A5C41453`, rebuilt **2026-09-05 12:55:52 local**, including automatic filter-derived selection, selected-only preset application and Export All. Frontend bundle: `index-83ePIypC.js`; stylesheet: `index-a_Gvg9Ac.css`. Prepared `raw/`, `exiftool/` and `toolkit/` folders, face helper, YuNet model and licenses accompany the executable; keep them together. The model/license pins are unchanged. This is Windows build verification, not native interactive, installer, signing or distribution acceptance.

### Added evidence

- The primary workflow is reduced to four rating bands, duplicate Show/Hide, issue toggles, Show All, Clear Selection and Run for Editing. Select Shown is removed: membership-changing filters now define and persist the editing snapshot automatically. Detailed relationship controls, counts, processing diagnostics, sorting and rating override remain available in the inspector or collapsed Development details rather than on every card.
- Duplicate Hide keeps one deterministic preferred/canonical member of exact, near-duplicate and burst groups, even when the scorer retains several candidates inside its one-point technical tie tolerance. The underlying tie evidence remains intact in the inspector. Similar-composition-only photographs remain visible. BEST, DUPLICATE and SIMILAR are display labels derived from persisted relationship evidence, not new classifications.
- Hide Blurry defaults on and uses only the existing confident severe-subject-softness reason. CLOSED EYES is an explicit issue type, but its filter is disabled and marked unavailable while the shipping eye provider reports no model; no closed-eye result is fabricated.
- Rating, duplicate visibility and available issue filters immediately replace local checkbox state with every matching asset ID across all pages, then serialize the same full snapshot through the existing persistence command. The 45-selected/5★-matching regression proves the filter itself replaces the stale 45 with five, checks all five visible boxes and hands exactly those IDs to Run for Editing. Manual checkbox changes are immediate and persist for the current filters; Clear remains at zero until another filter change recomputes selection. Run for Editing is disabled while persistence is pending or the selected set is empty.
- Every built-in preset command now carries the editing asset IDs explicitly. A central core guard rejects duplicate, stale, foreign or otherwise nonmatching scopes before recipe resolution. The 52-asset regression starts with 45 selected, replaces the selection with five, rejects the stale 45-ID call, then proves B&W, WARM and POP change only those five while all 47 unselected recipe payloads, generations and hashes remain unchanged.
- Export All is available for valid neutral/as-shot recipes as well as applied presets. It rechecks the persisted snapshot, then submits one full-resolution commit at a time through the existing cancellable development renderer. It uses the configured job output folder, collision-safe naming, source-preserving metadata policy and the existing DevelopmentPanel JPEG quality-95 default because the job currently has no persisted output-format preference. A failure marks only that asset and the batch continues; cancellation stops before the next asset and preserves completed files.
- The editing grid passes no discovery thumbnail request after a preset is active. It displays only recipe-rendered data returned by the existing reduced-preview renderer. B&W → WARM clears the old in-memory image immediately, and the Phase 3 effective recipe/dependency hash produces a different disk-cache path.
- POP uses the existing recipe mask operation with `generate=true`; MaskCache returns a valid cached matte without inference or creates the missing MODNet matte. The UI serializes requests, reports mask and preview progress, supports cancellation, marks failed assets for attention and continues rendering the rest. No React pixel processing or global exposure fallback exists.
- The real preset/export acceptance now derives its IDs from the actual current filters after starting with an old 45-ID snapshot. The 52-distinct-photo corpus has ten 5★ files; Duplicates Hide removes one alternative from each of five Near/Burst pairs and Hide Blurry removes no remaining candidate, producing exactly five selected IDs. B&W rendered those five as monochrome (three JPEG and two RAW), WARM rendered five color/rekeyed previews, POP produced five ready masks and five subject-only visible changes with global exposure 0 and local Subject exposure +0.35, and the full-resolution renderer produced exactly five files with zero failures.
- Show All resets rating to All, sets Duplicates to Show, disables issue hiding and automatically selects every resulting photograph. It does not alter AI ratings, relationship evidence or manual rating overrides.
- Complete SHA-256 identity across equal names, renamed copies and nested folders; different bytes with equal names do not match, nor do tiny nonidentical pixel variations. Stable cache reuse and reopened database read zero full-content bytes; forced hashing is distinct and cancellation is respected.
- Exact canonical is deterministic under input permutation and remains normally rated. Redundant copies alone receive 1★, never a claim that identical pixels have worse measured quality. Exact families collapse for visual preference but retain both exact and burst/near relationships.
- Near/burst versus later similar composition and unrelated/flat frames; Similar Composition produces no relative star/score adjustment. Existing useful alternate/tie/bracket regressions remain intact.
- The pre-calibration `IMG_4161`–`IMG_4165.CR3` assessments all started at score 82 despite global sharpness around 0.0107–0.0109, subject sharpness around 0.0032–0.0046 and reliable important-face detail around 0.150–0.185. The old 0.10 face threshold mislabeled these as sharp, while the preferred-group bonus could not express the absolute defect.
- The calibrated scorer separates ungated relative ranking from final rating gates. A reliably detected important portrait face below 0.20 normalized detail fires the severe-subject-softness cap at score 19, after group preference is chosen; therefore a bad group still has a relative preferred frame while every severely soft member remains 1★. Strong and exceptional evidence remain capable of 4★ and 5★, and the structured regressions now cover every rating band.
- The production acceptance run completed all 53 recursive assets (928,109,996 bytes) in 64.883 seconds with no analysis or identity failures. It found one exact-copy group, two near groups, four burst groups, nine similar-composition groups, four unique assets and no unclassified assets. Duplicate Hide reduced 53 to 45 by hiding one exact copy and seven near/burst alternatives; all similar-composition-only photographs remained visible. Hide Blurry alone reduced 53 to 47, and the normal combined duplicate/blur view contains 40 assets.
- Ratings were 10×5★, 37×4★ and 6×1★. The five distinct `IMG_4161`–`IMG_4165.CR3` photographs remain severe-softness 1★ frames; the second byte-identical `IMG_4161.CR3` is the sixth 1★ asset. Their materially sharper JPG companions remain 4★. Intentional portrait background blur is protected by relevant-face sharpness, while landscape and real-estate scoring retain their distinct conservative/global-focus weighting.
- Focus diagnostics expose global, subject and relevant-face measurements and confidences, the visual-group median and selected/median ratio, outlier interpretation, internal/absolute scores and every fired cap. Group-focus references are persisted as ordinary reason evidence.
- Exact AI1 → user5 → effective5, forced re-cull/restart still user5, clear returns AI1. Manual checkbox refinements survive persistence, cancellation, re-culling and restart until the next filter change intentionally replaces the snapshot. Exposure, HSL and local-layer changes preserve exact relationships, assessment IDs and hash/feature reuse.
- Rescan discovers exact D then near E; only new content is hashed, previous feature measurements are reused and all related member counts/IDs refresh. New membership makes old complete relationships stale until resume.
- A header byte edit preserving file size and restored modification time changes the Windows generation token, invalidates overlapping exact/visual groups and does not reuse stale Phase 4 analysis. Unreadable identity is unclassified/nonfatal. Equal undecodable file bytes yield exact copies with no invented image-quality evidence: canonical unrated, redundant copies 1★ for redundancy.
- Strict v2 fixture and actual v1 parser upgrade; unknown/malformed/future data remain rejected. Partial and final snapshots use distinct immutable IDs, including source failures. Existing atomic rollback and user-state independence tests pass.
- UI counts distinguish extra copies, groups, unique and unclassified images. Exact/near+burst+similar/preferred/unique filters compose with effective stars; automatic filtered selection applies membership filters across all pages. Duplicates Hide retains one exact canonical and one Near/Burst representative while Similar Composition remains visible. Compact indicators, immediate/manual checkbox refinement, Clear/filter reset, canonical/related navigation, rating override, editing selection persistence, focus diagnostics and 24-thumbnail inspector pagination are exercised.
- YuNet and unavailable-eye behavior are unchanged. Real prepared YuNet blank-input inference plus all previous recipe/ingestion/analysis/toolkit/mask/optics/export/cancellation/debounce tests pass. Mock open/closed-eye cases remain policy fixtures, never model-accuracy claims.

### Release performance observations

Single test thread; fixtures generated locally; test body only (not compilation):

| Work                                                       | 500   | 1,000 | 3,000 |
| ---------------------------------------------------------- | ----- | ----- | ----- |
| Visual grouping, all distinct content records              | 15 ms | 31 ms | 97 ms |
| Exact buckets, repeated identities / 200 distinct contents | <1 ms | <1 ms | <1 ms |
| Visual grouping after collapsing those exact families      | 6 ms  | 7 ms  | 11 ms |

A 3,000-member single exact family also passes, outside the visual group/window bounds. These are structured grouping benchmarks, not full 3,000-photo decode/inference/database/UI timing.

Full-file hashing is separate: a recently written **32 MiB synthetic file took 28 ms**, cached identity lookup **1 ms**, **zero content bytes reread**. First hashing and forced re-culls scale with file bytes/storage speed; this warm local-file observation is not a cold RAW/network-disk throughput claim. A three-photo synthetic 256px cull with mock faces took 450 ms (including hashing/storage and refreshing a prior unbound analysis). Single synthetic measurement took 35 ms; actual YuNet blank helper smoke 115 ms; a 1,000-record similarity case took 30 ms. The completed local portrait job is useful calibration evidence, not broad photographic accuracy, latency or peak-RAM certification.

Jobs permit 5,000 catalog assets subject to the separate 64 MiB serialized evidence budget. Exact buckets are job-wide; visual matching retains 64 recent anchors and 32 distinct-content representatives per group. Summaries still validate SQLite/source identities across the job and helpers start per photo. Grid/inspector paging prevents rendering every related thumbnail at once.

### Reproduce completion checks

```powershell
. ./scripts/activate-msvc.ps1
npm run format:check
npm run lint
npm test
npm run build
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) '.tools/verify-msvc'
cargo fmt --all -- --check
cargo test -p photo-core -p photo-contracts --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p photo-face-helper --locked
Remove-Item Env:CARGO_TARGET_DIR
cargo test -p photo-core --test culling --release --locked -- --nocapture --test-threads=1
cargo run -p photo-core --example real_culling_acceptance --release --locked -- test-photos/Portraits
cargo test -p photo-core --test rendering --release --locked large_tiff_full_resolution_export_uses_original_dimensions -- --ignored --nocapture
npm run desktop:build -- --no-bundle
git diff --check
```

Run native preparation only after native helper/model tests finish to avoid Windows copy locks. Frontend tooling uses normal filesystem access for esbuild ancestor configuration, as in earlier phases. Hash-generation validation is implemented for Windows and Unix but only Windows runtime was exercised. First v2 binding or changed full content can refresh Phase 4 source analysis; unrelated render/ingestion source fingerprints remain unchanged. See [AI_CULLING.md](AI_CULLING.md), [IMPLEMENTATION.md](IMPLEMENTATION.md) and [LIMITATIONS.md](LIMITATIONS.md) for contracts, identity semantics, persistence, invalidation and known bounds.

## Historical Phase 5 initial verification record

Date: 2026-09-04, Windows x64 MSVC, Rust 1.98.1, Node 22.20.0. All development/test fixtures/build outputs stayed inside PhotoEditor. Existing Phase 4 changes were preserved. No PhotographerApp changes, Git commits or pushes. The user explicitly requested continuing **without real-photo acceptance testing**.

## Phase 5 final checks

| Check                                                    | Result                                                                                       |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `npm run format:check`                                   | Passed                                                                                       |
| `npm run lint`                                           | Passed, no warnings                                                                          |
| `npm test`                                               | **48 passed**, 5 files: 4 format + 11 existing UI + 14 development + 6 analysis + 13 culling |
| `npm run build`                                          | Passed, TypeScript + Vite                                                                    |
| `cargo fmt --all -- --check`                             | Passed                                                                                       |
| `cargo test -p photo-core -p photo-contracts --locked`   | **163 passed**, 0 failures; one opt-in 42 MP test excluded from this default command         |
| `cargo clippy --workspace --all-targets -- -D warnings`  | Passed                                                                                       |
| `cargo test -p photo-face-helper --locked`               | **2 passed**                                                                                 |
| Explicit release-mode 42 MP TIFF regression              | **1 passed**, 2.42 s test body                                                               |
| `npm run desktop:build -- --no-bundle`                   | Passed, Windows x64 MSVC; includes pinned model preparation and final frontend assets        |
| `git diff --check`                                       | Passed                                                                                       |
| Real photographs / interactive native acceptance / macOS | Not run; no claim                                                                            |

Rust total is 132 preserved default regressions + 4 new culling-contract + 27 new culling-core = **163 default tests**. Adding two isolated face-helper tests and the separately run large TIFF regression gives **166 distinct Rust tests executed**, all passing. Release timing/smoke re-runs are not double-counted. No remaining failures.

Final executable: `target/release/photo-editor-desktop.exe`, **20,155,392 bytes**, rebuilt 2026-09-04 12:18:01 local. This is a no-bundle executable build, not installer/signing/distribution verification. It must ship with its prepared resource folders and licenses. The final build embeds `index-CeFpVpO6.js`, matching the separately verified frontend build.

### New evidence

- Complete schema fixture parsed/roundtripped by Rust; star-domain enforcement; future/missing/unknown/oversized payload rejection; finite values, confidence, geometry, subject and feature bindings; unavailable eyes are not closed.
- Structured single/group sharp/open-eye cases; one blink attached to person 5; one soft face attached to person 4; combined group failures despite a sharp global frame; low-confidence closed/uncertain eyes never produce blink penalties. These are policy fixtures, **not an eye model**.
- Synthetic local sharp/blurred/directional-average pixels, bright/dark/clipping measurements, boundary geometry and conservative photo-type differences. Portrait low-detail single faces cannot receive excellent ratings; many cropped/clipped faces cannot multiply small framing/exposure penalties without a cap.
- Similarity of identical/near frames, different scene/same time, far-apart capture times, different camera, missing time, bracket-like luminance spread, preferred ranking and ties; alternatives are not forced to 1★. Persisted groups invalidate together after source changes and regroup on resume.
- SQLite restart persistence, original AI evidence/reasons/models and override events, explicit AI2 → user5 → AI1 → still user5, clear restores AI, snapshot selection/reopen, atomic rollback/cancel, immutable ID collision rejection. Original image bytes remain unchanged.
- Exposure/HSL/local subject recipe changes do not alter culling snapshots or trigger culling inference. Actual provider-version change invalidates/re-extracts features while reusing the same Phase 4 analysis ID. Source and analysis changes invalidate AI but preserve user rating/selection.
- Bounded reservation, active second-frame cancellation, first-frame preservation, feature reuse on resume, interrupted recovery, optional provider failure and corrupt-source not-rated behavior.
- **Actual prepared YuNet CPU model** loaded/inferred on synthetic blank pixels with zero detections and scratch cleanup. Missing-model path is explicit unavailable; no cloud or face-image fixture. Verified prepared model and license SHA-256 against pins. This proves helper/model compatibility, **not photographic detection accuracy**.
- Thirteen culling frontend tests: effective counts, all/5/4–5/3–5/arbitrary filters, sorting, manual override and clear without AI loss, saved include/exclude and reopen, preserved selection on rerun, cancellation and reopened progress, per-person/group inspector, safe keyboard handling, mutation errors, photo types, stale response suppression and off-page selection across a paged grid.
- All previous ingestion, format, recipe/history, toolkit/mask/optics, analysis, preview/export/cancellation and auto-preview debounce tests remain passing. No existing assertions were removed.

Initial implementation checks caught a missing TypeScript provider alias, a Rust test variable shadow, an ambiguous accumulator type and two Clippy slice-clone warnings; these were fixed, not suppressed. Final tests use valid source-bound culling fixtures. Frontend tests/build use normal filesystem access because restricted esbuild cannot inspect ancestor configuration. No privileged runtime service or external state was created.

### Approximate release timings

On this workstation, measured test bodies (not compilation; no photographic quality inference):

| Work                                                                                                                      | Time   |
| ------------------------------------------------------------------------------------------------------------------------- | ------ |
| Single synthetic 256×256 portrait, source analysis + feature/scoring, **mock face detector**, no subject model            | 40 ms  |
| Three synthetic 256×256 portraits, **mock face detector**, one Phase 4 analysis already warm, storage + grouping included | 366 ms |
| Group 1,000 structured feature records                                                                                    | 29 ms  |
| Actual YuNet, synthetic 128×128 blank source resized/padded to 640×640, process/session startup + inference + cleanup     | 113 ms |

These are approximate local observations, not representative real-photo batch benchmarks. Per-image ONNX session/helper startup and job-wide SQLite/source validation are likely bottlenecks. Culling remains CPU-only, bounded to 2,000 photos and a 64 MiB serialized feature/evidence budget. No real-image accuracy or larger-job latency target is certified.

### Reproduce Phase 5

```powershell
. ./scripts/activate-msvc.ps1
npm run prepare:native
npm run format:check
npm run lint
npm test
npm run build
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) '.tools/verify-msvc'
cargo fmt --all -- --check
cargo test -p photo-core -p photo-contracts --locked
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p photo-face-helper --locked
cargo test -p photo-core --test culling --release --locked synthetic_timing -- --nocapture
cargo test -p photo-core --test culling --release --locked missing_and_real_runtime -- --nocapture
cargo test -p photo-core --test rendering --release --locked large_tiff_full_resolution_export_uses_original_dimensions -- --ignored --nocapture
Remove-Item Env:CARGO_TARGET_DIR
npm run desktop:build -- --no-bundle
```

Native preparation and helper tests should not run concurrently: Windows may lock in-use model/runtime/helper files while preparation copies them. Unset the verification target override before native preparation/build, whose copy scripts use `target/release`.

See [AI_CULLING.md](AI_CULLING.md), [PRESET_EDITING.md](PRESET_EDITING.md) and [LIMITATIONS.md](LIMITATIONS.md) for the explicit unavailable eye-state provider, conservative heuristics and deferred work. No trained-style presets, AI recipe generation, scene-consistency editing or PhotographerApp integration was implemented in that earlier analysis phase.

## Historical Phase 4 verification record

Date: 2026-09-04, Windows x64/MSVC, Rust 1.98.1, Node 22.20.0. Development, temporary fixtures and build outputs stayed in PhotoEditor. No PhotographerApp changes, commits or pushes.

## Phase 4 checks

| Check                                                               | Result                                                                               |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| npm run format:check                                                | Passed                                                                               |
| npm run lint                                                        | Passed, zero warnings                                                                |
| npm test                                                            | **35 passed**: 4 format, 11 UI, 14 existing development, 6 new analysis inspector    |
| npm run build                                                       | Passed, TypeScript and Vite production build                                         |
| cargo fmt --all -- --check                                          | Passed                                                                               |
| cargo test -p photo-core -p photo-contracts --locked                | **132 passed**, one existing opt-in 42 MP test excluded from standard run            |
| cargo clippy --workspace --all-targets -- -D warnings               | Passed, zero warnings                                                                |
| Windows x64 MSVC desktop build, --no-bundle                         | Passed; target/release/photo-editor-desktop.exe, 18,978,816 bytes, built 08:39 local |
| Release-mode analysis timing integration                            | Passed (also included in standard test count; not counted twice)                     |
| Real Canon portrait CR3 / landscape / real-estate manual acceptance | Not run: no source path supplied                                                     |
| Native interactive inspector/visual acceptance                      | Not run; component tests and native build are not a substitute                       |
| Apple Silicon/macOS                                                 | Not run; no claim                                                                    |

Rust total: 4 new analysis-contract + 12 recipe-contract + 5 toolkit-contract + 1 process + 15 new analysis-core + 22 foundation + 16 ingestion + 13 recipe-core + 19 standard rendering + 25 toolkit = **132**. The separately invoked release-mode 42 MP TIFF export regression also passed (2.31 s), making **133 distinct Rust tests executed**; no remaining failures. Existing generated-DNG LibRaw, real MODNet CPU inference and pinned Lensfun database tests passed. All existing development assertions remain.

### New evidence

- Known dark/bright/clipped images, ordered finite percentiles and exact exposure-class threshold boundaries; low/high tonal contrast; neutral/warm/cool/green/magenta color and saturation separation.
- Clean/noisy synthetic images and sharp/blurred edges, bounded noise severity and conservative unavailable blur classification; minimum dimensions, non-finite pixels and cancellation.
- Generated level, +2° and −3° horizontal references with sign/tolerance/support checks; no credible line remains unavailable, not zero angle.
- Synthetic alpha bbox, centroid, occupancy, luminance/background EV relationship, empty subject behavior and bounded JSON without mask pixels.
- Portrait/landscape/real-estate types, valid common reuse, persisted exact numeric reload, diagnostics, current/future/missing/unknown/oversized schemas, confidence/box bounds and corrupt-record recovery.
- Source changes and actual model-version changes invalidate as intended; model changes reuse common measurements but rerun subject inference. Cache-key tests include engine/decoder/type/model identities.
- Explicit recipe-independence checks after exposure, HSL and local subject changes. Source bytes and recipe remain unchanged after analysis. A dedicated renderer test verifies that analysis-generated masks cannot activate unresolved renderer layers or change renderer dependency identity.
- Bounded queue, duplicate/active invalidation guard, queued cancellation, active provider cancellation, changed source during work, no partial publication, rerun and scratch/cache cleanup behavior. Failed subject provider leaves common measurements valid with warning.
- Six frontend tests: lazy loading/no automatic analysis, numeric/unavailable display, request fields contain no edits, optional geometry diagram, JSON export, photo types/cache reuse, invalidation/rerun, cancellation/retry, close/unmount cancellation and stale-response suppression.
- Frontend analysis-fixture.json came from the real synthetic Rust pipeline; contract/core tests parse it and compare its common metrics, preventing an invented response shape.

Initial new tests caught one-ULP JSON parse drift; enabling serde_json float_roundtrip fixed persistence rather than relaxing equality. The final boundary test also required comparing the saved pre-analysis recipe (including its save timestamp), not the unsaved construction object. Existing test assertions were not removed. No photographic renderer algorithm changed. Frontend tooling uses ordinary local access because the restricted launcher blocks esbuild configuration resolution through ancestor directories.

### Approximate analysis timings

Measured on this development host, release-mode core integration, including input preparation and SQLite publication but **excluding ingestion, fixture creation and portrait ML**:

| Generated input                                | Normalized input | Observed time |
| ---------------------------------------------- | ---------------- | ------------- |
| 1800×1200 PNG (normal)                         | 1600×1067        | 248 ms        |
| Two additional 1800×1200 PNGs                  | 1600×1067 each   | 245 / 246 ms  |
| Tiny generated Bayer DNG through actual LibRaw | 64×48 half-size  | 71 ms         |
| Sequential four-image batch                    | Above inputs     | 816 ms total  |

Earlier debug-mode measurements were approximately 2.37–2.44 s per PNG, 112 ms for the tiny DNG, and 7.33 s total. The DNG is deliberately tiny, **not a normal-size CR3 benchmark**. OS filesystem caches and current machine load affect timings. The loop is a bounded batch-ready API demonstration, not batch intelligence or an optimized throughput guarantee. Real 16 GB-machine, representative RAW, portrait quality and native UI acceptance remain unverified.

## Historical Phase 3 verification record

Date: 2026-09-04, Windows x64 host. Rust 1.98.1 MSVC, Visual Studio x64 C++ Build Tools, Node 22.20.0. All development/test/build files stayed in PhotoEditor. No PhotographerApp work, Git commit or push.

## Phase 3 executed checks

| Check                                                  | Result                                                                        |
| ------------------------------------------------------ | ----------------------------------------------------------------------------- |
| npm run format:check                                   | Passed after formatting                                                       |
| npm run lint                                           | Passed, zero warnings                                                         |
| npm test                                               | 26 passed: 4 format + 11 existing UI + 11 development-panel tests             |
| npm run build                                          | Passed TypeScript and Vite production build                                   |
| cargo fmt --all -- --check                             | Passed                                                                        |
| cargo test -p photo-core -p photo-contracts --locked   | 113 passed; one opt-in 42 MP test excluded from this standard run             |
| cargo clippy --workspace --all-targets -- -D warnings  | Passed, zero warnings                                                         |
| Windows x64 MSVC desktop build, --no-bundle            | Passed; executable under target/release with prepared local runtime resources |
| Existing real Canon/Sony job manual acceptance         | Not run: no job/source folder supplied                                        |
| Native interactive recipe round-trip / external viewer | Not run                                                                       |
| Apple Silicon                                          | Not run; no macOS claim                                                       |
| Installer/signing/notarization                         | Out of scope                                                                  |

The separate release-mode 42 MP TIFF regression also passed on 2026-09-04 (2.28 s observed test runtime). This makes **114 distinct Rust tests executed** in Phase 3, including the opt-in case; it is a generated TIFF check, not a Canon/Sony RAW benchmark.

Standard Rust count: 12 recipe-contract + 5 toolkit-contract + 1 process + 22 foundation + 16 ingestion + 13 recipe-core + 19 standard rendering + 25 toolkit integration = **113**. Existing synthetic LibRaw DNG, real MODNet CPU-model loading/inference and actual pinned lens-database tests were rerun as part of that suite. They do not certify real camera/lens/portrait behavior.

### New evidence

- Twelve contract tests exercise neutral/default recipes, required fields, unknown/future schemas, explicit v0 bridge, NaN/infinity/ranges, curves/crops/layers/optics, mask-reference bounds, canonical round trips, normalized rotation/zero/identity curves, behavior hashes, semantic diffs, complete legacy control translation and independent template instantiation.
- Thirteen core tests cover current/draft/snapshot/restore/restart independence, reset pre-state capture, duplicate suppression, generation conflicts, SQL-injected rollback, Phase 2/2.1 payload migration, corrupt current/legacy recovery with exact-payload retention, unique JSON export/import, cross-asset masks, 200-snapshot retention, recipe-driven pixels, preview/export agreement on equal-resolution fixtures, actual mask replacement/deletion/model invalidation, actual lens XML/metadata dependencies and lazy 3,000-asset grid access.
- Foreign asset mask references are rejected even when the target has its own ready cache. Null logical references resolve only to the target's own derived mask.
- The existing renderer remains exercised through its compatibility API and the new recipe entry point. Small deterministic fixtures verify exposure, HSL, optical controls, subject/background selectivity, repeated output and preview cache reuse/rekeying. Reduced RAW previews are not claimed pixel-identical to full-resolution exports.
- Frontend tests now use recipe saves/render requests, verify successive save generations, reset commit reasons, Inspector identity/JSON export/history comparison/restore/import and corrupt-data recovery. These are component tests with mocked IPC, not native acceptance.

The first full run correctly exposed an outdated SQLite schema expectation (4 versus new 5); that fixture was updated. A new output-path assertion was corrected to compare canonical Windows paths. A repeat run also caught an inherited whole-TIFF byte comparison crossing the Little CMS profile-creation timestamp boundary. Existing overlay and preview/export checks, and the new recipe comparisons, now assert decoded RGB16 samples; embedded-profile presence/color handling remain covered separately. The renderer was not changed to hide this metadata variability. Full workspace lint findings were fixed without suppressing warnings. A final native-preparation attempt overlapped a running ExifTool integration test and encountered a Windows DLL file lock; preparation/build was retried after the tests exited. Frontend tests/build require ordinary build-tool access because the restricted launcher prevents esbuild from resolving the workspace through ancestor directories.

## Reproduce Phase 3

From PhotoEditor:

```powershell
. ./scripts/activate-msvc.ps1
npm run prepare:native
npm run format:check
npm run lint
npm test
npm run build
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) '.tools/verify-msvc'
cargo fmt --all -- --check
cargo test -p photo-core -p photo-contracts --locked
cargo clippy --workspace --all-targets -- -D warnings
```

Use a fresh process with the normal target directory for native preparation and the desktop build:

```powershell
. ./scripts/activate-msvc.ps1
npm run desktop:build -- --no-bundle
```

The activation script keeps compiler/cache/temp settings process-local and project-local. No global toolchain configuration was changed. The isolated debug target avoids the stale transitive metadata previously observed in the old target/debug folder.

## Manual Phase 3 acceptance still required

Supply an existing PhotoEditor job or its Canon/Sony source folder. Keep originals unchanged and verification outputs inside PhotoEditor. No unrelated folders were searched for photographs.

1. Open a previously edited Canon and Sony RAW; verify old Phase 2/2.1 basic, curve/HSL/detail/optics and local values.
2. Inspect schema 1, recipe ID, hash, origin and revision. Change exposure, Update Preview, verify hash/pixels.
3. Generate/inspect the source's own mask; adjust Subject Exposure, render and inspect boundaries.
4. Export recipe JSON. Reset All, import it, Update Preview and verify creative edits return.
5. Capture multiple meaningful snapshots; compare controls and restore an earlier revision. Update Preview and compare output.
6. Close/reopen PhotoEditor and the job; verify current recipe and revision history persist.
7. Export JPEG, compare to the edited preview in a color-managed viewer (allow documented resolution/detail differences).
8. Import onto an unrelated photo. Confirm no source mask is reused; resolve/generate that target's own mask and verify only the intended regions change.
9. Verify all reset variants and that failed/unresolved masks remain explicitly reported.

No real photographic or native acceptance checkbox is marked complete by synthetic fixtures. Phase 4 decisions, analysis, style training, cloud/auth/licensing and new segmentation types remain intentionally unimplemented.

---

## Historical Phase 2.1 verification record

Date: 2026-09-03, Windows 11 x64. Rust 1.98.1 MSVC, Visual Studio x64 C++ Build Tools, Node 22.20.0. All source changes, downloads, caches and generated tests stayed inside PhotoEditor. PhotographerApp was not modified.

### Historical executed checks

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
