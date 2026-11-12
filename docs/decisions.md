# Kinora — Decisions

Architecture choices that deviate from the project's default stack. Status: **proposed** until the user confirms.

## D1 — Backend in Rust, not Next.js + Convex (proposed, 2026-11-06)

**Context**: The default stack targets hosted SaaS apps. This is a self-hosted server that has to run on home hardware, offline, stream large files, and manage ffmpeg processes.
**Decision**: A single Rust binary (Tokio + axum) holds the API, scanner and transcode manager.
**Why**: Low, predictable memory and CPU; one artifact to deploy; strong compile-time checks. Convex can't serve local files or spawn ffmpeg, and needs internet.
**Trade-off**: New language for the user and slower iteration than TS.

## D2 — SQLite (sqlx) instead of Convex (proposed, 2026-11-06)

**Decision**: Embedded SQLite in WAL mode, accessed with sqlx (compile-time checked queries), plain SQL migrations.
**Why**: Zero ops, single-file backup, fast enough for personal and household libraries, works offline.
**Trade-off**: No realtime subscriptions out of the box; we use server-sent events or WebSockets where the UI needs live updates (scan progress, active streams).

## D3 — Local accounts instead of WorkOS AuthKit (proposed, 2026-11-06)

**Decision**: Username + password (argon2id) with hashed session tokens stored in SQLite. Roles owner/admin/editor/viewer plus per-library access. Optional OIDC later, which would also allow WorkOS or any other IdP.
**Why**: The server has to work without internet access, and TV clients log in with simple username/password or device codes.

## D4 — Single tenant per install; "org" = the server (proposed, 2026-11-06)

**Decision**: Tables don't carry `orgId`. One install = one household/organisation. Every route still goes through one role-checking helper (`require_role`) and never trusts IDs from the client.
**Why**: Self-hosted servers aren't shared between unrelated organisations; adding `orgId` everywhere adds cost and no benefit. If a hosted multi-tenant version is ever wanted, that would be a separate decision.

## D5 — Self-hosted distribution instead of Vercel (updated 2026-11-06)

**Decision**: Docker (multi-arch) is the primary, simplest install on Linux. Native Windows (service + tray) and macOS (app) installers ship as well, plus Linux binaries.
**Why**: The user requires Windows, Linux and macOS, and any GPU. Docker on macOS has no GPU access and Docker on Windows only passes through NVIDIA (via WSL2), so native builds are the only way to get hardware transcoding on those OSes.
**Trade-off**: More packaging work and code-signing certificates (paid; ask before buying).

## D6 — Drive ffmpeg as a child process, not via Rust bindings (proposed, 2026-11-06)

**Decision**: Use the `ffmpeg`/`ffprobe` CLI; build arguments in pure, snapshot-tested functions.
**Why**: Same encode speed, crash isolation, simpler builds, easy hardware flags, clean licensing, highly testable. Bindings (`ffmpeg-next`, `rsmpeg`) stay an option for narrow, proven needs.

## D7 — Web UI as a static SPA embedded in the binary (accepted, 2026-11-06)

**Options**: Vite + React (recommended) vs Next.js `output: 'export'`. Either way we keep TypeScript strict, Tailwind, shadcn/ui, Recharts and Vitest.
**Why Vite is recommended**: There's no Node server at runtime, so Next.js's server features (SSR, server actions, route handlers) go unused, and static export has limits with dynamic routes. A plain SPA is simpler to embed.

## D8 — Media positions in milliseconds

Positions and durations are integer ms, matching the "dates as Unix ms" default. If we add a Jellyfin-compatible API, it converts to/from that API's 100-ns ticks at the edge.

## D9 — Module/plugin architecture from day one (proposed, 2026-11-06)

**Decision**: Every optional feature is a module behind versioned extension-point traits in `plugin-api`. Stage 1: compiled-in Rust modules, toggleable at runtime. Stage 2: community plugins as WebAssembly components run by Wasmtime, sandboxed with manifest-declared permissions. Stage 3: a marketplace built from signed repository indexes.
**Why**: The user wants features they can turn off, community plugins, and a marketplace later. Building the built-ins on the same API is the only way to keep that API honest. Wasm gives one plugin file for every OS/CPU, plus sandboxing.
**Rejected**: Native dynamic libraries (no stable Rust ABI, per-platform builds, crashes take the server down). Out-of-process gRPC plugins (heavier for users; kept as a possible escape hatch).
**Trade-off**: More upfront design; the Wasm Component Model tooling is still maturing.

