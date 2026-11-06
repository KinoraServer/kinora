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
