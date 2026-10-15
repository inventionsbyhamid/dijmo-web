# syntax=docker/dockerfile:1
#
# dijmo-web: build with cargo-chef caching, run on distroless as non-root.
# Coolify builds this on every push to main.

ARG RUST_VERSION=1.90

# ---- chef: the toolchain plus cargo-chef -------------------------------------
FROM rust:${RUST_VERSION}-slim-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /app

# ---- planner: a recipe of the dependencies only -------------------------------
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

# ---- builder: dependencies (cached until Cargo.lock changes), then the app ---
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates crates
# Embedded at compile time: migrations (sqlx::migrate!) and templates (Askama).
COPY migrations migrations
COPY templates templates
RUN cargo build --release --locked --bin dijmo-web \
    && cp target/release/dijmo-web /dijmo-web

# ---- runtime: glibc + CA roots, no shell, uid 65532 ---------------------------
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=builder /dijmo-web /app/dijmo-web
COPY assets /app/assets

# PROXY_HOPS=1: Coolify's proxy (Traefik/Caddy) sits in front and appends the
# client address to X-Forwarded-For. Set 2 if Cloudflare (proxied) is in front too.
ENV PORT=8080 \
    ASSETS_DIR=/app/assets \
    LOG_FORMAT=json \
    RUST_LOG=info,sqlx=warn \
    PROXY_HOPS=1

EXPOSE 8080
USER nonroot:nonroot

HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=3 \
    CMD ["/app/dijmo-web", "--healthcheck"]

# The server stops gracefully on SIGTERM.
STOPSIGNAL SIGTERM
ENTRYPOINT ["/app/dijmo-web"]
