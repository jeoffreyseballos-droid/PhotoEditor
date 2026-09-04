# Photo Editor — Phase 3

A local-first desktop foundation for Windows 11 x64 and macOS Apple Silicon. Built with Tauri 2, React/TypeScript, and independent Rust services. This is a development foundation, not a release-ready photo processor.

## Implemented

- Create named jobs with native input/output folder selection; reopen locally stored jobs.
- Recursive still-photo discovery of CR3, CR2, NEF, ARW, DNG, RAF, ORF, RW2, PEF, JPG, JPEG, TIF, TIFF, PNG, HEIC and HEIF, with a central capability registry.
- Nullable EXIF metadata and header dimensions; damaged images retain placeholders instead of stopping the scan.
- Bundled ExifTool camera metadata/embedded JPEG previews, preserved JPEG path, PNG and bounded strip/tile TIFF previews. HEIF without an embedded JPEG remains a capability-warning placeholder.
- Output-inside-input support with automatic subtree exclusion, plus separate metadata/preview/access/readability/traversal diagnostics.
- SQLite migrations, job recovery, idempotent rescanning, and per-asset processing checkpoints.
- Background Rust tasks; 60-photo UI pages, lazy thumbnail requests, and a serialized preview worker.
- CPU/RAM/OS and GPU detection (DXGI/D3D12 on Windows, Metal/unified memory on macOS), local JSON logging, and provider-neutral future service contracts.

- LibRaw-based RAW development, floating-point linear RGB edits, explicit rendered previews, saved controls and full-resolution sRGB JPEG / 16-bit TIFF export.
- Collision-safe output naming, photographic-metadata allowlist and cancellable bounded CPU rendering. Original sources remain immutable.

Phase 2.1 adds typed RGB curves, eight-band color mixing, presence/detail, conservative Lensfun-database optics, creative post-crop vignette and local CPU MODNet portrait masks with independent subject/background development. See [architecture](docs/ARCHITECTURE.md), [limitations](docs/LIMITATIONS.md) and [verification](docs/VERIFICATION.md).

Phase 3 adds the authoritative [Edit Recipe v1 contract](docs/EDIT_RECIPE.md), lossless legacy migration, canonical hashes, dependency-aware preview caching, transactional per-asset history/restore, JSON import/export and a development Recipe Inspector.

No AI auto-editing, training/styles, authentication, cloud sync, licensing or credential implementation is included. Camera support is decoder-dependent; real Canon/Sony acceptance remains required. HEIC/HEIF development is unavailable.

## Development prerequisites

Use Node.js 22.12+ and current stable Rust with `rustfmt` and `clippy`.

- **Windows 11 x64:** install Visual Studio Build Tools with the **Desktop development with C++** workload and a Windows SDK, Rust's `x86_64-pc-windows-msvc` toolchain, and Microsoft Edge WebView2 Runtime.
- **macOS Apple Silicon:** install Xcode Command Line Tools and native `aarch64-apple-darwin` Rust. The bundled metadata helper uses `/usr/bin/perl`. The deployment minimum is macOS 12. Run development/builds on an Apple Silicon Mac.

See the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Linux, Intel Macs, Windows ARM, and mobile are not supported product targets; the desktop crate explicitly rejects other OS/architecture combinations.

```sh
npm ci
npm run desktop
```

`npm run dev` runs a **browser-only UI preview**, with local workflows disabled. It is not a substitute for launching the Tauri desktop app.

```sh
npm run desktop:build
```

Development bundles are unsigned. Release signing, macOS notarization, installer validation, and product branding are not configured as release workflows.

Desktop commands prepare the pinned ExifTool runtime and build LibRaw 0.22.2 from a checksum-verified source archive into local resources. Downloads occur only during preparation, never at application runtime. Do not omit optional npm dependencies. If calling Cargo directly after a clean checkout, run `npm run prepare:native` first. On this Windows workstation, `. ./scripts/activate-msvc.ps1` sets a process-local x64 MSVC environment and project-local caches. Keep `raw/` and `exiftool/` beside a no-bundle executable.

To edit: open a job, select a photo, choose **Develop selected photo**, adjust controls, and click **Update Preview**. Changes save locally. **Export full resolution** writes to the configured output folder, adding a numeric suffix if needed. See the manual acceptance checklist before relying on production results.

## Checks

```sh
npm run prepare:native
npm run format:check
npm run lint
npm test
npm run build
cargo fmt --all -- --check
cargo test -p photo-core -p photo-contracts --locked
cargo clippy --workspace --all-targets -- -D warnings
npm run desktop:build -- --no-bundle
```

To exercise the application services independently of Tauri:

```sh
cargo test -p photo-core -p photo-contracts
```

The GitHub Actions workflow defines Windows MSVC x64 and Apple Silicon checks. Merely adding this workflow does not mean those platforms have passed. See [verification notes](docs/VERIFICATION.md) for the checks actually executed in this workspace.

## Documentation

- [Architecture and boundaries](docs/ARCHITECTURE.md)
- [Known limitations and RAW support](docs/LIMITATIONS.md)
- [Verification record and manual acceptance checklist](docs/VERIFICATION.md)
- [Implementation/change inventory](docs/IMPLEMENTATION.md)

The desktop resolves its database, thumbnail, and log directories through Tauri's platform path resolver. SQLite and logs live locally; thumbnails are disposable but may contain sensitive photo imagery. Protect the OS account and disk accordingly. Do not share the job database or logs without reviewing their contents.
