# Kinora — Plan (v0.2)

_Name: Kinora. v0.1 and v0.2 written 2026-11-06. The first runnable build (Phase 0 + most of Phase 1–2) is in this repo; see README.md for what works today._

## 1. Overview

A self-hosted media server and web app meant to fully replace Jellyfin over time. You point it at folders of movies, TV, music and more, and it scans them, fetches metadata from TMDB, and streams to browsers (and later Android TV / Apple TV). When a client can't play a file directly, it transcodes on whatever GPU is available. Everything beyond a small core is a **module** that can be turned off, and the same extension points will later host **community plugins** and a **marketplace**.

**Goals, in priority order**

1. **Lightweight**: one native binary plus a bundled FFmpeg. Targets: under 50 MB RAM at idle, under 1 s cold start, no background CPU when nobody is watching. Disabled modules cost nothing.
2. **Runs everywhere**: Windows, Linux and macOS (x86_64 + ARM64). Docker is the simplest install on Linux; native installers cover Windows and macOS where Docker can't reach the GPU (see §6).
3. **Uses any GPU**: NVIDIA, AMD, Intel and Apple, with auto-detection and a CPU fallback. Direct play first (zero CPU).
4. **Excellent HDR / Dolby Vision**: direct play to capable clients, correct tone mapping when transcoding (see §5).
5. **Easy to test**: almost all logic lives in pure functions that run in milliseconds under `cargo test`, with no FFmpeg, disk or network needed.
6. **Extensible**: plugin-shaped from day one (see §4).

## 2. Answers so far (2026-11-06)

