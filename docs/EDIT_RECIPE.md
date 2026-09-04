# Edit Recipe contract — Phase 3

Analysis = what the image is.

Style = what the photographer wants.

Recipe = what to do to this individual image.

Renderer = executes the recipe.

Analysis and trained styles remain separate, unimplemented provider boundaries. A future generator may combine image analysis and a trained style to emit different corrections for every asset. One trained style does **not** mean a fixed adjustment vector applied to a folder. No AI decisions, training, cloud or PhotographerApp integration are implemented here.

## Authority and entry points

Rust `photo-contracts::EditRecipe` is authoritative. `RECIPE_SCHEMA_VERSION = 1` is independent of the application version, legacy adjustment schema 2, renderer version and SQLite schema 5.

React stores a recipe, derives a control view using `recipeControls`, and maps control changes back to that recipe. It saves through recipe IPC with an expected generation. Neither preview nor mask requests carry a second adjustment vector. The backend validates, resolves objective/mask dependencies, translates once to the existing renderer vocabulary and executes the unchanged photographic stages.

Public, UI-independent APIs:

- Contract: `parse_recipe`, `EditRecipe::validated`, `canonical_json`, `content_hash`, `adjustments`, `diff_recipes`.
- Repository: `get_recipe`, `save_recipe`, `create_revision`, `recipe_history`, `revision_recipe`, `restore_revision`, `recipe_diff`, `import_recipe`, `import_recipe_file`, `export_recipe`.
- Renderer: `CpuProcessingEngine::effective_recipe`, `render_recipe`.
- Job orchestration: `DevelopmentService::render_recipe`, `recipe_mask`, and serialized `with_recipes`.

Legacy `RenderAdjustments`, `ProcessingEngine::render` and old development methods remain low-level/compatibility APIs. Compatibility saves go through recipe validation/persistence. `DevelopmentState.adjustments` and SQLite `development_state.adjustments_json` are projections, not competing sources of edit truth. New clients use the recipe APIs.

## Schema

Required envelope: schema_version, recipe_id, asset_id, created_at, updated_at. IDs are bounded printable strings; timestamps are RFC 3339. Each asset **within a job** has one independent current recipe. Reusing the same original in two jobs does not share its edits.

Typed groups:

- global.basic: exposure_ev, temperature, tint, contrast, highlights, shadows, whites, blacks, saturation, vibrance.
- global.curve: master/red/green/blue ordered points.
- global.color_mixer: named red/orange/yellow/green/aqua/blue/purple/magenta bands.
- global.presence: texture, clarity, dehaze.
- global.detail: sharpening and noise structures, plus legacy_sharpening and legacy_noise_reduction.
- global.optics: objective profile switches, component strengths and manual optical fallbacks.
- global.effects: creative post-crop vignette.
- global.geometry: rotation_degrees and normalized crop.
- local_layers: ordered, individually identified logical-mask edits.
- metadata: optional scene_cluster_id, sequence_id, reference_asset_id, consistency_group_id, consistency_note, confidence, needs_review.
- provenance: origin, created_by, source_recipe_id, style_id, model_id, model_version, analysis_id, manually_modified, nullable acceptance.

No RAW, image analysis measurements, model weights, mask pixels, selected-slider state, overlays or machine cache paths are embedded. Recipe JSON is capped at 256 KiB; individual strings at 1024 bytes; at most eight local layers.

### Numeric units and neutral defaults

