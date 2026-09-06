# Photo Analysis Engine — Phase 4

Analysis = what the image is. Style = what the photographer wants. Recipe = what to do. Renderer = executes.

This engine describes the normalized source. It never creates editing recommendations, touches a recipe, changes sliders, renders an edited preview, or modifies an original. No OpenAI, network, identity, training, clustering, or new model dependency is introduced.

## Contract and boundaries

Rust `photo-contracts::analysis::PhotoAnalysis` is authoritative. `PHOTO_ANALYSIS_SCHEMA_VERSION = 1` is independent of recipe schema 1, application version, and SQLite migration 6. The earlier unused untyped ImageAnalysis placeholder now aliases this contract; Phase 7 consumes it through the separate trained-style feature builder without adding editing decisions to the analysis service.

The envelope contains analysis_id, asset_id, source_fingerprint, created_at, photo_type, common measurements, subjects, lighting, type_specific, confidence, and diagnostics. PhotoType is the stable enum portrait / real_estate / landscape. CommonAnalysis groups source, exposure, color, dynamic_range, detail, composition, scene, and decoder warnings. TypeAnalysis is a discriminated enum that must agree with the envelope. No mask pixels or high-resolution debug arrays are serialized.

Unknown fields, wrong types, non-finite numbers (including optional confidence), out-of-range fractions/confidence/boxes, unordered percentiles, invalid timestamps/identities/dimensions, photo-type disagreement, and oversized payloads are rejected. Limit: 128 KiB; strings at most 4096 bytes and lists at most 256 items. Canonical JSON sorts object keys. serde_json float_roundtrip is enabled so stored f64 measurements reload without one-ULP drift. Parse checks schema version before decoding; unsupported versions produce a structured AnalysisError. Future migration dispatch belongs there; no prior analysis format is invented.

`photo-core::analysis::measure` operates on normalized pixels, without React, Tauri, SQL, recipes, or styles. AnalysisService prepares input, runs/reuses analyzers, handles cancellation, and persists results. React makes one analyze request, not individual measurement calls.

## Source input and metadata

The read-only `CpuProcessingEngine::analysis_input` shares the existing unedited development proxy cache and decoder allocation lock. It never reads an embedded thumbnail or edited preview. Long edge is at most **1600 px**, preserving aspect ratio with integer rounding. Minimum usable input is 16×16. Orientation is applied by the existing decoder once; all geometry uses this oriented, uncorrected source frame, before optics, crop, rotation, or any creative processing.

RAW uses the existing half-size LibRaw development path, camera WB and normalization; it does not request a full-size demosaic. JPEG/PNG/TIFF use the existing memory-budgeted raster decode, orientation and ICC conversion, then linear-light area reduction. Raster decode can still allocate full source pixels first; it is bounded by the same conservative render limit. HEIC/HEIF development remains unsupported. Tiny/corrupt/oversized inputs fail safely.

Working pixels are f32 linear sRGB/D65. Analysis luminance is clamped to [0,1]; color descriptors use display-sRGB. These are developed proxy measurements, **not sensor-domain radiometry**. Orientation metadata, camera/lens, focal length, aperture, shutter, ISO and camera-local capture timestamp are copied from existing ingestion metadata. Missing EXIF stays null and does not fail analysis. Source width/height describe the measured proxy; metadata_width/height retain original metadata dimensions where available. No second EXIF subprocess is started for analysis.

## Measurement definitions

### Exposure and clipping

Luminance Y = clamp(0.2126 R + 0.7152 G + 0.0722 B, 0, 1), in linear sRGB. Mean is arithmetic; median and p01/p05/p25/p50/p75/p95/p99 use a 4096-bin histogram, nearest-bin input quantization and rank floor(q×(N−1)). Percentile resolution is approximately 1/4095.

- Shadows: Y < 0.10; midtones: 0.10 ≤ Y < 0.70; highlights: Y ≥ 0.70. Fractions sum to one.
- Black/shadow clipping: Y ≤ 0.001; near shadow: Y ≤ 0.01.
- Highlight clipping: Y ≥ 0.99; near highlight: Y ≥ 0.95.
- Any-channel highlight clipping: at least one original normalized RGB channel ≥ 0.99, reported separately from luminance clipping.
- Near-clipping fractions include clipped pixels; they are not disjoint categories.

Median classes are strongly_underexposed below 0.025, underexposed below 0.10, balanced below 0.55, overexposed below 0.80, and strongly_overexposed otherwise. These names express a low-confidence brightness heuristic (0.45), not correctness or a recommended correction. High-key, low-key, night scenes and intentional silhouettes can be valid photographs. All underlying numbers remain available.

Clipping may have occurred in the sensor, RAW development, color conversion or raster encoding. Proxy reduction can hide small clipped regions. No recovery/recoverability assertion is made.

### Tonal range