## D10 — Bundle a pinned static FFmpeg build (proposed, 2026-11-06)

**Decision**: Docker images and installers ship our own known-good FFmpeg (NVENC, QSV/oneVPL, VAAPI, AMF, VideoToolbox, Vulkan, libplacebo, OpenCL). An override path can point at a system FFmpeg.
**Why**: Consistent GPU and Dolby Vision behaviour on every OS. Users never install FFmpeg themselves.
**Trade-off**: We maintain a build pipeline (or start from a reputable static build) and must comply with the GPL when distributing it (publish the build scripts and source offer).

## D11 — OpenAPI as the client contract (proposed, 2026-11-06)

**Decision**: Generate the OpenAPI spec from Rust (utoipa), and generate typed clients for the web UI (TS), Android TV (Kotlin) and Apple TV (Swift) from it. We build our own API rather than copy Jellyfin's (unless the user asks for compatibility).

## D12 — Proxy-aware networking (proposed, 2026-11-06)

**Decision**: Configurable base path and published URL, and a trusted-proxy list for `X-Forwarded-*`. A setting decides whether Tailscale addresses count as local. TLS is terminated by the proxy or Tailscale by default.

## D10a — Docker image uses Debian's FFmpeg for now (2026-11-06)

**Decision**: The Docker image installs FFmpeg 7.1 from Debian trixie rather than a custom static build. Debian's build already includes NVENC/NVDEC, Quick Sync (oneVPL), VAAPI, Vulkan, libplacebo and zimg, and gets security updates with the base image. A pinned static build (D10) is still the plan for the native Windows/macOS installers, where there is no package manager.
**Trade-off**: FFmpeg version follows Debian stable (7.1) instead of the newest release.

## D13 — Hash routing in the web app (2026-11-06)

**Decision**: The SPA uses `#/` routes and only relative URLs, and Vite builds with `base: "./"`.
**Why**: It works behind any reverse proxy sub-path (`https://example.com/kinora/`) with zero configuration, which matters for users with their own proxies.
**Trade-off**: URLs contain `#`. Can switch to path routing plus a base-path setting later.

## D14 — Themes are token files (2026-11-06)

**Decision**: A theme is JSON: an id, a name, light/dark, and values for a fixed list of design tokens (colours, radius, font). The server validates and serves built-in plus drop-in themes (`<config>/themes/*.json`); the web app applies them as CSS variables that every Tailwind colour utility reads. The same format will be what theme plugins and marketplace themes ship.
**Why**: Making a theme needs no code and can't break the app (values are validated against CSS injection).

## Constraints

- Don't copy Jellyfin code (GPL-2.0), names, logos or UI design. Any API compatibility is implemented clean-room from public API documentation.
- Ask before adding new external services (metadata providers, telemetry, etc.).

## D15: Player styles are layouts in one component; skins are sanitised markup + CSS

**Status:** accepted (2026-11-06)

All behaviour lives in one controller in `web/src/components/MediaPlayer.tsx`;
the four built-in layouts only arrange shared controls, so every style has the
same features. A theme may choose a layout and a few look knobs (data only).
Fully custom markup is a separate module ("player-skins"): folders of
`skin.html` + `skin.css` that are sanitised (server and browser), scoped, and
wired through `data-kn-*` attributes. Rationale: themes stay drop-in data;
anything that changes structure is a plugin-tier extension with its own
safety checks. Deviation from the default stack: none (no new dependencies).

Update (2026-11-06): the style is one server-wide setting (`player_style`,
`GET`/`PUT /api/player/style`), chosen by an admin on the Modules & themes
page, so every client shows the same player. The in-player style menu was
removed.

## D16 — Android TV client in Kotlin + Compose for TV + Media3 (accepted, 2026-11-06)

