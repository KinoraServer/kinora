# syntax=docker/dockerfile:1.7
#
# Kinora: one image for amd64 and arm64.
#   1. build the web app (Node)
#   2. build the server with the web app embedded (Rust)
#   3. small Debian runtime with FFmpeg and GPU drivers

# ---- 1. web app -------------------------------------------------------------
FROM node:22-bookworm-slim AS web
WORKDIR /src/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
# The app skin rules and starter skins live with the server and are shared with the web app.
COPY crates/plugin-api/skin-rules.json /src/crates/plugin-api/skin-rules.json
COPY crates/server/src/modules/app_skin_samples /src/crates/server/src/modules/app_skin_samples
RUN pnpm build

# ---- 2. server ----------------------------------------------------------------
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web /src/web/dist ./web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p kinora-server \
 && cp target/release/kinora /kinora

# ---- 3. runtime -----------------------------------------------------------------
FROM debian:trixie-slim
ARG TARGETARCH

# FFmpeg from Debian is built with NVENC/NVDEC, Quick Sync (oneVPL), VAAPI,
# Vulkan + libplacebo (Dolby Vision tone mapping) and zimg.
# Drivers: Mesa for AMD (VAAPI + Vulkan) and Intel Vulkan; Intel media driver
# + oneVPL runtime for Quick Sync on amd64. NVIDIA drivers are injected by the
# NVIDIA container runtime at start.
RUN sed -i 's/^Components: main$/Components: main contrib non-free non-free-firmware/' /etc/apt/sources.list.d/debian.sources \
 && apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates ffmpeg mesa-va-drivers mesa-vulkan-drivers libvulkan1 \
 && if [ "$TARGETARCH" = "amd64" ]; then \
      apt-get install -y --no-install-recommends intel-media-va-driver-non-free libvpl2 libmfx-gen1.2 ; \
    fi \
 && rm -rf /var/lib/apt/lists/*

COPY --from=server /kinora /usr/local/bin/kinora

ENV KINORA_CONFIG_DIR=/config \
    KINORA_TRANSCODE_DIR=/transcode \
    KINORA_PORT=8420 \
    NVIDIA_VISIBLE_DEVICES=all \
    NVIDIA_DRIVER_CAPABILITIES=compute,video,utility,graphics

VOLUME ["/config"]
EXPOSE 8420
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD bash -c 'exec 3<>/dev/tcp/127.0.0.1/${KINORA_PORT}' || exit 1
ENTRYPOINT ["/usr/local/bin/kinora"]
