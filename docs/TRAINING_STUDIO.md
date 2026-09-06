# Phase 8 — Training Studio v1

Training Studio is a top-level authoring workflow, independent of Jobs, Culling, and Editing. It learns a photographer's **creative recipe choices**, not finished pixels. Its deployed path is:

`source → PhotoAnalysis → BatchContext → target EditRecipe estimation → style_features_v1 + recipe controls → regularized model → Phase 7 package`

At inference time the unchanged Phase 7 path loads that package, predicts bounded creative controls, produces an ordinary EditRecipe, and uses the deterministic renderer. Source and reference files are always read-only.

## Pairing and contracts

Folder import reads every supported file recursively, skips hidden/system files and known app cache/output directories, and reports traversal failures instead of silently skipping inaccessible subtrees. Import mutations are serialized: overlapping Before and After requests cannot overwrite each other's persisted selections. Image pickers select one file; folder pickers enumerate the selected directory. Natural numeric ordering puts `(2 of 47)` before `(10 of 47)`.

Match / Validate now runs as one cancellable worker. Its bounded progress snapshot reports `scanning_before`, `scanning_after`, `sorting`, `building_pair_candidates`, `structural_validation`, `finalizing_matches`, and `complete`. Scanning here rechecks imported files; directory enumeration happens during Add Folder. Progress counts describe actual work within the current stage; the percentage is explicitly a stage percentage. The UI appears immediately, polls serially every 250 ms, and disables input/training controls until the worker settles. Only one matching worker can run at a time, including through direct IPC.

The existing structural validator runs on candidates without starting target estimation or training. Candidate mappings and validation are saved together only after completion; cancellation or failure retains the previously saved dataset. Renderer/cache work may remain reusable. Renamed equal-count exports receive natural-order candidates, with first/last alignment based on structural validation. Unequal counts never trigger index pairing. Explicit manual mappings take priority over order fallback. Dataset Ready exposes Train Style nearby; successful rows are collapsed by default, and Before/After review previews are available before target fitting.

`photo-contracts::training` defines version-1 `TrainingPair`, `TrainingDataset`, target-result, split, metric, configuration, and persisted `TrainingRun` contracts. Unknown fields and incompatible versions fail safely. Pair and dataset identity include full-file SHA-256 fingerprints; distributable packages never include source or reference paths.

Sources use the formats already developable through the existing raster/LibRaw boundary: CR3, CR2, ARW, DNG, JPEG, TIFF, and PNG are primary. The actual LibRaw camera/compression support still varies. Finished references are JPEG, TIFF, or PNG. HEIC/HEIF remains unsupported because it has no full development path.

Before and after files are selected explicitly, either as a single image, a multi-file selection, or a folder. Matching removes only conservative suffixes such as `_EDIT`, `-edited`, and `_final`, then requires a unique normalized filename stem on both sides. Multiple source assets or references with the same stem are reported as ambiguous and are not paired automatically. The first/last identity report is a sanity check, never the only match rule. When every folder has the same count but filenames differ, ordered candidates are offered for structural review; a missing middle file never shifts later exact matches. Unmatched before/after files are shown; there is no fuzzy or semantic filename matching.

Validation checks readability/decoding, supported format, plausible dimensions/aspect, and a luminance-normalized structural descriptor. Geometry is recorded as Exact/Near, Crop Difference, Major Mismatch, or Unusable. Moderate centered crops require review instead of automatic rejection. The structure check tolerates exposure, white-balance, saturation, and contrast changes, but deliberately rejects obvious wrong-photo matches. It is conservative evidence, not identity proof.

Dataset size is guidance, not a gate: fewer than 20 pairs is marked experimental, 20–50 is a reasonable first style, and larger sets are described as broader coverage while still requiring holdout review. A small dataset can still train if it has usable targets.

## Target recipe estimation

`TargetRecipeOptimizer` is independent of the trainer. The v1 `StagedTargetOptimizer` decodes an oriented source/reference pair through the existing engine, reduces it to a bounded working proxy, and estimates ten global creative controls:

- exposure and source-relative temperature delta;
- tint, highlights, shadows, whites, and blacks;
- saturation, vibrance, and clarity.

Contrast, dehaze, curves, HSL, detail, vignette, local masks, crop, and rotation remain neutral/not learned in v1. Objective orientation, optics, and camera behavior stay in the deterministic pipeline and are not style targets.

