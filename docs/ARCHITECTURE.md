# Phase 3 architecture

## Preserved boundaries

React → Tauri background commands → photo-core → photo-contracts. The renderer has no React, SQLite, cloud or authentication dependency. Phase 1.1 discovery, immutable originals, ExifTool metadata/source previews, job membership, output-subtree pruning and checkpoints are preserved. Phase 2 LibRaw, Little CMS, floating-point processing and collision-safe exports remain in place.

The per-asset EditRecipe v1 is authoritative Rust: a required identity/version envelope, typed global basic/curve/color-mixer/presence/detail/optics/effects/geometry groups, ordered logical-mask layers, metadata and provenance. Each job/asset owns independent editing intent. RenderAdjustments is a validated compatibility projection, not another persisted authority. See [the complete recipe contract](EDIT_RECIPE.md).

Source-derived OpticsMetadata stays separate. CPU render_recipe validates an EditRecipe, resolves source/mask/profile dependencies and translates into the unchanged low-level renderer. The React state holds a recipe, not a separate render parameter vector. Legacy adjustment schemas 1/2 migrate losslessly into recipe schema 1; SQLite schema 5 adds transactional current recipes, revision history and corrupt-data recovery.

Analysis = what the image is. Style = what the photographer wants. Recipe = what to do to this individual image. Renderer = executes the recipe. Future analysis/style generation remains separate and unimplemented.

## Source normalization and formats

LibRaw 0.22.2 remains an isolated native helper: sensor calibration, applicable AHD demosaic, camera WB/matrices, sRGB primaries, linear gamma, 16-bit output, no automatic brightness and one orientation normalization. Optional RawSpeed/DNG SDK/JPEG/JPEG-XL/Jasper/zlib integrations remain disabled. RAW support is camera/compression dependent, not guaranteed by extension.

Raster JPEG/PNG/TIFF use bounded decoding, EXIF orientation and Little CMS for RGB ICC → linear sRGB D65. Missing ICC assumes sRGB with a warning; non-RGB ICC is unsupported. PNG alpha flattens over white. HEIC/HEIF remain ingestion/embedded-preview only. Working pixels are f32 RGB; sensor/LibRaw clipping is not recoverable.

## Actual processing order

1. Decode/calibrate immutable source, normalize orientation and source color.
2. Preview only: reduce normalized pixels to a 1600-pixel long-edge proxy. Cache this unedited proxy.
3. Optical peripheral-illumination correction on unwarped pixels; combined distortion and lateral-CA coordinate lookup.
4. Global relative Bradford WB, exposure, tonal zones, contrast, saturation/vibrance.
5. Legacy Phase 2 NR/sharpening, only when old scalar values are nonzero.
6. Master RGB curve followed by per-channel curves; smooth color mixer.
7. Global texture, clarity and dehaze.
8. Enabled local layers in serialized order: basic WB/tone/color, local presence and local detail, blended into the current float image.
9. Expanded global noise reduction and sharpening.
10. Bilinear rotation into an expanded canvas, then normalized crop.
11. Creative post-crop vignette, centered on the final canvas.
12. Preview size bound; sRGB transfer, final range/gamut clamp and quantization.

Preview and export execute these same stages. Reduced demosaic and spatial sampling are not pixel-identical to full resolution. Export never processes the preview JPEG.

Objective stages (sensor/color normalization, orientation and optics) are separate from learnable creative choices. Segmentation generates alpha only from the normalized **unedited, uncorrected** source proxy; it does not make exposure/style decisions.

## Global tools