| Controls                                                                  | Units / bounds                                                                                    | Neutral                      |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------- |
| Exposure                                                                  | stops (EV), −5…+5                                                                                 | 0                            |
| Temperature                                                               | relative WB control 2000…12000 K; **not measured as-shot Kelvin**                                 | 6500                         |
| Tint, contrast, highlights, shadows, whites, blacks, saturation, vibrance | −100…100                                                                                          | 0                            |
| Curve points                                                              | normalized x/y in [0,1]; 2…16 points; x strictly increasing, endpoints x=0 and 1, y nondecreasing | (0,0), (1,1)                 |
| HSL hue / saturation / luminance                                          | −100…100; renderer maps to ±30°, 0…2 saturation factor, ±1 EV                                     | 0                            |
| Texture, clarity, dehaze                                                  | −100…100                                                                                          | 0                            |
| Sharpening amount/detail/masking                                          | 0…100                                                                                             | 0 / 25 / 0                   |
| Sharpening radius                                                         | reference-pixel scale 0.5…3                                                                       | 1                            |
| NR luminance / color                                                      | 0…100                                                                                             | 0                            |
| NR luminance_detail / color_detail                                        | 0…100                                                                                             | 50                           |
| Legacy sharpening / NR                                                    | 0…100, earlier Phase 2 stages                                                                     | 0                            |
| Optics profile enable                                                     | boolean                                                                                           | false                        |
| Distortion / optical vignette / CA switches                               | boolean                                                                                           | true, inactive until enabled |
| Profile distortion / vignette strength                                    | 0…1                                                                                               | 1                            |
| Manual optical distortion / vignette                                      | −100…100, existing polynomial / peripheral-EV controls                                            | 0                            |
| Creative vignette amount                                                  | −100…100 (up to ±2 EV)                                                                            | 0                            |
| Creative midpoint / feather / roundness                                   | 0…100 / 1…100 / −100…100                                                                          | 50 / 75 / 0                  |
| Rotation                                                                  | degrees, accepted −36000…36000; normalized to [−180,180)                                          | 0                            |
| Crop x/y/width/height                                                     | fractions of rotated canvas; inside [0,1], positive width/height                                  | 0/0/1/1                      |
| Local strength                                                            | 0…1                                                                                               | supplied explicitly          |
| Confidence                                                                | nullable 0…1                                                                                      | null, never fabricated       |

Luminance NR contrast and crop aspect-lock do not exist in the renderer and are not invented in the schema. Legacy detail controls are **not aliases** of expanded detail; preserving both maintains the old pipeline stages and pixels.

Absent optional groups/controls deserialize to these neutral defaults. Required envelope fields cannot silently default. Local layers require their ID, selector, enabled state and strength; invert defaults false, controls neutral, references/confidence null.

## Objective versus creative

Sensor calibration, demosaic, orientation and camera color conversion are source/decoder responsibilities, not learnable creative recipe parameters. Source-derived OpticsMetadata travels separately. The recipe's optics group selects objective correction preferences; the resolved lens profile identity, database version and applied components live in render diagnostics. They are not baked into a transferable creative preset.

Exposure, relative WB preference, tone, curve, color mixer, presence, local editing, preferred detail, creative vignette and geometry are creative intent. Local layers cannot contain geometry, global curves/HSL or lens corrections; their type exposes only the currently implemented local toolset.

## Local masks and portability

Each RecipeLayer has a stable id, mask_type, enabled, strength, invert, optional confidence, optional MaskReference and typed LocalAdjustments. Layers execute in vector order. Subject/background are supported. The historical custom enum is retained as an explicitly unresolved/unsupported selector; unknown kinds such as sky are rejected, not approximated as subject.

MaskReference contains a SHA-256 content_id and optional SHA-256 source_fingerprint, model_id, model_version and geometry_version. These identify derived content, never a caller-chosen filesystem location. A legacy reference lacks some optional descriptors but must still equal the current source/decoder/model cache identity to be used.

A null reference means **resolve the logical selector against this target asset's own cache**. It does not mean reuse the last image's mask. Generate Masks remains explicit; unavailable/mismatched masks disable only the effective local operation, retain stored intent and emit warnings. Global export remains possible.

Import always strips source-bound mask references and mask confidence, including on same-asset import because the underlying source may have changed. Resolution can immediately use a compatible mask already cached for the target; otherwise the layer remains unresolved until regeneration. The imported source's alpha file is never read or copied. The source_fingerprint hashes the existing canonical-path/size/mtime identity; it is not a cryptographic digest of all RAW bytes.

Mask geometry version is `oriented-source-optics-geometry-v1`. The existing oriented-source 16-bit alpha is transformed through shared optical/crop/rotation coordinates; changing creative edits does not regenerate segmentation.

