# Culling → Preset Editing MVP

The saved culling selection is the boundary into local editing. `Run for Editing`
does not re-cull, change ratings, or change the selection; it opens the preset
screen with the persisted asset IDs. The screen creates one authoritative Phase 3
recipe per selected asset, then shows reduced previews rendered through that
recipe. It never substitutes the discovery/source thumbnail for an edited result.
Originals are never changed.

## Built-in presets

Presets are typed and resolved in `photo-core::presets`, outside React. Each
application replaces the creative recipe portion while retaining the asset-bound
recipe identity, objective optics/geometry, and recipe metadata. Provenance is
`origin=system`, `created_by=photo-editor/built-in-preset`, `style_id=<id>`,
`model_id=built-in-preset`, and `model_version=1`.

| Preset        | Global creative recipe                                                                                 | Local layers                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| POP           | All global basic/tone/color/presence/detail values neutral; 6500 is the renderer’s neutral relative WB | One logical Subject layer, exposure **+0.35 EV**, no embedded mask |
| WARM          | Relative WB **+500 K** (`temperature=7000`, because 6500 is neutral), tint **+2**, vibrance **+4**     | None                                                               |
| BLACK & WHITE | Saturation **−100**                                                                                    | None                                                               |

POP never applies a global +0.35 EV compensation. The preset screen asks the
existing Phase 2.1 mask service to reuse a valid cached Subject mask or generate a
missing one before rendering. Mask preparation and reduced-preview rendering run
one asset at a time through the existing cancellable worker boundary, so results
appear progressively without unbounded inference. If a mask remains absent,
stale, or failed, the existing effective-recipe resolver disables only that local
layer, records the asset as needing attention, and renders it unchanged. Other
assets continue.

Every contact-sheet preview uses the existing Phase 2/2.1 renderer cache. Its
identity contains the effective recipe hash and dependency hash, not a preset
name. Changing BLACK & WHITE to WARM therefore produces a new color preview and
cannot reuse a stale monochrome result. These are reduced proxy renders; only an
explicit later export performs full-resolution rendering.

## Replacement and persistence

The editing screen always reloads the persisted culling selection. It never
derives scope from the job, culling visibility, or stale handoff state. Applying
a built-in preset sends those asset IDs explicitly; the core compares that exact
set with the current persisted snapshot and rejects a stale or merged request
before changing any recipe. Switching presets therefore updates the same selected
assets and leaves every unselected recipe untouched.

Applying the same preset twice compares the resolved recipe before saving, so the
second application is unchanged and does not create a generation or history
revision. Choosing another built-in preset clears the prior built-in creative
values and resolves the new baseline (for example POP → WARM removes the Subject
layer). Selection, recipes, and provenance are stored in the existing SQLite
recipe/culling tables and are restored when the job is reopened.

## Export All

Export All processes only the current persisted editing selection. It sends one
full-resolution recipe render at a time through the existing cancellable
development worker, continues after individual failures, and reports exported and
failed counts. The existing renderer writes JPEG at the application's existing
default quality for this batch action, uses the job output folder, preserves the
metadata policy and collision-safe naming, and never overwrites a source or prior
export. A POP asset whose mask remains unresolved follows the existing effective
recipe semantics: the local layer is disabled and the unchanged result is safely
exported rather than receiving global exposure.

The current screen is intentionally an MVP: there is no trained-style resolver,
AI generation, job-level persisted batch format preference, or cloud dependency.
The existing DevelopmentPanel remains available by selecting a contact-sheet
thumbnail and retains its individual JPEG/TIFF controls.
