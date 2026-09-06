# Phase 7 limitations

## Adaptive style scope

The bundled `Adaptive Natural — Development` package is a deterministic linear regression fixture used to prove the runtime contract. It is not trained from the photographer's images and must not be interpreted as a learned personal style. Its outputs are creative-control suggestions bounded by the renderer; objective orientation, optics, source identity and geometry are preserved.

The v1 feature vector is limited to reliable Phase 4 measurements and Phase 6 relative context. Missing measurements are explicit, but neutral defaults reduce confidence and can reduce adaptation. Batch relationships are conservative proxy deltas, not recovered exposure or calibrated Kelvin measurements. The resolver has no ONNX/GPU backend, HSL/curve/local-mask prediction, reference-image embedding or style blending.

Selection scoping, persistence and stale detection are implemented, but real-photo visual quality still requires manual review. One failed asset is marked Needs Review and left unchanged; confidence is evidence metadata, not a calibrated probability. Preview/export math remains the existing deterministic CPU renderer, with its documented reduced-preview versus full-resolution differences.

Phase 8 training is intentionally absent: no RAW/reference pairing, optimization loop, photographer feedback learning, Training Studio, cloud service, licensing or production signing is included. A package can be replaced only through the validated local artifact format documented in [TRAINED_STYLES.md](TRAINED_STYLES.md).

# Phase 6 limitations

## Batch context is relationship evidence, not an edit decision

See [BATCH_CONTEXT.md](BATCH_CONTEXT.md) for the exact schema and methods. Scene and lighting groups are conservative deterministic heuristics over current Phase 4 measurements and Phase 5 descriptors, not semantic room, location, event, person-identity, pose, panorama or lighting classifiers. Repeated structures, wrong/missing capture time, camera metadata, processed RAW/JPEG differences and changing light can split or confuse groups. Filename sequence is never used. The two-second metadata fallback requires compatible camera/lens/orientation/aspect but still cannot prove two files depict the same scene.

Scene grouping compares at most 64 anchors and lighting searches at most 27 indexed neighbors. This prevents uncontrolled quadratic work but can miss a relevant nonlocal neighbor or split a long series. Anchor comparison limits transitive collapse but may also split a gradually moving viewpoint. Current selection changes use bounded full regrouping rather than incremental graph splicing; exact prior identities remain cached, with no cache quota/pruning UI yet.

Exposure relationships use proxy median luminance in log2 space. They do not estimate recoverable RAW exposure, target EV, or an edit. WB relationships are differences on Phase 4 warm/cool and green/magenta signals, not Kelvin/tint measurements. Mixed light, dominant subject color, windows, silhouettes and intentional light progression can affect both. No histogram matching or normalization is performed.

Reference candidates are technically stable anchors, not artistic winners. Thresholds over existing proxy/culling evidence can reject a useful image or accept a visually inappropriate anchor. Up to three near-equivalent candidates are retained; weak groups intentionally have none. Confidence is evidence strength, not a calibrated probability.

Missing current PhotoAnalysis yields an unavailable asset context and does not fail the batch. Available PhotoAnalysis without current Phase 5 features is partial and can still enter measurement-based lighting context, but lacks visual scene evidence. Phase 6 does not automatically run missing Phase 4/5 analysis. A source/analysis/grouping/selection change invalidates the current identity; recipe, preset, mask and export changes do not.

The inspector is a development validation surface, not a final photographer workflow. It does not show pixel overlays or every diagnostic/reference reason, and native interactive/macOS acceptance is not claimed. The local portrait shoot is useful evidence but does not validate real-estate rooms/brackets, landscapes, varied cameras, daylight transitions or 3,000 real RAW files. Phase 7 inference is a development-only local linear model with no learned personal style, calibrated quality guarantee, style blending or cloud AI.

## Preserved Phase 5 limitations

## Culling is an explainable recommendation, not photographic certification

See [AI_CULLING.md](AI_CULLING.md) for exact policy, thresholds, safety bounds and selection semantics. CPU YuNet adds face geometry/local detail, **not eye-open/closed detection**. Its default eye provider is unavailable; no eye labels are inferred from face landmarks. Structured blink tests validate policy only. Reliably focused Near/Burst leaders can reach 5★ with an explicit unknown-eye warning; that rating does not certify expression or open eyes. Manual 1–5★ overrides remain available and are never erased by reruns.

Proxy face measurements are texture-, scale-, pose-, lighting- and noise-dependent; no full-resolution focus certification or motion/defocus/intent classifier. Tiny/profile/occluded faces may be missed. Box-edge warnings are not anatomical head/body cutoff detection. Subject masks remain fallible. Intentional crops, silhouettes, near-blank frames and long exposures need photographer review. Confidence is not calibrated probability. Real-estate flash misfires/reflections/occlusion and semantic landscape horizons are not classified.