| Question | Answer |
|---|---|
| OS | Windows, Linux and macOS all first-class |
| Install | Docker, as simple as possible |
| Clients | Web app first; Android TV + Apple TV later |
| Metadata | TMDB approved |
| Remote access | Via Tailscale or the user's own reverse proxy; the server must behave correctly behind both |
| GPUs | All vendors: NVIDIA, AMD, Intel (+ Apple on macOS) |
| HDR | Dolby Vision must "just work" |
| Adaptive streaming | Auto-adjust to network and hardware limits (later, but design for it now) |
| Scope | Eventually everything Jellyfin does |
| Extensibility | Toggleable modules; community plugins and a marketplace later |
| Still open | Jellyfin API compatibility (assumed **no**, since we'll build our own TV apps), Vite vs Next.js for the UI (assumed **Vite**), household users from day one (assumed **yes**) |

## 3. Architecture

```
                ┌──────────────────────── single binary ─────────────────────────┐
browser ──────► │  HTTP layer (axum)  ── auth/roles ── public API (OpenAPI)      │
Android TV ───► │        │                                   │                   │
Apple TV  ────► │        ├── embedded web UI                 ▼                   │
(via proxy /    │        │                      ┌──────── kernel ─────────┐      │
 Tailscale)     │        │                      │ module registry          │      │
                │        │                      │ event bus                │      │
                │        │                      │ settings, scheduler      │      │
                │        │                      │ library + DB (SQLite)    │      │
                │        │                      └─────┬───────────┬────────┘      │
                │        │       extension points ────┘           │               │
                │   ┌────┴─────────┬──────────────┬───────────────┴─────┐         │
                │   │ built-in     │ built-in     │ built-in            │ Wasm    │
                │   │ TMDB module  │ transcode    │ subtitles, music,   │ plugin  │
                │   │              │ module       │ trickplay, ...      │ host    │
                │   └──────────────┴──────┬───────┴─────────────────────┴─(later)─┘
                └─────────────────────────│──────────────────────────────────────┘
                                          ▼
                          bundled ffmpeg / ffprobe child processes
                     (NVENC · QSV · VAAPI · AMF · VideoToolbox · Vulkan · CPU)
```

### Cargo workspace layout

A Rust "workspace" is one repo with several crates (packages). Keeping the pure logic in its own crate is what makes testing fast.

```
crates/
  core/          pure logic, no I/O: models, filename parsing, playback decision
                 engine, ffmpeg argument builder, HDR/DV rules, permissions
  plugin-api/    the extension-point traits + event types + manifest format.
                 Versioned with semver; this is the "SDK" plugins compile against
  kernel/        module registry, event bus, settings, scheduler, DB (sqlx)
  media/         ffprobe/ffmpeg runner, hardware detection, transcode sessions
  modules/
    tmdb/        metadata provider (implements plugin-api traits)
    local-nfo/   .nfo + local artwork provider
    transcode/   decision engine glue + HLS sessions
    subtitles/   extraction, conversion, burn-in
    ...          one crate per feature module
  plugin-host/   (later) Wasmtime host that adapts Wasm plugins to the same traits
  server/        axum routes, auth, wiring; produces the binary
web/             web UI (static SPA, embedded in the binary)
packaging/       Dockerfile, compose examples, Windows/macOS installers
tests/fixtures/  tiny generated media (incl. HDR10 / DV samples) for integration tests
docs/            plan.md, decisions.md
```

`core` plays the role `lib/engine` plays in the TS apps.

### How playback works

1. The client sends its capabilities (containers, codecs, HDR/DV support, max resolution) and, later, measured bandwidth.
2. `core::decide_playback(media, client_caps, network, hw_budget, user_limits)` returns `DirectPlay`, `Remux`, or `Transcode { video, audio, subs, hdr_mode }`. It's a pure function and the most heavily tested one. The `network` and `hw_budget` inputs exist from day one so adaptive streaming slots in later without redesign.
3. Direct play: serve the file with HTTP range requests (zero CPU).
4. Remux/transcode: `core::build_ffmpeg_args(plan, hw_profile)` produces the argument list; `media` spawns FFmpeg writing HLS fMP4 segments; the server serves the playlist and segments and kills FFmpeg on stop or long seek.
5. **Adaptive streaming (later)**: generate a multi-rendition HLS ladder only when needed; the client player (hls.js / ExoPlayer / AVPlayer) switches renditions based on bandwidth. The server also downgrades when the GPU is at its session or load limit.

## 4. Plugin and module architecture

This is the part that is hardest to retrofit, so it shapes the design from day one.

### Principles
- **Everything optional is a module.** TMDB, subtitles, music, trickplay, notifications, and later DLNA and live TV each implement the same extension points that community plugins will use. If a built-in module works through the plugin API, third-party plugins can do anything the built-ins can.
- **Turning a module off means it's never started**: no memory, no background tasks, no routes. The admin UI has a Modules page with toggles.
- **Hot paths stay native.** Streaming bytes, transcoding and DB access never pass through a plugin boundary, so plugins can't slow down playback.
- **Stable, versioned contract.** `plugin-api` is semver-versioned; every plugin declares which API version it targets, and the server refuses incompatible ones with a clear message.

### Extension points (first set)
| Extension point | Built-in examples | Community examples |
|---|---|---|
| `MetadataProvider` | TMDB, local `.nfo` | TVDB, AniDB, MusicBrainz, custom scrapers |
| `ImageProvider` | TMDB, local artwork | Fanart-style art sources |
| `LibraryType` (scanner + item model) | Movies, TV, Music | Audiobooks, comics, photos |
| `SubtitleProvider` | Embedded/sidecar | Online subtitle download |
| `EventSubscriber` (playback started, item added, user created…) | Activity log | Discord/Telegram notifications, scrobbling (Trakt, Last.fm) |
| `ScheduledTask` | Library scan, cleanup | Backups, intro detection |
| `AuthProvider` | Local accounts | OIDC/SSO, LDAP |
| `HttpRoutes` (namespaced `/plugins/<id>/…`) | — | Webhooks, custom APIs |
| `UiExtension` (later) | — | Settings pages, dashboard widgets, item-page panels |

### How plugins are loaded (three stages)
1. **Stage 1 (MVP): compiled-in modules.** Each module is a Rust crate registered with the kernel and can be toggled at runtime. Simple, fast, fully type-checked.
2. **Stage 2: WebAssembly plugins** via Wasmtime and the WebAssembly Component Model (interfaces described in WIT files, generated from `plugin-api`). This is the recommended route for community plugins:
   - **One `.wasm` file runs on Windows, Linux, macOS, x86 and ARM.** Authors don't build per-platform, which matters a lot for a marketplace.
   - **Sandboxed with explicit permissions.** A plugin only gets what its manifest declares and the admin approves (for example network access to `api.example.com`, or read access to library metadata). A buggy or malicious plugin can't read your files or crash the server.
   - **Any language that compiles to Wasm components** (Rust first, then others such as Go, JS or Python as their tooling matures).
   - _Trade-offs_: calls cost microseconds rather than nanoseconds (fine, since plugins aren't on hot paths), and the Component Model tooling is still maturing.
3. **Stage 3: marketplace.** A plugin repository is a JSON index (id, versions, API compatibility, download URL, checksum, signature). The server can subscribe to several repositories (official + community), like package managers do. Signing and review come before anything is installable with one click.

**Rejected options**: native `.dll`/`.so`/`.dylib` plugins (Rust has no stable ABI, authors would need three OS × two CPU builds, and a crash takes the server down), and out-of-process plugins over HTTP/gRPC (heavier for users; possible later as an escape hatch for plugins that need native code).

**Plugin manifest (sketch)** `plugin.toml`: id, name, version, author, `api_version = "^1"`, extension points provided, permissions requested, settings schema (JSON Schema, so the UI can auto-render a settings form).

**Web UI extensions (later)**: plugins ship Web Components rendered in defined slots, or a sandboxed iframe for whole pages. Plugin settings pages are auto-generated from the settings schema, so most plugins need no UI code at all.

## 5. Video, GPUs and Dolby Vision

### "Is there a library for every GPU?" Yes: FFmpeg itself.
FFmpeg is the abstraction layer. It already speaks every vendor's hardware API; our job is to pick the right flags per machine. We **bundle a known-good static FFmpeg build** (built with NVENC, QSV/oneVPL, VAAPI, AMF, VideoToolbox, Vulkan, libplacebo, OpenCL) in the Docker image and installers. Then behaviour doesn't depend on whatever FFmpeg the OS happens to have, and users never install it themselves.

| Backend | Vendor | OS |
|---|---|---|
| **NVENC / NVDEC (CUDA)** | NVIDIA | Windows, Linux |
| **QSV (oneVPL)** | Intel iGPU/Arc | Windows, Linux |
| **VAAPI** | Intel, AMD | Linux |
| **AMF** | AMD | Windows (Linux support is newer/limited) |
| **D3D11VA / D3D12** (decode) | any | Windows |
| **VideoToolbox** | Apple Silicon / Intel Mac | macOS |
| **Vulkan** (decode/encode, libplacebo filters) | cross-vendor | Windows, Linux | Future "one path for every GPU"; still maturing for encode |
| **Software** (x264/x265/SVT-AV1) | CPU | any |

**Auto-detection**: at startup (and on demand from the admin UI) the `media` crate lists hardware devices, runs a ~1 s test encode/decode per backend and codec, and saves a capability profile (which codecs decode/encode, 10-bit support, tone mapping available, max concurrent sessions). The decision engine only uses what the probe proved works. Users can override it.

**Full GPU pipeline**: decode → scale → tone map → encode all on the GPU, so frames never copy back to system RAM. That's where most of the performance win is.

### Dolby Vision
Dolby Vision comes in several "profiles", and each needs different handling:

| Profile | Typical source | Has HDR10 fallback? | Strategy |
|---|---|---|---|
| **5** | Streaming-service rips | No (proprietary colour space) | Direct play to DV-capable clients; otherwise tone map with libplacebo, which applies the DV metadata (without this, colours come out green/purple) |
| **7** | UHD Blu-ray remux (dual layer) | Yes | Direct play where supported; otherwise convert to 8.1 or use the HDR10 base layer |
| **8.1** | Most modern releases | Yes (HDR10) | DV clients get DV; HDR10 clients get the base layer by remuxing without the DV metadata (no transcode); SDR clients get a tone-mapped transcode |
| **8.4** | Phones, HLG | Yes (HLG) | Same pattern as 8.1 |

- **Clients**: Apple TV (AVPlayer) and many Android TV devices (ExoPlayer/Media3) play DV natively, which is the best experience and one reason the native TV apps matter. In browsers, Safari plays DV in HLS fMP4, while Chrome/Firefox mostly can't. So the web app will usually get HDR10 or a tone-mapped SDR stream, and the decision engine handles that automatically.
- **Tone mapping** (HDR/DV → SDR): libplacebo (via Vulkan) is the highest-quality cross-vendor option and understands DV metadata; vendor-specific filters (CUDA, VAAPI/QSV, OpenCL, VideoToolbox) are fallbacks.
- **Testing**: the DV/HDR rules are a pure table in `core` with snapshot tests for every (profile × client × GPU) combination. Real DV sample clips run in an opt-in integration suite.

## 6. Running it: Docker, native apps, proxies

### Docker (simplest path, Linux)
```yaml
services:
  mediaserver:
    image: <tbd>/mediaserver:latest
    ports: ["8420:8420"]          # port TBD; deliberately not Jellyfin's
    volumes:
      - ./config:/config          # SQLite DB, settings, plugins, image cache
      - /path/to/media:/media:ro
    devices: ["/dev/dri:/dev/dri"]   # Intel/AMD GPU (delete if none)
    # NVIDIA: add `gpus: all` (needs NVIDIA Container Toolkit)
    restart: unless-stopped
```
One image for amd64 and arm64, with FFmpeg and GPU user-space drivers included. The first run opens a setup wizard in the browser.

### Native installers (Windows and macOS)
Docker on Windows and macOS runs inside a Linux VM, so GPU access is limited: none on macOS, and on Windows only NVIDIA via WSL2. To get "any GPU on any OS", we also ship:
- **Windows**: an installer that runs as a background service plus a tray icon (QSV, NVENC, AMF, D3D11VA all work natively).
- **macOS**: a signed app/menu-bar item using VideoToolbox.
- **Linux**: a static binary + systemd unit, in addition to Docker.

Rust cross-compiles to all of these from one codebase, and CI builds every target on each release.

### Reverse proxies and Tailscale
- **Base path setting** (e.g. `https://example.com/media/`) and only relative URLs in the UI, so subpath proxying works.
- **Trusted proxies list**: honour `X-Forwarded-For/Proto/Host` only from configured proxy IPs, so client IPs, "local vs remote" detection and HTTPS links are correct and can't be spoofed.
- **Published URL setting** for links and the future TV-app pairing.
- WebSockets/SSE work through Nginx, Caddy and Traefik; docs ship sample configs for each.
- **Tailscale**: works as-is. A setting decides whether Tailscale addresses (100.64.0.0/10) count as "local" (full bitrate) or "remote" (bitrate limits apply).
- **HTTPS**: left to the proxy/Tailscale by default; optional built-in TLS with a user-supplied cert.

## 7. Rust choices, explained briefly

Rust compiles to a native binary with no garbage collector, so memory stays low and predictable, and the compiler catches whole classes of bugs (null pointers, data races) before the program runs. The cost is a steeper learning curve and slower compiles. Most of our code (the `core` crate) is plain functions on plain data, which is the easiest kind of Rust.

- **Async runtime: Tokio.** Rust doesn't ship one; Tokio is the de facto standard, and everything we need assumes it.
- **Web framework: axum.** From the Tokio team. Handlers are async functions, with `tower-http` for static files, range requests, compression and tracing. Handlers can be tested in-process. (Actix Web is comparable; Rocket is friendlier but less flexible.)
- **FFmpeg: run the bundled CLI, not bindings.** `ffmpeg-next` and `rsmpeg` exist, but the encoding speed is the same either way. Running FFmpeg as a child process isolates crashes, keeps builds simple on all three OSes, makes GPU flags straightforward, keeps FFmpeg's (L)GPL licence separate from our binary, and makes the command a testable `Vec<String>`.
- **Database: SQLite via sqlx.** One file, no server, compile-time checked SQL, plain `.sql` migrations. WAL mode allows reads during scans.
- **Plugins: Wasmtime** (stage 2). The most mature Wasm runtime, written in Rust, built around the Component Model.
- **API spec: utoipa** generates an OpenAPI document from the Rust route definitions. That document drives typed clients for the web UI (TypeScript), Android TV (Kotlin) and Apple TV (Swift), so the three clients can't drift from the server.

### Testability
- **Unit tests** beside the code, mostly on `core`: filename parsing, the playback decision table, ffmpeg argument building, DV rules, permissions.
- **Snapshot tests** (`insta`): the expected FFmpeg command per scenario lives in a file; any change shows as a reviewable diff.
- **Property tests** (`proptest`): thousands of random filenames and stream combinations.
- **Plugin contract tests**: one shared test suite that every module (built-in or community) must pass for its extension point.
- **API tests**: axum handlers in-process with a temp SQLite DB.
- **Integration tests** (opt-in): generate tiny clips with FFmpeg's test sources, plus HDR10/DV samples, and run real scans and transcodes. CI runs on Windows, Linux and macOS.
- **I/O behind traits** (HTTP, filesystem, process spawning) so tests use fakes.
- CI: `cargo fmt`, `cargo clippy`, `cargo nextest`; Vitest + Playwright for the web UI.

Other mainstream crates (listed so you can veto): `serde`, `tracing`, `notify` (file watching), `argon2`, `reqwest`, `rust-embed`, `thiserror`/`anyhow`.

## 8. Which parts of the default stack still apply

See decisions.md for the reasoning.

| Default | Applies? | What we do instead |
|---|---|---|
| Next.js | Partly | Static SPA served by the binary; Vite + React assumed (D7) |
| TypeScript strict, Tailwind, shadcn/ui, Recharts, Vitest | **Yes** | Same UI conventions as the other apps |
| Convex | No | SQLite inside the binary (D2) |
| WorkOS AuthKit | No | Local accounts; OIDC/LDAP via `AuthProvider` modules later (D3) |
| Vercel | No | Docker + native installers (D5) |
| Stripe | Not now | Only relevant if a paid marketplace or hosted tier ever happens |
| Multi-tenant orgs | Adapted | One install = one household/org; same roles (D4) |
| Roles owner/admin/editor/viewer | **Yes** | Plus per-library access and parental ratings |
| One auth helper | **Yes** | `require_role` axum extractor on every route; plugins get scoped permissions too |
| Dates as Unix ms | Yes | Media positions also in ms (D8) |
| Pure logic + unit tests | **Yes** | `crates/core` + `cargo test` |
| `.env.example` | Yes | Plus a `config.toml` example and documented Docker env vars |

## 9. Data model (first pass)

IDs are UUIDv7 (sortable). Times are Unix ms.

- **users**: id, username, password_hash, role, max_parental_rating, created_at, last_login_at
- **sessions / devices**: token_hash, user_id, device_name, client, created_at, expires_at, last_seen_ip
- **libraries**: id, name, library_type (owned by a module), paths[], settings JSON
- **library_access**: user_id, library_id
- **items**: id, library_id, kind, parent_id, title, sort_title, year, overview, official_rating, provider_ids JSON, added_at, updated_at
- **media_files**: id, item_id, path, size, mtime, container, duration_ms, probe_json, last_scanned_at
- **streams**: media_file_id, index, kind, codec, profile, bit_depth, color_transfer, hdr_format (sdr/hdr10/hdr10plus/hlg/dv), dv_profile, dv_compat_id, language, channels, is_default, is_forced
- **images**: item_id, kind, source, local_path, blurhash
- **people / item_people**
- **user_item_data**: user_id, item_id, position_ms, played, play_count, last_played_at, favorite
- **modules**: id, version, enabled, source (builtin/wasm), api_version, permissions_granted JSON, settings JSON
- **plugin_kv**: module_id, key, value (each plugin gets its own namespaced storage; plugins never touch core tables directly)
- **activity_log**: id, time, user_id, event_type, payload JSON
- **transcode_sessions**: in memory only

## 10. Roadmap

**Phase 0 — Learn + spike (1–2 weekends).** Rust Book chapters 1–10. Spike: an axum server that direct-plays one file and HLS-transcodes another on your GPU, plus one DV 8.1 file tone-mapped to SDR. That proves the riskiest parts first.

**Phase 1 — Kernel + library + direct play.** Workspace, CI on 3 OSes, SQLite, users/roles, setup wizard, **module registry and event bus** (even with only built-ins), Movies/TV library types, scanner + file watching, TMDB + local `.nfo` modules, browse/search API with OpenAPI, web UI (home, library, detail, player, admin), direct play, resume/watched, proxy settings.

**Phase 2 — Transcoding + HDR.** Bundled FFmpeg, hardware probing for all backends, decision engine, argument builder, HLS sessions, subtitles (text → WebVTT, burn-in for image subs), HDR10/DV handling and tone mapping, concurrent-session limits, admin view of active streams (every playback method, see D21).

**Phase 3 — Packaging.** Multi-arch Docker image, Windows service installer, macOS app, Linux binaries, proxy docs, backups/restore.

**Phase 4 — Jellyfin-level breadth (built-in modules).** Music library + audio transcoding, collections and playlists, trickplay thumbnails, intro/credit detection, chapter images, "next up"/continue watching polish, parental controls, multiple versions of one item, extras/trailers, scheduled tasks UI, notifications.

**Phase 5 — Adaptive streaming.** Bandwidth-aware HLS ladders, GPU-load-aware downgrading, per-user remote bitrate limits.

**Phase 6 — TV clients.** Android TV (Kotlin, Media3/ExoPlayer) and Apple TV (Swift, AVPlayer), both using clients generated from the OpenAPI spec; device pairing with a code.

**Phase 7 — Community plugins.** Wasm plugin host, permission prompts, plugin SDK + template repo + docs, contract test kit.

**Phase 8 — Marketplace.** Repository index format, signing, in-app browse/install/update, review process.

**Later / maybe.** Live TV & DVR, DLNA, watch-together (SyncPlay-style), books/comics/photos, OIDC/LDAP, offline downloads in the TV/mobile apps.

## 11. Risks

- **Scope.** "Everything Jellyfin has" is years of work for a large team. Mitigation: a strict phase order, and the module system lets features land independently without destabilising the core.
- **Rust learning curve.** Keep complex async code in a few crates; `core` and modules stay simple. Lean on `clippy` and Claude.
- **GPU matrix.** 3 OSes × 4+ vendors × many driver versions. Mitigation: probe-based detection, the software fallback always works, and community bug reports include the probe output.
- **Dolby Vision** depends on FFmpeg/libplacebo versions and client support. Mitigation: we control the bundled FFmpeg version, plus a sample-clip test suite.
- **Plugin API stability.** Once community plugins exist, breaking changes hurt. Mitigation: build all built-in modules on it first and only declare v1 stable after several modules exist.
- **Plugin security.** The Wasm sandbox and permissions help; signing + review come before the marketplace opens.
- **TMDB terms.** Attribution is required in the UI, and the API key must be configurable per install. Commercial use needs a separate agreement if this ever becomes paid.
- **macOS/Windows distribution** needs code-signing certificates (Apple Developer account; a Windows signing cert), which are new paid services. Ask before buying.

## 12. Open questions

1. Jellyfin API compatibility: assumed **not needed** since we'll build our own TV apps. Confirm?
2. Web UI: **Vite + React** assumed (instead of Next.js static export). OK?
3. Household users from day one: assumed **yes**.
4. What machine will **you** test on first (OS + GPU)? That decides which hardware backend gets polished first.
5. Is Wasmtime OK as a future dependency for community plugins? (It's open source and embedded, not an external service.)