**Decision**: The Android TV app lives in `clients/android/` in this repo. It is native Kotlin with Jetpack Compose for TV and AndroidX Media3 (ExoPlayer), with plain-Kotlin `core` modules (models, API client, capability rules) that are unit-tested on the JVM. It is a thin client: it reports what the TV can play and the server's decision engine picks direct play, remux or transcode. TV comes first (tested on an NVIDIA Shield); a phone app will reuse the same `core` modules.
**Why**: The player is most of the app, and Media3 gives full control over Dolby Vision, HDR, PGS subtitles and audio passthrough. Keeping it in this repo means a server API change and the client change land in one PR.
**Deviation from the default stack**: Kotlin instead of TypeScript/Next.js, Material for TV instead of Tailwind/shadcn, JUnit instead of Vitest, sideloaded APKs from GitHub Releases instead of Vercel. The API client is hand-written until the OpenAPI spec (D11) exists. Full reasoning: the Android plan in the project files (`kinora-android/docs/`).

## D17 — Storybook with Vitest for web component tests and visual checks (accepted, 2026-11-07)

**Decision**: The web app uses Storybook 10 (React + Vite) for its components, starting with the video player. Every story runs as a Vitest test in headless Chromium through `@storybook/addon-vitest`, with axe accessibility checks failing the test. Visual tests are free and local: Vitest's `toMatchScreenshot` on a shared device list (`web/.storybook/devices.ts`), opt-in with `pnpm test:visual`, plus a CI job that uploads the screenshots as an artifact. `@storybook/addon-mcp` exposes the stories to coding agents.
**Why**: The player has to work on phones, tablets, laptops and TVs, and that is only checkable by rendering it at each size. Stories with mocked data need no server, so they double as component tests.
**Deviation from the default stack**: new dev dependencies only (Storybook, its addons, Vitest browser mode, Playwright). No hosted service: Chromatic was considered and left out because it is paid past its free tier. Details: `docs/storybook.md`.

## D18 — App skins are templates with bindings, not code (accepted, 2026-11-07)

**Decision**: Whole-app skins (module "app-skins") are folders of HTML templates, one CSS file and assets in `<config>/app-skins/`. Templates use `data-kn-*` bindings to read view models (Card, Home, Item, ...) and to link, act, scroll rails and rotate carousels; Kinora renders them as React elements, so skins never run JavaScript. One rules file (`crates/plugin-api/skin-rules.json`) drives the server checker, the `kinora skin check` CLI, the editor autocomplete files and the browser sanitiser. Skin CSS goes in the last cascade layer inside `@scope ([data-app-skin]) to ([data-kn-standard])`, so it beats Kinora's defaults without overrides and can't reach Kinora's own pages. Every page falls back to Kinora's page on its own (missing template, render error, unsupported device or browser), admin and settings never use a skin, and `#/?skin=off` always restores the standard look. Guide: `docs/app-skins.md`.
**Why**: Skin authors get full control of markup and layout without fighting existing styles, while a skin stays data that can be checked, previewed and shared safely.
**Deviation from the default stack**: none. No new dependencies: zips are read in the browser (`DecompressionStream`), images are resized with the FFmpeg Kinora already needs, and spatial navigation is built in.

## D19 — Copy HEVC into fMP4 HLS; OpenCL tone mapping; CPU tone mapping capped at 1080p (proposed, 2026-11-08)

**Decision**: When a client plays HEVC (and can show its HDR format) but not the file's container or audio, the video is copied into fragmented MP4 HLS segments (`seg_N.m4s` + `init.mp4`, tag `hvc1`, `movflags=+frag_discont` so restarted runs keep source timestamps) and only the audio is transcoded. H.264 remuxes stay MPEG-TS. Dolby Vision is copied only when its base layer is usable on its own (8.1 → HDR10, 8.4 → HLG, 8.2 → SDR); profile 5 is always tone mapped, since the segments carry no DV configuration. Tone mappers are tried in the order libplacebo (Vulkan), `tonemap_opencl` (GPU, probed on 4K frames), zscale (CPU). Dolby Vision is decoded on the GPU unless libplacebo is the tone mapper (the only one that reads DV metadata). When tone mapping falls back to the CPU, HDR transcodes are capped at 1080p.
**Why**: A 4K DV/HDR10 file with E-AC3 on Chrome for Mac was software-decoded, CPU tone mapped and re-encoded at 4K, saturating the CPU on an i9-13900K while the GPU idled. Copying the video avoids all of that; the other changes make the transcodes that remain far lighter.
**Deviation from the default stack**: none. The Docker image adds the OpenCL loader (`ocl-icd-libopencl1`) and an NVIDIA ICD file; nothing else.

