<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/logo-on-dark.svg" />
    <img src="docs/assets/logo-on-light.svg" alt="Kinora" height="72" />
  </picture>
</p>

# Kinora

A lightweight, self-hosted media server. Point it at your movies and TV shows,
and it scans them, fetches artwork from TMDB, and streams them to any browser,
transcoding on your GPU (Intel, AMD, NVIDIA or Apple) only when a device can't
play a file as-is.

One Rust binary with the web app built in, SQLite for storage, FFmpeg for video.

## Run it with Docker

```bash
docker run -d --name kinora -p 8420:8420 \
  -v /path/to/config:/config \
  -v /path/to/media:/media:ro \
  --device /dev/dri:/dev/dri \
  ghcr.io/kinoraserver/kinora:latest
```

Then open `http://<server-ip>:8420`, create your account, and add a library
from `/media` in **Admin → Libraries**.

- **Intel / AMD GPU**: keep `--device /dev/dri:/dev/dri` (remove it if the
  machine has none, e.g. Docker Desktop on a Mac).
- **NVIDIA GPU**: add `--runtime=nvidia` (NVIDIA Container Toolkit, or the
  "Nvidia Driver" plugin on Unraid).
- **Artwork and descriptions**: install **TMDB metadata** in
  **Admin → Modules & themes** and add a free TMDB API key there (or pass
  `-e TMDB_API_KEY=...`).
- **Transcode temp files**: optionally mount fast storage or a RAM disk at
  `/transcode`.

There's also a [docker-compose.yml](docker-compose.yml) and an
[Unraid template](packaging/unraid/kinora.xml).

### Unraid

1. Copy `packaging/unraid/kinora.xml` to
   `/boot/config/plugins/dockerMan/templates-user/my-kinora.xml`.
2. Docker tab → **Add Container** → pick **kinora** from the template list.
3. Check the media path, then **Apply**.

While the repository is private, the image is private too: either run
`docker login ghcr.io` once on the server (username = your GitHub user,
password = a token with `read:packages`), or make the package public under
GitHub → Packages → kinora → Package settings.

### Reverse proxies and Tailscale

Kinora uses only relative URLs, so it works behind Nginx, Caddy, Traefik or
Tailscale Serve, at the root or under a sub-path like `/kinora/`, with no
settings. HTTPS is left to the proxy.

## What works today

- First-run setup, sign-in, users with roles (owner, admin, editor, viewer)
- Movie and TV libraries, incremental scans (scheduled, plus on demand)
- TMDB metadata and artwork (a module you install from the registry)
- Direct play with seeking; when needed, remux or transcode to HLS
- GPU detection with a real test encode per backend: NVENC, Quick Sync,
  VAAPI, VideoToolbox, with CPU fallback
- HDR10 / HLG / Dolby Vision detection; tone mapping to SDR with libplacebo
  (Vulkan, understands Dolby Vision), OpenCL (GPU) or zscale (CPU, capped at 1080p)
- HEVC is copied into fragmented MP4 HLS for clients that play it (HDR10, HLG and
  Dolby Vision 8.1 with an HDR screen), so only the audio is re-encoded
- Dolby Vision profile 8.1 plays as HDR10 on HDR screens without transcoding
- Embedded text subtitles, audio track switching, resume and watched state
- Admin dashboard with active streams and activity log
- Modules, themes and skins you install and uninstall at runtime from a
  registry ([docs/modules.md](docs/modules.md)); your own themes as JSON files

See [docs/plan.md](docs/plan.md) for the roadmap and
[docs/decisions.md](docs/decisions.md) for the architecture choices.

## Android TV app

A native Android TV client lives in [`clients/android`](clients/android/README.md). It direct-plays whatever your TV can decode (including Dolby Vision and HDR) and falls back to the server's transcoding. The README there explains how to sideload it on an NVIDIA Shield or other Android TV device.

## Development

You need Rust (stable), Node 22 with pnpm, and FFmpeg on your PATH.

```bash
# web app (rebuild whenever the UI changes; the server embeds web/dist)
cd web && pnpm install && pnpm build && cd ..

# server on http://localhost:8420, data in ./dev-config
KINORA_CONFIG_DIR=./dev-config cargo run -p kinora-server

# UI with hot reload on http://localhost:5173 (talks to the server above)
cd web && pnpm dev
```

Test media: `scripts/seed-test-media.sh` fills `~/test-media` (or a folder you pass)
with about 500 fake movies, shows and anime episodes with real titles. They're hard
links to one tiny video, so the whole folder stays under a megabyte. Add
`Movies`, `Shows` and `Anime` as libraries. `--clean` removes only what it added.

Tests:

```bash
cargo test --workspace     # Rust: pure logic, snapshot tests, API tests
cd web && pnpm test        # web: Vitest unit tests + every Storybook story in headless Chromium
cd web && pnpm storybook   # web components on phones, tablets and TVs: see docs/storybook.md
```

### Layout

```
crates/core/        pure logic, no I/O: file name parsing, ffprobe parsing,
                    the playback decision engine, ffmpeg command builder
crates/plugin-api/  extension points shared by modules (metadata, themes, events)
crates/server/      the binary: HTTP API (axum), SQLite (sqlx), scanner,
                    transcoder, hardware detection, module host
web/                React + Tailwind web app (Vite)
packaging/          Unraid template
docs/               plan, decisions, theme, app skin, Storybook and code quality guides
```

Most behaviour lives in `crates/core` as plain functions, so it's tested
without FFmpeg, files or a network. The ffmpeg commands for every GPU backend
are snapshot-tested in `crates/core/src/snapshots/`: if a change alters a
command, `cargo insta review` shows the diff.

## Contributing

After cloning, run `pnpm install` in the repo root once. That sets up a git hook that formats staged web files with
Prettier and checks `cargo fmt` and `cargo clippy` when Rust files change. `pnpm format` formats everything.

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org), checked by a `commit-msg` hook
and in CI for every pull request:

```text
feat(web): add a library icon picker
fix(server): stop transcodes when the client disconnects
chore(deps): bump vite to 7.1
```

The type is one of `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore` or `revert`. The
scope is optional; use a crate name (`core`, `server`, `plugin-api`), `web`, or an area like `docker`, `docs`, `ci` or
`deps`. Add `!` after the scope for a breaking change. Run `pnpm lint:commits` to check your branch, and
`git commit --no-verify` to skip the hook once.

Every push and pull request also runs SonarQube Cloud and free linters (Oxlint, Knip, cargo-deny, actionlint,
ShellCheck, Hadolint). What each one checks, how to run them locally and the one-time SonarQube setup are in
[docs/code-quality.md](docs/code-quality.md).

## License

Kinora is released under the [GNU General Public License, version 2](LICENSE),
the same license as the Jellyfin server. FFmpeg is not linked into the binary;
it runs as a separate program under its own licence.