Percentile range = p95−p05; interquartile range = p75−p25. Proxy EV span = log2((p95+0.001)/(p05+0.001)), not true scene or sensor dynamic range. High-contrast tendency = clamp((range−0.4)/0.5); low-contrast tendency = clamp(1−range/0.25); evidence 0.5. The IQR and occupancy describe midtone distribution without a speculative aesthetic score.

### Color

Mean RGB is in display-sRGB [0,1]. Warm/cool balance = mean R−mean B, positive warm, range [−1,1]. Green/magenta = mean G−(mean R+mean B)/2, positive green. These measure image content; there is no illuminant Kelvin estimate or assumption that a warm sunset has incorrect WB.

Average chroma = mean(max RGB−min RGB). Saturation is HSV S=(max−min)/max (zero for black). Low saturation S<0.15, high saturation S≥0.65. Every pixel belongs to neutral (S<0.15) or the nearest circular hue center: red 0°, orange 30°, yellow 60°, green 120°, aqua 180°, blue 240°, purple 275°, magenta 315°. Fractions sum to one.

Spatial cast variation is RMS deviation of warm/cool and green/magenta balances over a 3×3 grid. Mixed-lighting tendency = clamp(variation/0.25), evidence 0.25: differently colored objects can give exactly the same signal.

### Detail, noise, blur

Edge strength is the mean magnitude of central first differences of linear luminance. Laplacian RMS uses 4Y−left−right−above−below. A 3×3 grid reports the spatial distribution of edge strength. These are proxy-scale signals, not full-resolution acuity or image-defect scores. Background blur in a shallow-DOF portrait is not labeled a failure. Global blur and motion-blur likelihood remain unavailable because low texture and optical/motion blur cannot be reliably distinguished by these simple statistics.

Noise uses low-gradient pixels (central gradient <0.04). Residual is center minus the eight-neighbor mean. Samples with absolute luminance residual ≥0.15 are excluded. Luminance sigma is residual RMS / sqrt(1.125); chroma sigma uses the R−G and B−G residual differences, divided by 4.5 before RMS. Severity = clamp(max(sigma_luma,sigma_chroma)/0.05). Require at least 64 samples and 5% coverage. Evidence = min(0.65, 0.65×flat coverage). Texture, compression and demosaic can confound the estimate; reduction attenuates sensor noise. ISO is retained as metadata, never substituted for measured noise. No full-resolution noise certification or denoising is performed.

### Lines, level and composition

Line analysis uses an area-reduced ≤320 px proxy, central difference edges with contrast >0.12 and axis-dominant gradient >2× the cross gradient. A bounded Hough-style search checks ±12° in 0.5° steps using two-pixel intercept bins. Support is votes/(2×line span), clamped to one. Require support ≥0.35, center intercept inside 3–97% of the frame and an estimate strictly inside the search limit. Confidence/evidence = min(0.85, 0.85×support).

Angle sign is clockwise in image coordinates. Horizontal position is y at the frame's x-center; vertical position is x at y-center; both normalized [0,1]. Tests exercise 0°, +2°, −3° horizontal references. These are credible **straight-line candidates**, not semantic horizon recognition. Common semantic horizon stays unavailable; landscape's horizon field explicitly reuses this candidate evidence. A shelf, roof or patterned scene can be mistaken for a level reference. Keystone/converging-line analysis is unavailable. No rotation/perspective correction is applied.

Aspect ratio and frame orientation are direct. Subject bbox uses alpha ≥0.5; centroid and area use soft alpha weights. Coordinates are normalized to [0,1], bbox extents include pixel edges. Center distance is Euclidean distance from frame center ×sqrt(2). Top margin and nearest frame-edge distance describe geometry, **not anatomical headroom**.

### Subject and lighting relationships

Portrait reuses a valid existing Phase 2.1 source mask read-only. If absent, it runs the same isolated SegmentationProvider / pinned MODNet CPU helper through MaskCache, but writes only to **analysis-masks-v1**, not renderer masks-v1. This is essential: generating a renderer mask could activate unresolved local recipe layers and change a later preview/export. Analysis cannot do that. Scratch is likewise isolated and existing helper temporary-directory cleanup applies.

No new ML assets/downloads/licenses are added. Actual model version is recorded in diagnostics, including attempted providers. MODNet returns portrait alpha, not calibrated detection confidence, per-person instances or faces. Subject presence means sufficient usable matte area, not independently proven person detection. Below 1% alpha area, subject_present is false; nearly full (>99%) or inseparable geometry is unavailable. Subject/background RGB, mean luminance, luminance standard deviation and edge strength use alpha and 1−alpha weights. Subject sharpness uses weighted backward-difference energy (not numerically identical to global central differences).