- Basic controls retain Phase 2 math and ranges: EV ±5; relative temperature 2000–12000 K (6500 neutral); tint/tone/saturation/vibrance ±100. The temperature is not an as-shot Kelvin measurement.
- Curves: master RGB plus red/green/blue; 2–16 ordered x/y points in [0,1], x endpoints 0 and 1, strictly increasing x, nondecreasing y. Deterministic piecewise-linear interpolation in extended sRGB transfer space; end segments extrapolate to preserve headroom. Identity bypasses the transfer round trip.
- HSL: red/orange/yellow/green/aqua/blue/purple/magenta centers at 0/30/60/120/180/240/275/315 degrees. Normalized raised-cosine windows overlap across 65-degree half-widths and wrap around red. Hue/saturation are computed in perceptual HSV coordinates; the photographic luminance control is an EV-like linear-light gain within weighted hue bands. ±100 means ±30° hue, saturation factor 0–2, or ±1 EV. Achromatic colors are left alone; negative gamut residuals are retained.
- Texture: difference between narrow and medium luminance box means; bounded band-pass contribution. Clarity: larger-radius luminance contrast weighted toward midtones. Dehaze: low-frequency dark-channel veil removal/addition with bounded strength. These are distinct CPU approximations, not proprietary Lightroom algorithms.
- Expanded detail: sharpening amount/radius/detail/masking, using luminance unsharp scales, fractional scale mixing, fine-detail weighting and an edge gate. NR separates luminance and color with a small spatial/range-weighted neighborhood; separate detail controls tighten the range weights. Defaults do no processing.
- Creative vignette: amount ±100 (up to ±2 EV), midpoint, feather and roundness. It is applied after crop, not in the optical stage.

Spatial box means use separable running sums, O(pixels); texture/clarity/dehaze/sharpen scales reference a 4000-pixel long edge with minimum practical radii. The small NR neighborhood and legacy detail operate at current pixel resolution. Processing is f32 throughout; only mask disk storage is quantized to 16-bit alpha.

## Optics