Exact duplication is job-wide complete-file SHA-256 plus byte length, including metadata. Equal decoded pixels with different file metadata/encoding are not exact. Canonical is the smallest stable catalog asset ID; membership changes can change it. Redundant exact copies alone receive AI 1★ for redundancy, even when bytes cannot decode; an undecodable canonical remains not rated. Manual stars and inclusion still win. No copy is deleted or moved.

Visual similarity is a bounded 64-anchor pass, max 32 distinct-content representatives/group (exact siblings expand the displayed size). It can miss nonadjacent near matches, split long series and confuse repeated structures. Near/burst groups use modest relative ranking; generic Similar Composition has no rating adjustment. Duplicates Hide suppresses redundant exact copies and all but one deterministic Near/Burst display representative from the grid, but never Similar Composition alone. The scorer may retain several technical ties in the inspector even though the simplified view shows one BEST. This is display-only: Show restores all alternatives, and neither setting changes ratings, relationships or selection. Burst needs compatible visual evidence and known camera/time proximity, never sequential filenames. Metadata may be wrong or absent; expression/semantic moment differences are not reliably recognized. Unique means no relationship found within this search, not globally unique. No semantic/event/person-identity matching or automatic duplicate deletion. Suggested brackets are not HDR eligibility judgments.

The 0.20 severe-softness, 0.18 strong-focus and 0.45 exceptional-focus boundaries are calibrated against the existing contrast-normalized face-detail signal, not a learned blur probability. The severe gate additionally requires a relevant face covering at least 0.008 of the frame plus reliable detection/detail evidence. Low-texture faces, motion patterns and unusual lighting can still cross these boundaries incorrectly. The UI exposes the values, group reference and confidence so photographers can override the result. The accessible recursive folder has 53 assets representing 52 distinct photographs and is one acceptance set rather than broad model validation; filenames are not part of scoring.

Jobs are limited to 5,000 photos and a separate 64 MiB serialized evidence budget, not a RAM cap. Summary loading validates job-wide SQLite/source generations; helper startup adds per-image overhead. First cull reads every uncached complete file; Run/resume reuses hashes when OS generation matches, while explicit Re-cull all rehashes. Hash cost is reported separately. Synthetic grouping at 500/1,000/3,000 and a 32 MiB hash do not certify real 3,000-RAW latency, peak RAM or slow/network volumes.

The culling cache uses Windows file/volume ID plus creation/write/change times and size, or Unix inode/device/mtime/ctime/size. Unsupported/unreliable file-generation access makes identity unavailable, not a fallback exact match. Ordinary size/mtime-preserving edits were tested on Windows; malicious timestamp manipulation, unusual filesystems, network cache consistency and Mac runtime remain unverified. Hashes are checked against source generations at extraction/publication, but no filesystem watcher runs between user actions. Culling binds changed full content to refreshed analysis; other ingestion/render fingerprints are not globally upgraded. OS reads/codecs can block cancellation internally.

No automatic resume, history quota or hard-crash scratch cleanup service. Rerun/rescan preserves manual rating and the current selection. Every rating/duplicate/available-issue filter change now intentionally replaces the saved selection with all current matches across all pages. Manual checkbox refinements and Clear Selection remain authoritative only until the next filter change; filters themselves are not persisted across closing the culling screen. Optimistic selection writes are serialized, Run for Editing waits for the latest write, and a persistence failure hides the local overview until the authoritative snapshot reloads. Run for Editing opens the deterministic built-in preset screen and reloads that snapshot; it does not re-cull or fall back. Edited contact-sheet proxies, POP masks and Export All files are prepared serially through the existing cancellable worker. A failed asset is marked for attention and does not stop the remaining batch; cancelled partial previews can be regenerated from their recipe-keyed disk cache, while completed exports remain in the output folder. Export All currently uses the existing DevelopmentPanel JPEG/quality-95 default because there is no persisted job-level output-format setting; per-photo DevelopmentPanel export retains JPEG/TIFF choice.

The BLURRY issue requires the calibrated severe subject-softness reason; mild softness and ambiguous low texture remain inspector review evidence. A confident global-blur issue for real-estate/landscape photographs is not yet emitted because the current proxy cannot reliably distinguish technical blur from intentional low-detail scenes. CLOSED EYES exists as an explicit issue contract with per-person detailed reasons, but the shipping eye-state provider is unavailable, so its UI filter is disabled. No eye state is fabricated.

One accessible 53-asset/52-distinct-photo portrait folder was inspected and reprocessed through the production service. It calibrated a specific severe-softness failure and verified ratings plus duplicate display behavior, but it did not supply labeled blink/expression truth or broad portrait accuracy evidence. Phase 7 additionally rendered four adaptive-style previews from this corpus, but that does not establish style quality. No Sony RAW, real-estate/landscape quality, interactive native acceptance, macOS runtime or commercial release readiness is claimed. Built-in presets remain deterministic examples; feedback learning, scene-consistency authoring, PhotographerApp integration and cloud dependency remain out of scope.