## D20 — Keyframe-aligned HLS segments for copied video (proposed, 2026-11-08)

**Decision**: When video is copied into fMP4 HLS from a Matroska file with a Cues index, the playlist is built from the file's real keyframes: a new segment starts at the first keyframe at least 6 s after the previous one, so segment lengths vary. ffmpeg cuts the copied stream at every keyframe into small pieces (`gop_{run}_N.m4s`); the server reads each piece's start time from its `tfdt` and joins complete pieces into `seg_N.m4s` for the matching playlist entry. Restarts seek to just past a segment's keyframe, and their audio starts one second early (from a second copy of the input) so no audio is lost where the previous run was stopped. The throttle counts a run's progress as the unbroken stretch of segments from where it started, ignoring leftovers from earlier runs. Files without an index, MPEG-TS sessions and video transcodes keep fixed 6 s segments.
**Why**: Copied video can only be cut at keyframes, so with fixed 6 s playlist times segments started and ended at different times from what the playlist said. hls.js then requested the wrong segment after seeks and resumes, left holes in the buffer and stalled.
**Same times from every run** (2026-11-11): pieces keep the file's own timestamps, moved 1 s forward (`-output_ts_offset`), and the playlist counts from the file's first keyframe. A run from the start of a file has negative decode times, which ffmpeg would otherwise shift for that run alone, leaving its segments about 80 ms later than the same segments from a restarted run (a gap or overlap where the two meet). Files whose timestamps don't start at 0 work the same way. ffmpeg starts a restarted run one keyframe early; those pieces are the tail of the segment before and are thrown away instead of becoming a segment with its beginning missing. Fragments are written without a segment index (`skip_sidx`), because with one ffmpeg rewrites the time of each fragment's first frame and shows open-GOP keyframes a few frames early.
**Deviation from the default stack**: none.

## D21 — The dashboard lists every playback through a now-playing registry (accepted, 2026-11-08)

**Decision**: The server keeps an in-memory list of everything playing (`crates/server/src/now_playing.rs`), whatever the method: direct play, direct stream (remux) and transcode. Starting a playback adds an entry and returns a `play_id` (the HLS session id when there is one). Players send a heartbeat every 5 s to `POST /api/now-playing/{play_id}/heartbeat` with their position, state (playing, paused, buffering) and a few stats (render fps, dropped frames, stalls, buffer, bitrate, recent errors); `DELETE /api/now-playing/{play_id}` ends it. Clients without heartbeats are kept alive by progress saves and stream requests, and an entry is dropped after 45 s with no news. `GET /api/sessions` (admins) returns every entry with the item, client, source media, plan, the transcoder's status and the player's stats. Admin > Dashboard shows only these live streams, as cards with expandable Info and Trace sections; the activity log moved to its own Admin > Activity tab.
**Why**: The old list only knew about HLS sessions, so direct play was invisible, and its position was a guess from the last downloaded segment. Heartbeats give the real position and let admins see the player's side of a problem without asking the viewer.
**Deviation from the default stack**: none. No new dependencies; the dashboard polls every 2 s while visible instead of using a push channel.

## D22 — Let go of a video's page cache when its playback ends (accepted, 2026-11-10)

**Decision**: When a transcode session or a direct play ends, and nobody else is playing the same file, the server calls `posix_fadvise(POSIX_FADV_DONTNEED)` on the source file (`crates/server/src/page_cache.rs`, Linux only, a no-op elsewhere). The kernel drops that file's clean cached pages.
**Why**: The Kinora process itself stays around 30 MiB during and after playback. What grew was the container's memory number: reading a video leaves it in the page cache, and in a container that cache is charged to the container. Docker (and Unraid's Docker tab) count recently used cache as used memory, so after an episode the number stayed hundreds of MiB above idle until the kernel got round to reclaiming it. Measured on a 10 minute, 12 Mbit/s HEVC episode with the transcode folder in RAM: 42 MiB idle, about 880 MiB while playing, 540 MiB still shown minutes after stopping; with this change it is back to 62 MiB a second after stopping. During playback the transcode folder (up to 10 segments behind and about 30 ahead) still counts when it is in RAM (/tmp on Unraid).
**Deviation from the default stack**: adds `libc` as a direct dependency on Linux (it was already in the build through tokio).

