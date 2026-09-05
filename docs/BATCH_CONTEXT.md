# Phase 6 batch context

BatchContext describes how the **current persisted editing selection** relates as source photography. It is not PhotoAnalysis, culling policy, a style, or an edit recipe.

```text
PhotoAnalysis + BatchContext + future TrainedStyle -> future EditRecipe
```

Only the first two inputs exist today. Phase 6 never writes an EditRecipe, changes a rating, normalizes a histogram, or adjusts exposure, white balance, tone, color, masks, or exports. POP, WARM and BLACK & WHITE continue through the independent deterministic preset resolver.

## Versioned contract

`photo_contracts::batch_context::BatchContext` schema 1 contains:

- stable batch and deterministic selection identities, job, photo type, creation time, analysis/grouping versions and canonically sorted selected asset IDs;
- independent scene and lighting groups;
- sequence groups with Burst, Repeated Frames, or Exposure Bracket kind and an optional Phase 5 source-group identity;
- one AssetBatchContext for every selected asset, including available/partial/unavailable state, group references, an optional technical reference, relative source exposure/color relationships, confidence and typed consistency notes;
- explicit ranked reference candidates for scene and lighting groups; and
- bounded-work counts, availability counts, stage timings and warnings.

The contract rejects unknown fields, future schemas, duplicate/unsorted selection IDs, foreign group members, invalid references, non-finite or out-of-range measurements, oversized strings and payloads above 8 MiB. At most 5,000 assets are accepted. No pixel, mask, descriptor grid, recipe or preset payload is duplicated into BatchContext.

## Exact input scope and identity

The service reads `culling_user_state.selected=1`; it never expands to every job asset. Identity hashes the sorted asset IDs, current ingestion source fingerprints, current PhotoAnalysis IDs, current CullingAssessment IDs, photo type, PhotoAnalysis schema and grouping/descriptor versions. UI order is absent.

Changing selection, source evidence, current PhotoAnalysis, current culling descriptor evidence, or grouping code produces a different identity. Recipe edits, built-in presets, local masks and exports are absent and therefore do not invalidate context.

SQLite migration 9 stores complete bounded JSON contexts keyed by batch/selection identity plus a separate progress row. An exact identity is reused after reopening. A changed identity receives bounded full regrouping; old identities remain cached, so returning to an earlier exact selection can reuse its context. Phase 6 does not yet attempt risky partial graph splicing between similar selections.

## Scene groups

Scene grouping is conservative and deterministic:

1. Reuse current Phase 5 Near Duplicate/Burst group evidence directly.
2. Sort remaining analyzed assets by capture time and stable asset ID.
3. Compare only the most recent 64 group anchors using the existing difference hash, spatial luminance/color grids, aspect ratio, capture time and camera evidence.
4. Require high visual similarity plus a photo-type time window: Portrait 3 minutes, Real Estate 15 minutes, Landscape 30 minutes. Missing timestamps require an exceptionally close near-duplicate match.
5. A separate two-second same-camera/lens/orientation/aspect fallback can connect RAW/JPEG companions whose processed descriptor color differs. Filename adjacency is never evidence.

Every candidate must match the group anchor, limiting transitive chaining and favoring several small groups over one weak large group. Singleton groups are valid and intentionally low-confidence.

Photo type changes the temporal/visual boundary: portrait setup continuity is tight; real-estate adjacent views have more time but stricter visual matching; landscape light/series continuity can span longer while requiring the strongest visual similarity.

## Lighting groups

Lighting groups are independent from scene groups. They use Phase 4 source measurements: warm/cool and green/magenta balance, dynamic range, saturation, source exposure, subject light when available, and mixed-light evidence. A bounded three-axis bucket index searches at most 27 neighboring lighting anchors; a photo-type-weighted distance then decides membership.

- Portrait emphasizes subject-light continuity.
- Real Estate gives mixed-light and source-exposure relationships more weight.
- Landscape emphasizes color/light progression and dynamic-range continuity.