## Parsing, normalization, upgrades

`parse_recipe` bounds size, reads the explicit version, upgrades supported old envelopes, deserializes typed fields, then validates before renderer access. It rejects future versions clearly; unknown fields, NaN/infinity, bad curves, duplicate IDs, invalid strengths, incompatible local fields, invalid optics and crops fail safely.

The first shipped full recipe contract is v1. There were no previously shipped full v1 operations recipes: the old operations struct was an unused placeholder. It is not silently reinterpreted.

An explicit v0 **interchange bridge** is accepted for legacy adjustment payloads:

```json
{
  "schema_version": 0,
  "recipe_id": "legacy-example",
  "asset_id": "asset-example",
  "created_at": "2026-09-04T00:00:00Z",
  "updated_at": "2026-09-04T00:00:00Z",
  "adjustments": { "schema_version": 1, "exposure_ev": 0.4, "sharpening": 12 }
}
```

This upgrades to a grouped v1 recipe with migrated provenance. Actual Phase 2/2.1 SQLite rows are converted directly through the same lossless adjustment bridge, not falsely relabeled as full recipe v1. Future schema changes should add isolated, tested upgrade steps before changing the current version.

Normalization wraps rotation, replaces negative zero, and collapses redundant identity curves to their endpoints. Nonidentity curve and local-layer order are preserved. JSON uses recursively sorted object keys, compact UTF-8, normalized zeros and deterministic serde numeric serialization. It is this application's canonical representation, not a claim of RFC 8785 compliance. Repeated parse/serialize is stable.

## Hashes and effective rendering

The SHA-256 recipe content hash is a version-tagged canonical projection of global settings and ordered enabled/nonzero-strength local intent. It excludes asset/recipe/layer IDs, clocks, provenance, confidence, review/scene metadata and all UI state. Disabled/zero-strength layers do not affect output identity. Their full settings remain in storage/history and semantic diffs.

This is conservative edit identity, not algebraic equivalence of arbitrary pipelines: changing a stored radius while amount is zero can still rekey, as can inactive optical suboptions. No attempt is made to prove commutativity or remove every neutral enabled local stage.

EffectiveRenderRecipe adds source identity, decoder, validated mask sample hash/dimensions, reference/model/status, geometry version, actual loaded lens XML content digest/database version, and source objective metadata. The mask digest hashes validated f32 samples decoded from 16-bit alpha, so same-key replacements, corruption or deletion invalidate affected previews. The profile digest is fixed for an engine's loaded database; rebuilding the engine after database changes rekeys.

Final preview keys combine source fingerprint, recipe hash, dependency hash, renderer version, backend and fixed reduced-preview/quality contract. Optics/local previews can now reuse cache safely without the prior blanket bypass. Diagnostic/warning sidecars are disposable; missing/invalid sidecars cause rerender. Source/dependencies are rechecked before publishing output and before accepting cached previews. Overlays never enter rendering or hashes.

The renderer's photographic version remains `photo-cpu-linear-srgb-v2.1`: no pixel algorithms changed. The recipe cache contract has its own version tag. Same source/recipe/masks/profiles/platform gives stable pixels; full RAW export versus reduced preview can differ in demosaic/spatial sampling, and cross-platform floating-point/codec tolerances remain. Little CMS writes the profile creation clock into embedded ICC metadata, and exported photographic metadata can also vary; entire file bytes/hashes are therefore not guaranteed identical. Pixel determinism tests compare decoded samples, not time-stamped file containers. Same-size/same-mtime source replacement is an inherited fingerprint limitation.

## SQLite migration and recovery

Migration 005_recipes.sql adds:

- asset_recipes: per-job/asset current canonical JSON, schema/hash/origin, optimistic generation, current revision and timestamps/error.
- recipe_revisions: UUID identity, monotonically increasing per-asset revision number, canonical snapshot/hash/origin/reason/time; unique asset/revision and descending lookup index.
- recipe_recovery: exact damaged payload and structured error/time retained locally.