## Preserved Phase 4 source-analysis limitations

The statements below describe Phase 4 PhotoAnalysis, not the separate Phase 5 culling detector.

## Source analysis is descriptive, not photographic certification

PhotoAnalysis v1 describes the oriented unedited ≤1600-pixel normalized proxy. All thresholds, units and confidence heuristics are in [PHOTO_ANALYSIS.md](PHOTO_ANALYSIS.md). No editing decisions are produced. Low/high brightness classes, mixed-color variation and backlighting tendency can reflect intentional scene content; they are not defect judgments. Developed-proxy clipping does not prove RAW recoverability. Proxy reduction attenuates noise and fine detail; ISO is metadata, not a noise detector.

Hough-style horizontal/vertical evidence supplies candidate level references only, not semantic skyline recognition. There is no keystone solver. Blur/motion classification is unavailable rather than falsely scoring shallow-DOF portraits. Sky/foreground, indoor/outdoor/interior/exterior, anatomical headroom, instance/face counts and face geometry are unavailable. No new ML model was added; existing MODNet alpha has no calibrated confidence. Confidence elsewhere is evidence strength, not probability.

Missing/failed subject analysis is partial, with valid common measurements. Analysis masks are generated in a separate cache namespace, so unresolved edit layers cannot be activated by running analysis. Same-version model installation/repaired masks require explicit analysis invalidation to replace a cached unavailable result. Cache identity uses source path/size/mtime and metadata, not a whole-source content hash. Raster decode is budgeted but initially full resolution. Analysis concurrency is intentionally conservative: one execution and one waiting request, suitable for a bounded future batch caller but not a batch UI.

The debug geometry view is a source-coordinate schematic; pixel overlays on the edited viewport and clipping heatmaps are deferred. No real Canon CR3/landscape/real-estate manual acceptance or Apple Silicon run is claimed without actual execution; current evidence is recorded in [VERIFICATION.md](VERIFICATION.md). Face detection, full-resolution detail analysis, calibrated scene semantics, automatic recipes/styles and clustering are deferred beyond Phase 4.

## Preserved Phase 3 limitations

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

Outside the separate culling identity pass, ingestion/rendering do not hash every full source; same-size/same-mtime changes can still evade those inherited fingerprints. No filesystem watcher, removed-file reconciliation, cache quota, encryption or orphan-temp cleanup service. A source can be developable even if its bounded ingestion thumbnail is unavailable.

Exports require temporary disk headroom. Crash after file publication but before SQLite commit may leave an untracked valid export. Existing outputs are never silently overwritten. Metadata copying is an allowlist, not a general-purpose privacy sanitizer.

## Recipe-specific limits

The first complete contract is recipe schema 1; only its documented v0 legacy interchange bridge is upgraded. Arbitrary operation lists, future schemas and Lightroom/XMP files are rejected. Recipes are capped at 256 KiB and eight local layers. Export includes IDs/timestamps/provenance, so inspect these before sharing, though no source/cache paths or pixels are embedded.

History retains the original plus latest 199 meaningful snapshots; older intermediate snapshots are pruned. This is not a permanent correction-training event log. Recovery archives have no automatic purge. No immediate undo/redo, full template UI, copy-to-all workflow or batch recipe scheduler. The Inspector displays latest 100 history entries; core history is paginated. Draft persistence is asynchronous and shutdown during an in-flight save can lose that final unsaved change.

Missing/stale mask references preserve intent but skip affected local output; warnings must be reviewed. Import rebinds selectors only to the target's cache or leaves them unresolved. No mask file is transferred. Resolved profile identities remain source-derived diagnostics, not creative settings. Hashing conservatively includes some neutral/inactive subparameters rather than proving mathematical equivalence of all pipelines.

Same-source fingerprints still use canonical path/size/mtime rather than reading every RAW byte. Mask dependencies hash actual validated samples; optics dependencies hash loaded database XML. Embedded Little CMS ICC profiles include creation time, so equal rendered pixels do not imply identical whole-file hashes. No cross-platform bit-for-bit certification or real Canon/Sony recipe round-trip acceptance has been performed.

## Deferred

Still deferred after Phase 7: Phase 8 trained-style authoring/training, automatic semantic scene classifiers, Lightroom/XMP imports, face beauty/generative editing, PhotographerApp APIs, auth, cloud, licensing/billing, GPU providers and production installers/signing. Source analysis, culling, batch context, adaptive creative recipes and deterministic rendering are implemented as separate boundaries.

Ship the executable with raw/, exiftool/ and toolkit/ resources and their licenses. The MODNet model is Apache-2.0; ONNX Runtime is MIT; the unmodified Lensfun database is CC BY-SA 3.0. Distribution/license-obligation review remains a release task.