Related rooms can therefore remain separate scenes while sharing one lighting group. Exposure is intentionally a modest term: a darker frame from the same setup need not become a different lighting condition.

## Sequences and brackets

Current Phase 5 Burst/Near group identities are referenced rather than copied into another descriptor store. When Phase 5 evidence is absent, a same-scene capture span up to eight seconds can form a lower-confidence repeated-frame sequence.

A Phase 5 bracket signal always produces an Exposure Bracket sequence. Real Estate can also classify a close visual/time sequence as a bracket when source exposure span is at least 0.55 EV. The members keep their signed exposure relationships, but the Bracket Member note prevents consumers from treating that intentional span as arbitrary inconsistency. Phase 6 does not merge or HDR-process bracket frames.

## Technical references

References are consistency anchors, not artistic winners. Candidate scoring reuses Phase 5 absolute technical score/confidence and rejects severe source-unavailable, clipping, or subject-softness evidence. Phase 4-only candidates use lower confidence. A score below 65, confidence below 0.45, or severe clipping prevents candidacy.

Candidates within 2.5 technical points of the strongest candidate are retained, up to three, with deterministic score/confidence/asset-ID ordering. A weak group has no reference. Asset context prefers the lighting-group rank-one candidate, then its scene candidate; absence remains explicit.

## Relative source relationships

Exposure context is the signed difference between each source median luminance in log2 space and the lighting-group median. A negative value means the source appears darker; a positive value means brighter. It is context, not an EV correction.

White-balance context stores signed differences from the lighting-group median on Phase 4's warm/cool and green/magenta observation axes. These are dimensionless source signals, not Kelvin or renderer tint instructions. Typed notes call out meaningful differences while preserving numeric values and uncertainty.

## Background work and inspector

The desktop service reserves one cancellable Batch Context task, loads existing source evidence with `completed / total` progress, groups on a blocking worker, publishes one validated SQLite snapshot, and retains the last complete cache on cancellation/failure. One missing or invalid analysis becomes an unavailable per-asset context; the rest continue.

Editing contains a collapsed development-oriented Batch Context inspector. It shows selection/group/reference counts, cache/stale state, progress/cancel controls, selected-asset scene/lighting/sequence membership, relative exposure/color, confidence, and a simple scene-member view. It does not run automatically or add controls to the photographer-facing preset chooser.

## Complexity and measured release performance

Scene work is bounded by 64 anchors per asset. Lighting indexing examines at most 27 buckets. Sequence and context passes are linear apart from bounded candidate work and small deterministic sorts. The synthetic release benchmark includes cached-input materialization, grouping/context creation, validation/serialization and SQLite persistence, but not RAW decoding or Phase 4 inference.

| Assets | Input materialization | Candidate | Grouping | Context | SQLite persistence | Total  | Comparisons |
| ------ | --------------------- | --------- | -------- | ------- | ------------------ | ------ | ----------- |
| 100    | 1 ms                  | <1 ms     | <1 ms    | <1 ms   | 11 ms              | 13 ms  | 289         |
| 500    | 2 ms                  | <1 ms     | 1 ms     | 2 ms    | 16 ms              | 23 ms  | 2,371       |
| 1,000  | 4 ms                  | <1 ms     | 3 ms     | 8 ms    | 18 ms              | 35 ms  | 7,437       |
| 3,000  | 14 ms                 | <1 ms     | 22 ms    | 68 ms   | 42 ms              | 151 ms | 54,829      |

These timings are structured CPU/SQLite measurements on one development machine, not end-to-end 3,000-RAW latency or photographic accuracy certification. Real-photo results are recorded in [VERIFICATION.md](VERIFICATION.md).

## Future trained-style boundary

A future resolver can accept immutable source evidence without changing this lifecycle:

```text
resolve_style(PhotoAnalysis, AssetBatchContext, TrainedStyle) -> EditRecipe
```

That resolver, trained styles, recipe inference and consistency edits are deliberately not implemented in Phase 6.
