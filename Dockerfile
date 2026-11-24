# syntax=docker/dockerfile:1

# --- UI ---------------------------------------------------------------------
FROM node:22-bookworm-slim AS ui

RUN corepack enable && corepack prepare pnpm@12.23.0 --activate
WORKDIR /app/ui

COPY ui/package.json ui/pnpm-lock.yaml ui/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile

COPY ui/ ./
RUN pnpm build

# --- Server ------------------------------------------------------------------
FROM rust:1-bookworm AS server

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release -p lima-server

# --- Runtime -----------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=server /app/target/release/lima-server /usr/local/bin/lima-server
COPY --from=ui /app/ui/dist ./ui/dist

ENV LIMA_SERVER_PORT=6767 \
    LIMA_UI_DIR=/app/ui/dist \
    LIMA_LIBRARY_DIR=/library \
    LIMA_STATE_DIR=/state

EXPOSE 6767
VOLUME ["/library", "/state"]

ENTRYPOINT ["lima-server"]
