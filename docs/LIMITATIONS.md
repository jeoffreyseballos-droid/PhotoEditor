# Phase 2.1 limitations

## Verification is not photographic certification

Windows x64 build and automated results are recorded in VERIFICATION.md. No actual Canon/Sony RAW folder was supplied. No portrait hair/body-edge acceptance, real-lens visual validation, external-viewer comparison or native interactive desktop acceptance is claimed. The real MODNet test uses generated non-portrait pixels; it proves loading/inference/dimensions/ranges/repeatability, not portrait quality. The actual Lensfun database is exercised with Canon EOS 650D / EF-S 10–22mm metadata and synthetic pixels; this is **not** a verified camera/lens photograph.

Apple Silicon paths and CI exist but no Mac build/runtime was run. No installer, signing, notarization or release distribution validation.

## Portrait masking

MODNet is human portrait alpha matting, not general object/scene segmentation. Animals, landscapes, tiny/distant people, multiple overlapping people, unusual poses, motion blur, backlight, transparent fabrics and fine hair may be wrong. Output is soft alpha, not a calibrated confidence estimate; confidence remains null. Nearly empty/full masks trigger a warning but do not establish failure or success. Inspect overlays before local edits.

Input is an oriented unedited proxy, shortest edge 512/max edge 1024, dimensions rounded to 32. It cannot recover full-resolution hair detail and may slightly alter aspect through rounding. Background is simply subject's complement. No per-person/face/skin/sky masks, manual brush, feather/expand/contract controls, corrections to model output or AI denoise. A custom kind is reserved but unsupported.

Generate Masks is explicit. Failed/missing/stale masks skip local layers and allow global export, which means an export may intentionally lack requested local changes; inspect visible diagnostics. Cache deletion requires regeneration. Model preprocessing clamps source proxy values to display sRGB for inference only; this does not downgrade the editing pipeline.

## Optics

The mature Lensfun **database** is bundled, but a limited independent Rust adapter evaluates it. It does not provide full Lensfun-library matching/interpolation/rescaling/projection behavior. Profiles default off.

Only unique exact lens names, recognized cameras, essentially equal calibration crop factor and compatible aspect are eligible. No fuzzy aliases beyond punctuation/case/maker-prefix normalization. Unrecognized metadata, duplicate lens identities, missing focal length, mismatched crop, shifted optical centers, fisheye/non-rectilinear profiles and ACM models are skipped. Partial correction is explicit in applied-component diagnostics.

No focal/aperture/distance interpolation or crop-factor coefficient rescaling. Distortion and CA require an exact stored focal calibration. PA vignetting also requires recorded focus distance and exact aperture; many real files will therefore skip profile vignetting. Manual distortion and peripheral illumination remain available and separate. No automatic inscribed crop or border fill: optics preserves canvas bounds and can expose black edges. CA only addresses lateral channel displacement, not longitudinal fringing. PA gain is safety-limited; no real optical accuracy claim.

Already lens-corrected JPEGs/RAWs are not automatically detected. Enabling profile correction on them can double-correct. Metadata may be inaccurate; model-name matching is not proof of lens identity. User choice and real-file comparison remain necessary.

## Creative quality and color

These are deterministic photographic tools, not a recreation of proprietary Lightroom math or a final commercial quality certification.

- f32 linear sRGB/D65 intermediate; LibRaw enters through a linear16 bridge. Camera/sensor clipping cannot be recovered. No wide-gamut/HDR mastering, DCP profile browser, soft proofing or sophisticated gamut mapping.
- Curves are monotone piecewise linear, not spline/parametric curves; master is RGB, not a luminance-preserving curve. HSL hue/saturation use perceptual HSV and hue-weighted luminance gain. Extreme edits can leave output gamut.
- Texture is a bounded luminance band-pass; clarity is local midtone contrast; dehaze is a simplified low-frequency dark-channel veil estimate. Halos, darkening and color artifacts are possible on extreme settings. Local presence/detail candidates are processed before mask blending, so neighborhood information can cross a mask boundary.
- Sharpening uses scale-mixed unsharp luminance detail; masking is an edge gate, not a separately editable mask. NR is a deterministic small bilateral-like luma/chroma filter, not AI denoise; no separate luminance-contrast control.
- Legacy Phase 2 sharpening/NR retain their original stage/math and can coexist with new detail controls. UI exposes and warns about nonzero legacy values; Reset Detail clears both.
- Preview uses reduced RAW demosaic and a 1600-pixel proxy. It shares algorithms/parameters but not exact pixels with full export. Minimum radii, NR neighborhoods, demosaic and rounding differ. Inspect full-resolution exports for detail and mask edges.
- Rotation/crop uses bilinear sampling and may leave black corners. Creative vignette is centered after crop. There is no draggable crop or sophisticated resampler.
- RGB ICC uses Little CMS; unprofiled sources assume sRGB. Non-RGB ICC is rejected; PNG gamma-only metadata is not used as an ICC profile. Camera/source thumbnails may differ from neutral RAW development.
- JPEG is 8-bit sRGB; TIFF is RGB16 sRGB, not layered/linear/HDR/RAW output. PNG transparency flattens over white.

## Decoder and resource boundaries

LibRaw 0.22.2 covers registered RAW families but actual camera/firmware/compression support varies. Optional DNG SDK, RawSpeed, zlib, JPEG/JPEG-XL and Jasper integrations remain disabled. Nikon HE/HE*, compressed/lossy/float DNG and new camera variants require special testing. HEIC/HEIF remains discovery/metadata/embedded-preview only.

One expensive worker plus one replacement; no batch renderer or dynamic GPU scheduler. Full raster previews decode before reducing. Memory preflight is conservative, not an OS-enforced cap. Raster codec/encoder calls can be noninterruptible internally; RAW timeout 300 s, segmentation 120 s, metadata 30 s. Native crashes/hard OOM/hostile files are not completely contained. CPU only.

No content hashing of every source, filesystem watcher, removed-file reconciliation, cache quota, encryption or orphan-temp cleanup service. Same-size/same-mtime changes can evade fingerprints. A source can be developable even if its bounded ingestion thumbnail is unavailable.

Exports require temporary disk headroom. Crash after file publication but before SQLite commit may leave an untracked valid export. Existing outputs are never silently overwritten. Metadata copying is an allowlist, not a general-purpose privacy sanitizer.

## Deferred

Phase 3: full recipe/operation history and recipe migrations. Later phases: trained styles/training, automatic exposure or image analysis, batch AI recipes/scene consistency decisions, clustering, Lightroom/XMP imports, face beauty/generative editing, PhotographerApp APIs, auth, cloud, licensing/billing, GPU providers and production installers/signing. Work stops at Phase 2.1.

Ship the executable with raw/, exiftool/ and toolkit/ resources and their licenses. The MODNet model is Apache-2.0; ONNX Runtime is MIT; the unmodified Lensfun database is CC BY-SA 3.0. Distribution/license-obligation review remains a release task.