LensProfileResolver reads the pinned, unmodified Lensfun version-2 XML database. This is a **Rust database adapter, not a binding to the Lensfun LGPL library**. It supports rectilinear centered profiles with poly3/poly5/PTLens distortion, linear/poly3 lateral CA, and PA vignetting. Hugin distortion/CA radius is half the short side; PA radius is half the diagonal. The implementation follows documented models and reverse lookup ordering. [Lensfun correction ordering](https://lensfun.github.io/manual/v0.3.1/corrections.html).

Matching normalizes punctuation/case and manufacturer prefixes, requires a unique lens identity plus recognized camera, essentially matching crop factor and compatible aspect. There is no fuzzy candidate application, focal interpolation, sensor-size rescaling or projection conversion. Exact calibrated focal values are required; vignetting additionally requires matching aperture and recorded focus distance. Missing/unsupported pieces are skipped, not extrapolated. Diagnostics distinguish exact_match, approximate_match, no_profile, profile_unavailable and correction_disabled, and list the actual applied components/profile/database revision.

Profile correction defaults OFF. Individual distortion/vignette/CA enables and 0–1 strengths are independent; manual distortion and peripheral illumination are separate fallback terms, active independently of the profile switch. Optical correction retains canvas size; border samples outside source are black, with no automatic crop/scale. Creative rotation/crop is later.

## Generic masks and local layers

LocalAdjustmentLayer has id, mask_type, enabled, strength [0,1], invert, optional confidence/reference and a typed LocalAdjustments. At most eight unique layers. Subject/background are initial providers; custom is reserved and explicitly skipped/diagnosed. There is no brush, radial, face/skin/sky provider yet.

Subject is soft portrait alpha. Background is its complement in the same source coordinate system, a separate logical layer with independent parameters. Local tools include EV, relative temperature/tint, all tonal zones/contrast, saturation/vibrance, texture/clarity/dehaze and expanded detail/NR. Local structures cannot deserialize crop, rotation or optics.

For each layer, compute a candidate from the current float image, then blend base + alpha × strength × (candidate − base). Layers are evaluated in stored order, one candidate buffer at a time. Disabled/zero-strength/neutral/stale layers do not change pixels. No JPEG compositing is used.

The subject matte is in oriented uncorrected source coordinates. For each corrected output pixel, the same green-channel optical lookup used for the image samples the original matte; background inversion and layer inversion follow. Later rotation/crop operates on the already-blended image. Debug overlays use that same optical lookup and geometry, and share the preview viewport's contain-fit letterboxing. Overlay state is UI-only and absent from RenderRequest/exports.

## Local segmentation backend

photo-mask-helper is a separate one-request process using ort 2.0.0-rc.13 and dynamically loaded ONNX Runtime 1.29.0 CPU. MODNet FP32 portrait matting is pinned to the Xenova conversion revision fa2fa546052fba4c08921230a26cc69a333fca12. Model size: 25,888,640 bytes. Input is bilinear-resized RGB, shortest edge 512 with maximum edge 1024, rounded to multiples of 32; linear proxy RGB is converted to sRGB and normalized to [-1,1], NCHW. Output is single-channel alpha [0,1]. No calibrated confidence is supplied.

The SegmentationProvider trait separates provider/model from cache and renderer. This phase uses CPU only, two intra-op threads and one inter-op thread. No NVIDIA/GPU requirement. Packaging paths support Windows x64 and Apple Silicon, but Mac runtime/build is unverified. Helper cancellation kills/reaps the process; timeout is 120 seconds.

## Cache, persistence and bounded work

One executing expensive task plus at most one pending preview/mask replacement; exports are never implicitly cancelled. Tauri dispatches background work and existing request tokens cancel stale jobs. UI defaults to explicit Update Preview; optional Auto Preview debounces changes by 350 ms and stops automatic retries on failure. Slider changes save parameters without starting inference per keystroke. Mask generation is explicit and cache-first.

A SHA-256 mask key includes canonical source identity (path, size, mtime), decoder/orientation version, model revision and preprocessing contract. Creative adjustments, optical coefficients, crop and rotation do not enter the key: the source matte is reused and transformed. Source identity is not a cryptographic file-content fingerprint; same-size/same-mtime changes can evade it.

Alpha is a disposable compressed grayscale16 PNG, bounded to 1024², plus small JSON diagnostic sidecar. SQLite migration 004 adds toolkit_json to development_state and a mask_state metadata table. No image/float arrays enter SQLite. Missing/corrupt caches are stale and regenerate through Generate Masks. Model failures/unavailability are nonfatal to global render/export; local layers are skipped with visible diagnostics.

Normalized source proxy reuse remains. Edited preview keys now combine source identity, canonical recipe content hash, renderer/backend version, validated mask sample/model/geometry dependencies, actual loaded lens XML digest and objective metadata. Optics/local cache hits are supported; unavailable or replaced derived data rekeys. Diagnostic sidecars are disposable and overlays remain excluded. See EDIT_RECIPE.md for conservative hash semantics and dependency checks.

Migration 005 is additive and lazily converts one current recipe per opened asset. Saves, hashes, optional history and legacy checkpoint projections commit together with optimistic generation checks. Meaningful actions create deduplicated snapshots; auto-preview/slider events do not. Keep the original plus latest 199 snapshots. Corrupt payloads are retained and explicit reset/import/restore recovers a neutral/error display. History metadata is paged lazily; the grid never loads thousands of full recipe histories.

The startup render budget remains min(4 GiB, available RAM / 2), with a conservative 64 bytes/source-or-output-pixel preflight (~67 MP at maximum), and LibRaw's 2 GiB decoder limit. Local candidates are processed sequentially; scalar blur scratch avoids many RGB buffers. Segmentation uses only a reduced proxy and is serialized against full export. These are application limits, not an OS/native-memory sandbox. No disk quota/eviction UI yet.

## Export and checkpoints

JPEG is 8-bit sRGB (default UI quality 95); TIFF is uncompressed strip-based RGB16 sRGB. Both include ICC. Photographic metadata is copied from an allowlist; GPS, descriptions, serials, maker notes and XMP are omitted. Export writes new suffixed names in the job output directory and never overwrites originals or existing exports. Checkpoints are published only after output publication. A crash between publication and SQLite commit can leave an untracked output; retry gets a suffix.

No trained styles, automatic edit decisions, batch AI recipes, scene clustering, generative processing, PhotographerApp API, authentication, cloud, licensing/billing or production signing were implemented.
