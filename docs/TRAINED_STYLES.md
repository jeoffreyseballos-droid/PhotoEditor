# Phase 7 — Trained Styles / Adaptive AI Editing v1

Phase 7 adds a local, replaceable creative-editing runtime. The flow is:

`PhotoAnalysis + current BatchContext + TrainedStyle package → StylePrediction → EditRecipe → deterministic renderer`

The model predicts controls; it never renders pixels, changes source files, or replaces objective corrections.

## Contract and package

`photo-contracts::trained_style` is the versioned boundary. The current package, model, rules, metadata, integrity, feature-vector and prediction schemas are all version 1. `style_features_v1` is the stable feature schema. Unknown fields, unsupported versions, non-finite values, path traversal, mismatched dimensions and incompatible renderer/recipe versions fail safely.

The bundled development package is:

```text
styles/adaptive-natural-development/
  style.json       # identity, supported Portrait type and renderer compatibility
  model.json       # packaged linear_v1 model parameters
  rules.json       # per-control safe output bounds
  metadata.json    # development-only provenance; not trained from user photos
  checksums.json   # canonical JSON SHA-256 identities
```

Files are parsed canonically, capped at 1 MiB each, and checked against `checksums.json` plus a package identity. The package directory is copied into the native resource bundle under `styles/`; the runtime does not download models or use cloud services. A future Phase 8 trainer can emit the same artifact without changing this runtime.

## Features and inference

The feature builder consumes only typed PhotoAnalysis and AssetBatchContext data: tonal percentiles/clipping/dynamic range, color balance/saturation, edge/blur/noise signals, subject/background luminance, backlighting/mixed-lighting, relative group exposure/WB, group confidence and photo type. Missing observations carry an explicit availability bit and use documented neutral zero values; raw UI state is never an input.

The v1 resolver is a deterministic packaged linear model (`photo-editor-linear-style-v1`). It computes an output for every selected asset, applies missing-feature weights, rejects non-finite values, clamps through the package's control-specific bounds and returns confidence plus diagnostics. Batch relationships influence each frame without copying a reference recipe or forcing identical edits: darker relative frames receive stronger exposure recovery, while warm/cool relationships alter the temperature response.

Supported predicted controls are exposure EV, temperature delta from the 6500 K neutral, tint, contrast, highlights, shadows, whites, blacks, saturation, vibrance, texture, clarity, dehaze, sharpening amount, noise reduction and creative vignette amount.

## Recipe conversion and lifecycle

Each prediction is converted to a normal independent EditRecipe. Objective optics and geometry are preserved; creative globals and local layers are replaced, so a second run is idempotent and another trained style replaces the previous trained-style creative state rather than stacking it. Provenance records `origin=trained_style`, style/version, model/package identity, feature schema, BatchContext identity/version and PhotoAnalysis engine/schema.

Only the persisted current editing selection is processed. One asset failure leaves its prior recipe untouched, marks that asset Needs Review and does not stop peers. Progress reports inference and recipe stages, and cancellation stops remaining work while preserving completed recipes. SQLite stores progress, per-asset predictions, feature summaries, recipe hashes and stale status. Reopening reuses valid persisted results; source, analysis, batch-context, style/package or resolver identity changes mark them stale and naturally re-key rendered previews through the effective recipe hash.

## Editing screen

AI Styles are shown separately from POP, WARM and BLACK & WHITE. The normal view stays photographer-facing: choose `Adaptive Natural — Development`, apply it to the current selection, watch simple progress, cancel if needed, and compare original versus recipe-rendered previews using the existing mechanisms. A collapsed development inspector exposes style/version, feature summary, prediction values, confidence and diagnostics without showing tensors or feature vectors in the normal cards. Export All uses the same selected recipes and deterministic full-resolution path as built-in presets.

## Verification and boundaries

Contract tests cover package integrity/version failures, feature-schema rejection, model dimensions/NaN, bounds and prediction validation. Core tests cover dark/bright and warm/cool adaptation, BatchContext directionality, a three-frame scene, idempotent recipe conversion, provenance and objective-field preservation. Frontend tests cover the AI chooser, exact selection scoping, preview updates, prediction details and cancellation while retaining built-in preset regressions.

The bundled development package remains a regression fixture, not a claim to reproduce a photographer's taste. Phase 8 now exports local user-trained packages through this exact loader/resolver contract. Trained packages add optional persisted feature mean/scale vectors and path-free training provenance; legacy packages with no normalization vectors retain identity behavior and their canonical checksums. Completed versions are added to the live catalog without replacing previous versions. See [TRAINING_STUDIO.md](TRAINING_STUDIO.md). Real-photo style quality, broad camera coverage, GPU acceleration, signing/licensing and macOS runtime acceptance remain open.
