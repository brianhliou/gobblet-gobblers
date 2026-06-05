# Gobblet Gobblers tablebase probe — single always-on container for Railway.
# gobblet-api (Rust/axum) loads the compact MPH tablebase (gobblet.ctb, ~531 MB)
# into RAM and answers POST /lookup/batch. The .ctb is git-ignored and fetched at
# build time from a public GitHub Release asset; override the URL with
#   --build-arg CTB_URL=...   (Railway: set a build arg) when the TB is re-cut.
#
# Build context must be this directory (v2/) so both crates are present.

# ---- build the probe ----
FROM rust:1-slim AS build
WORKDIR /src
COPY gobblet-core ./gobblet-core
COPY gobblet-api ./gobblet-api
RUN cd gobblet-api && cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/gobblet-api/target/release/gobblet-api /app/gobblet-api

ARG CTB_URL=https://github.com/brianhliou/gobblet-gobblers/releases/download/tb-v1/gobblet.ctb
ADD ${CTB_URL} /app/gobblet.ctb

ENV CTB_PATH=/app/gobblet.ctb
ENV PORT=8080
EXPOSE 8080
CMD ["/app/gobblet-api"]
