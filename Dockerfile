FROM ghcr.io/nekoshinobi/rust-chef-sccache:main AS base
FROM base AS planner
WORKDIR /app
COPY . .
RUN --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef prepare --recipe-path recipe.json

FROM node:25-alpine AS frontend-builder
WORKDIR /app/ui
COPY ui/package.json ui/package-lock.json* ./
RUN npm ci
COPY ui/ .
RUN npm run build

FROM base AS builder
WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release

FROM ubuntu:26.04
WORKDIR /app

RUN DEBIAN_FRONTEND=noninteractive apt update && apt upgrade -y && apt install -y curl python3 ffmpeg && apt clean && rm -rf /var/lib/apt/lists/*
RUN curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/bin/yt-dlp && chmod a+rx /usr/bin/yt-dlp

COPY --from=builder /app/target/release/rust-discord-bot rust-discord-bot
COPY --from=frontend-builder /app/ui/build /app/static

CMD ["/app/rust-discord-bot"]