## D23 — Libraries can be renamed and re-foldered; moved files keep their records (accepted, 2026-11-11)

**Decision**: `PUT /api/libraries/{id}` takes any of `name` and `paths` (and `kind`, which must match: a library's type is fixed, since movie and TV items are built differently). The id never changes, so links, skins and the Android app keep working after a rename. Names are unique (ignoring case). Folders are trimmed of trailing slashes, de-duplicated, must exist and be readable, and may not sit inside one another, in the same library or another one (a file belongs to one library). Changing folders starts a rescan, or queues one right after a scan that is already running. During every scan, a file that vanished and a new file with the same file name and size (a one-to-one match) are treated as a move: the file's record gets the new path, so its item, metadata and watch progress are kept and it isn't probed again. Files no longer under any folder are removed as before, and so are titles left without files.
**Why**: Changing a library's folders, or reorganising files on disk, used to give moved movies new items (their match key is the path), losing watch progress and metadata. Matching on name and size is cheap, needs no hashing of large files, and refusing ambiguous matches keeps it from guessing.
**Deviation from the default stack**: none.

## D21 — The dashboard lists every playback through a now-playing registry (accepted, 2026-11-08)

**Decision**: The server keeps an in-memory list of everything playing (`crates/server/src/now_playing.rs`), whatever the method: direct play, direct stream (remux) and transcode. Starting a playback adds an entry and returns a `play_id` (the HLS session id when there is one). Players send a heartbeat every 5 s to `POST /api/now-playing/{play_id}/heartbeat` with their position, state (playing, paused, buffering) and a few stats (render fps, dropped frames, stalls, buffer, bitrate, recent errors); `DELETE /api/now-playing/{play_id}` ends it. Clients without heartbeats are kept alive by progress saves and stream requests, and an entry is dropped after 45 s with no news. `GET /api/sessions` (admins) returns every entry with the item, client, source media, plan, the transcoder's status and the player's stats. Admin > Dashboard shows only these live streams, as cards with expandable Info and Trace sections; the activity log moved to its own Admin > Activity tab.
**Why**: The old list only knew about HLS sessions, so direct play was invisible, and its position was a guess from the last downloaded segment. Heartbeats give the real position and let admins see the player's side of a problem without asking the viewer.
**Deviation from the default stack**: none. No new dependencies; the dashboard polls every 2 s while visible instead of using a push channel.

## D22 — Let go of a video's page cache when its playback ends (accepted, 2026-11-10)

**Decision**: When a transcode session or a direct play ends, and nobody else is playing the same file, the server calls `posix_fadvise(POSIX_FADV_DONTNEED)` on the source file (`crates/server/src/page_cache.rs`, Linux only, a no-op elsewhere). The kernel drops that file's clean cached pages.
**Why**: The Kinora process itself stays around 30 MiB during and after playback. What grew was the container's memory number: reading a video leaves it in the page cache, and in a container that cache is charged to the container. Docker (and Unraid's Docker tab) count recently used cache as used memory, so after an episode the number stayed hundreds of MiB above idle until the kernel got round to reclaiming it. Measured on a 10 minute, 12 Mbit/s HEVC episode with the transcode folder in RAM: 42 MiB idle, about 880 MiB while playing, 540 MiB still shown minutes after stopping; with this change it is back to 62 MiB a second after stopping. During playback the transcode folder (up to 10 segments behind and about 30 ahead) still counts when it is in RAM (/tmp on Unraid).
**Deviation from the default stack**: adds `libc` as a direct dependency on Linux (it was already in the build through tokio).

## D23 — Libraries can be renamed and re-foldered; moved files keep their records (accepted, 2026-11-11)