Migration is additive. Old checkpoints, output paths, other future processing_state columns and legacy adjustment payloads remain intact during conversion. Existing edits convert **lazily on first recipe access**, once per asset under an immediate transaction. A grid of 3,000 assets does not instantiate recipes or deserialize histories. New untouched assets conceptually have independent neutral recipes and materialize on first access. History queries select metadata only, paginate to at most 100, and fetch snapshot bodies individually for restore/diff.

Recipe writes use SQLite immediate transactions and expected-generation checks. Current JSON/hash/schema/origin, optional history and legacy checkpoint projection commit together. A stale writer is rejected. An injected SQL failure test verifies rollback after snapshot insertion. Normal saves preserve the recipe's identity and creation timestamp.

Corrupt current payloads, hash/binding mismatches and unsupported stored schemas return a structured error plus a safe neutral display fallback; the asset/grid remains. Normal save/render/export is blocked until explicit reset/import/restore recovery. Corrupt current text stays in place until recovery archives it atomically. Corrupt legacy JSON is archived when a neutral recovery record is first materialized, and its legacy row is retained until explicit recovery. Recovery archives have no automatic purge. Damaged revision restore fails without replacing current edits.

## Revisions, reset, provenance and retention

Sliders persist only the current draft; they do not make snapshots. Explicit Update Preview, photo export, Save Snapshot, reset, import and restore are commit points. Auto-preview never makes a revision. Unchanged repeat commits deduplicate while meaningful provenance/metadata changes can be retained; timestamps alone do not make duplicates.

Reset/import/restore first retain any unsnapshotted state they replace, then capture the replacement in the same transaction. Existing Reset All, Reset Global, section resets and individual Subject/Background resets all use the same recipe path. Restoring creates a new increasing revision; it does not rewrite history.

Retention is bounded at **200 snapshots per asset: the original revision plus the latest 199**. Intermediate older snapshots are pruned transactionally when this limit is exceeded; there is no full permanent training-event log. Current drafts, initial evidence, recent revision identities/times/provenance and recovery archives remain. Export valuable recipes before long sessions if permanent retention is required. No training or upload uses this history.

Origins manual/imported/migrated/system are used today. Reserved trained_style/ai_generated/correction/batch_consistency and optional source/style/model/analysis IDs support future attribution only. A manual edit marks manually_modified and clears acceptance without replacing a future AI origin. Nullable accepted/rejected evidence is schema preparation, not an implemented feedback or Needs Review workflow. Import records the source recipe ID and imported origin, clears source-bound confidence/review/acceptance, and retains optional provenance identifiers.

There is no command-stack undo/redo. Durable revision restore is the minimum implemented history mechanism. Drafts survive restart after their asynchronous save completes; leaving an asset does not force an extra revision. Snapshot explicitly before closing if its history milestone matters.

## Semantic comparison

diff_recipes compares validated typed global groups recursively, uses human-readable group/control labels and explicit units, aligns local changes by stable IDs, and reports vector ordering separately. It reports control values, curve point vectors, enable/strength/invert changes, mask bindings and added/removed layers. It does not compare serialized JSON strings. Provenance/scene/review metadata is excluded from the control diff; full snapshots still retain it.

## JSON tools and Inspector

Inspector displays schema, ID, saved hash, origin, current revision, local count, unresolved masks and modified/saving state. JSON/history load on demand. History shows the latest 100 entries with semantic comparison and restore; the core supports pagination for the remaining retained snapshots.

Export writes compact canonical JSON in the job's output directory with a safe source stem and UUID suffix, for example IMG_1234-<uuid>.recipe.json. No-clobber publication protects originals and existing recipe files. Import uses the native JSON-file picker, a bounded read, parse/upgrade/validation, target binding and transactional revision. A failure leaves current edits intact. After import/restore, use Update Preview to render the new state; the old preview is marked stale.

No copy-to-all workflow, trained preset UI, full template UI or batch-AI orchestration exists. RecipeTemplate is a lightweight concrete-intent contract that instantiates independent asset-bound recipes and always strips derived mask bindings. It is not a trained style.