EV difference = log2((subject mean Y+0.001)/(background mean Y+0.001)); negative means darker subject. Backlighting tendency = clamp(−EV difference/3), evidence 0.4. It cannot determine physical light placement. A failed/unavailable segmentation provider leaves common measurements valid and records a warning; cancellation remains a whole-request cancellation. Face detection/count/geometry and subject instance count are explicitly unavailable, not zero.

## Photo types, uncertainty and status

Portrait runs subject relationships and exposes unavailable face fields. RealEstate skips portrait inference, reports brightness/highlight occupancy, p05 shadow depth, mixed-color evidence and horizontal candidate roll; interior/exterior and keystone remain unavailable. Landscape skips portrait inference and exposes candidate horizon, tonal range, color and sharpness distribution; semantic sky/foreground and atmospheric identification remain unavailable. The 3×3 brightest-region center is direct; high/low-key and low-light tendency are weak brightness signals, not day/night/indoor/outdoor truth.

Observation<T> is available {value, confidence}, unavailable {reason}, not_applicable {reason}, or failed {reason}. Missing values never masquerade as zero. Confidence is a documented, uncalibrated **evidence/repeatability heuristic**, not a probability. Direct calculations or geometry inherited from uncalibrated MODNet have null confidence; no aggregate confidence or aesthetic quality score is fabricated.

Request states: not_analyzed, queued, analyzing, complete, warning, failed, cancelled, interrupted. Overall warning indicates actual decoder/provider warnings; a deliberately unimplemented optional analyzer has explicit unavailable diagnostics without necessarily making the source failed. Interrupted reservations are recovered on startup.

## Persistence, reuse and invalidation

Migration `006_photo_analysis.sql` adds photo_analysis and analysis_status, independently of recipes, development_state and unused historical processing_state.analysis_json. The record stores envelope columns, engine/provider versions, canonical payload, timestamps, status, and indexed median/clipping summaries. At most one current record per job/asset/photo type is retained. No analysis history/training dataset is created.

Source fingerprint hashes canonical source path, file size, high-resolution mtime and ingestion metadata. This inherits the project's inexpensive source identity strategy: changing bytes while deliberately preserving path, length and mtime can evade detection. It is not a full-content digest. Recipe contents are absent from every analysis key.

Cache identity includes source fingerprint, independent analysis schema, engine version, input-preparation version, decoder ID, photo type and configured renderer/analysis model versions. Common measurements reuse any matching source/common-engine record across photo types without decoding again (portrait may still need pixels for subject measurements). Failed/absent-model results are saved as partial observations; after installing/restoring a same-version model, explicitly invalidate to retry. Manually replacing same-version mask pixels also requires explicit invalidation. Ordinary cache reads preserve ID, timestamp, diagnostics and measurements.

Source/metadata identity is checked again before transactional publication. Only fully validated PhotoAnalysis is committed. Corrupt/future payloads fail safely; explicit invalidation discards disposable analysis and allows recomputation. Invalidation never deletes masks, photographs or edit history. JSON exports use unique sanitized filenames in the job output directory with no-clobber publication; they contain no mask arrays or original source paths. Treat metadata/analysis from private photos as private.

## Concurrency, cancellation and UI

AnalysisService permits at most two reservations (one executing and one waiting), rejects duplicate active assets, and serializes analysis pixel work. It shares the renderer's source-decode mutex, preventing simultaneous expensive decoding on the same engine. Waits are cancellation-aware; loops check cancellation by rows/search steps and MODNet/LibRaw subprocesses use the existing cancellable helper path. A released unused permit is cancelled; crashes recover as interrupted. Cancellation before transaction commit leaves no partial analysis and cleans helper temporaries; valid disposable mask cache entries may survive a late cancellation. A commit already completed before cancellation remains complete.

Future batch callers can feed many assets through this bounded API; no 300-image in-memory queue, scene clustering or unbounded worker pool is introduced. The proxy, luminance/color scratch, histogram and small line image are bounded. Cold raster decoding remains the main large allocation. Measurements run on a background worker, never React's thread.

The collapsed development inspector loads only when opened. It supports photo type, analyze/cancel/invalidate, persisted/cache status, numeric summary, type-specific JSON, diagnostic/confidence JSON and complete JSON export. An optional source-coordinate diagram shows the subject box and candidate line. It is a schematic, not an overlay on a potentially cropped/rotated edited preview; no debug data enters rendering. Face boxes and pixel-level clipping overlays are deferred.

## Verification and limitations

See [VERIFICATION.md](VERIFICATION.md) for actual checks/timings. Generated fixtures cover known exposure/color/noise/detail/geometry, masks, safe schemas, persistence, caching, recipe independence, cancellation and a real LibRaw generated-DNG path. They do not certify real Canon CR3 portrait quality. No real user-photo path was available during initial implementation. No face model, sky model, calibrated scene classifier, full-resolution noise/acuity path, automatic editing, trained style, clustering, or macOS verification is claimed.