**Decision**: `PUT /api/libraries/{id}` takes any of `name` and `paths` (and `kind`, which must match: a library's type is fixed, since movie and TV items are built differently). The id never changes, so links, skins and the Android app keep working after a rename. Names are unique (ignoring case). Folders are trimmed of trailing slashes, de-duplicated, must exist and be readable, and may not sit inside one another, in the same library or another one (a file belongs to one library). Changing folders starts a rescan, or queues one right after a scan that is already running. During every scan, a file that vanished and a new file with the same file name and size (a one-to-one match) are treated as a move: the file's record gets the new path, so its item, metadata and watch progress are kept and it isn't probed again. Files no longer under any folder are removed as before, and so are titles left without files.
**Why**: Changing a library's folders, or reorganising files on disk, used to give moved movies new items (their match key is the path), losing watch progress and metadata. Matching on name and size is cheap, needs no hashing of large files, and refusing ambiguous matches keeps it from guessing.
**Deviation from the default stack**: none.

## D24 — Modules are installed at runtime from a registry; server logic runs as sandboxed WebAssembly (accepted, 2026-11-11)

**Context**: Every module was compiled into the server and only switched on or off (D9, stage 1). The goal now is a small server where a feature is only there after the admin installs it, and can be removed again.
**Options for running downloaded server code**:
1. *Native dynamic libraries*: fastest, but Rust has no stable ABI, each OS and CPU needs its own build, and a bug or a hostile module has the whole server. Rejected (as in D9).
2. *WebAssembly on Wasmtime (JIT)*, the runtime D9 named: fast, but adds roughly 10 MB and long builds, and a JIT is one more thing to get working on every platform.
3. *WebAssembly on wasmi (interpreter)*: small, pure Rust, runs anywhere the server does. Slower, which doesn't matter for modules that mostly wait on the network or the database.
4. *Only themes and skins from the registry*, logic stays compiled in: no new dependencies, but modules would still be packaged.

**Decision**: Option 3. A registry (the kinora-registry repository) publishes four kinds of package: `module` (one WebAssembly file), `theme`, `player-skin` and `app-skin` (plain files). TMDB, Auto audio & subtitles and the Activity log moved there as modules; the Midnight, Daylight, OLED Black and Ember themes, the Neon player skin and the Marquee app skin moved there as files. The server keeps the three engines that render in the web app (themes, player skins, app skins), its own two themes and the Blank starter skin.

- *Module ABI 1*: core WebAssembly, JSON in and out. The module exports `memory`, `kn_alloc` and `kn_call`; it may import one function, `kinora.host_call`, through which it asks for `log`, `http`, `kv_*` and `activity_record`. The manifest lists permissions (HTTPS hosts, storage, activity log) and the host refuses anything else. Each call is a fresh instance with a 64 MB memory cap and a fuel budget. Plain core Wasm rather than the Component Model keeps the ABI independent of the runtime, so moving to Wasmtime later needs no change to modules.
- *Trust*: `index.json` is signed (Ed25519) and checked against keys pinned in the server; the index holds the SHA-256 of every manifest and a manifest the SHA-256 of every file. Packages are downloaded as individual files, so the server needs no archive code. Until a key is pinned (`DEFAULT_REGISTRY_KEYS` is empty), only the hashes are checked.
- *Storage*: a module's data lives in `module_data`, keyed by module and optionally by viewer (deleted with the viewer). Auto tracks' choices were moved there from `track_choices`.
- *Upgrades*: a server that already has users installs the formerly built-in packages once, in the background; a new server installs nothing.

**What can't be installed at runtime**: anything that needs new code in a client. The web app is one bundle and loading scripts from a registry into an admin's session would undo the sandbox, so a module's UI is limited to the settings form Kinora draws from its manifest, and the hooks modules plug into (track changes in the player, the Activity page) ship with the app. The Android TV app is native and can't load code either; it benefits from modules through the server API. The engines that take themes and skins stay in the server for the same reason.
**Trade-offs**: interpreted modules are slower than native code; a module can't hold state in memory between calls (it uses storage, and the host caches HTTP answers when asked); the registry is one more thing to keep running, though it is only static files and installed packages work without it.
**Deviation from the default stack**: two new Rust dependencies, `wasmi` and `ed25519-dalek` (approved 2026-11-11). `rust-version` rises to 1.86 for wasmi. No new services: the registry is a branch of a GitHub repository served as static files. Guide: `docs/modules.md`.