The optimizer starts from a median-luminance exposure estimate, then runs bounded deterministic coordinate stages for exposure/WB, tone, and color/presence. It never performs an uncontrolled slider Cartesian search. The loss combines tonal quantiles, normalized color balance, saturation, and a structure descriptor over the common centered region. Each result stores the controls, valid target EditRecipe, loss breakdown, iterations, unsupported-difference notes, and High/Medium/Low fit confidence. Low-confidence targets are excluded by default; if enabled later, their weight is 0.2 versus 0.65 for Medium and 1.0 for High.

The UI exposes Source / AI Edit (after application) / Target Recipe Render / Reference side by side. A low numeric loss is not proof of photographic equivalence; bad targets should be excluded before training.

## Features, split, and model

Training calls the existing Phase 4 analysis and the exact Phase 7 `style_features_v1` builder. Related usable pairs are fed to the Phase 6 grouping implementation. If context cannot be built, an explicit unavailable AssetBatchContext is used rather than invented values.

Split assignment is deterministic from dataset and stable scene-group identities. The default is approximately 80/20 and guarantees one validation example when possible. Every scene group stays entirely on one side, so burst/near-duplicate leakage does not make validation artificially easy. If all examples form one group, training proceeds with a clear warning that a leakage-safe holdout was impossible.

`StyleModelTrainer` is replaceable. V1 uses one coordinated Phase 7 `linear_v1` package with regularized, confidence-weighted per-control regression. It computes weighted feature means and standard deviations from training examples only. Those exact normalization vectors are stored in `model.json`; Phase 7 applies them before inference. Missing features retain their explicit availability behavior. Outputs are clamped to conservative training bounds and then to package/renderer bounds.

Target controls are already expressed in renderer units, so v1 does not require separate target normalization. Recipe-space metrics report per-control MAE and normalized mean MAE. Rendered validation loss is calculated against held-out references for the trained model, neutral recipe, and weighted mean recipe. Training and validation error are both shown. The UI warns about substantial overfitting and explicitly reports when the model fails to beat the mean-recipe baseline; that run must not be claimed as a learning success.

## Persistence, cache, and cancellation

SQLite migration 011 stores datasets, target cache entries, runs, and a future-compatible feedback table. A run records stage, progress, configuration, status, metrics, version, artifact path, duration, and error. Startup changes queued/running runs to Interrupted.

Target cache identity is SHA-256 over full source fingerprint, full reference fingerprint, renderer version, optimizer version, ordered allowed-control set, and relevant mask-model version. Changing only trainer configuration or model version reuses target work. Changing either file or a renderer/optimizer/control/mask dependency invalidates it. Phase 4 analysis retains its own versioned cache.

Only one bounded training run is active per service. Pair analysis and target optimization are currently sequential, intentionally limiting memory on 32 GB-class machines. Cancellation is checked between stages, pair operations, optimizer controls, and renderer work. Cached completed targets remain reusable; temporary package directories are validated first and renamed atomically, so a cancelled/failed run is never published as a valid style.

## Package and workflow

Every successful run creates a new directory under the application's local `trained-styles` root, such as `jeoffrey-portrait-v1`, then `-v2`. Existing versions are never overwritten. It contains the actual Phase 7 files: `style.json`, `model.json`, `rules.json`, `metadata.json`, and `checksums.json`. Metadata includes dataset identity, pair counts, schemas, trainer/renderer versions, training time, and metric summary, but no personal paths or pixels. Canonical SHA-256 integrity is validated through the Phase 7 loader before publication.

The desktop installs the completed package into the live AI Styles catalog immediately. The top-level Presets section lists both built-in and trained styles, and trained versions remain available after restart. Review Matches shows each mapped before/after row with status; ambiguous or unmatched files can be paired manually, then must pass the same structural validation. Validation pairs can be reviewed as Source / AI edit / Target recipe render / Reference and marked Accept, Needs Adjustment, or Reject. Feedback is persisted for future correction learning; v1 does not retrain continuously or store manual recipe deltas.

## Verification and honest limits

Programmatic tests generate a known reference through the real deterministic renderer and verify that target fitting approximately recovers its exposure while materially beating neutral loss. Other tests cover strict contracts, matching ambiguity, deterministic scene-aware holdout, adaptive dark-versus-bright predictions on unseen features, normalization persistence, package integrity/versioning, Phase 7 loading, UI diagnostics/previews/baselines, validation handoff, feedback, and cancellation.

The local portrait culling corpus has CR3/JPEG camera companions but no explicitly identified finished photographer edits. It is therefore not used to claim real personal-style training. Human acceptance still requires 20–30 genuine before/after pairs, held-out Original / AI Edit / Reference review, and photographer labels such as Very Close, Directionally Correct, or Wrong.
