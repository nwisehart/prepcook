# syntax=docker/dockerfile:1.6
# The board service image: the `remote` binary only. The board UI is served by
# the native Prepcook server, so no frontend is built here.

ARG APP_NAME=remote

FROM rust:1.93-slim-bookworm AS builder
ARG APP_NAME
ARG POSTHOG_API_KEY=""
ARG POSTHOG_API_ENDPOINT=""
ARG SENTRY_DSN_REMOTE=""

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_TARGET_DIR=/app/target
ENV POSTHOG_API_KEY=${POSTHOG_API_KEY}
ENV POSTHOG_API_ENDPOINT=${POSTHOG_API_ENDPOINT}
ENV SENTRY_DSN_REMOTE=${SENTRY_DSN_REMOTE}

RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates git \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY rust-toolchain.toml ./
RUN cargo --version >/dev/null
COPY Cargo.toml Cargo.lock ./
# The remote crate is its own cargo workspace but shares workspace-level
# dependency declarations with the root; copying the root manifests and the
# crates it depends on keeps the dependency graph resolvable.
COPY crates/api-types crates/api-types
COPY crates/remote crates/remote
COPY crates/utils crates/utils
COPY assets assets

RUN mkdir -p /app/bin

RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=remote-target,target=/app/target \
    cargo build --locked --release --manifest-path crates/remote/Cargo.toml \
 && cp /app/target/release/${APP_NAME} /app/bin/${APP_NAME}

FROM debian:bookworm-slim AS runtime
ARG APP_NAME

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates libssl3 wget \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --create-home --uid 10001 appuser

WORKDIR /srv

COPY --from=builder /app/bin/${APP_NAME} /usr/local/bin/${APP_NAME}

USER appuser

ENV SERVER_LISTEN_ADDR=0.0.0.0:8081 \
    RUST_LOG=info

EXPOSE 8081

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["wget","--spider","-q","http://127.0.0.1:8081/v1/health"]

ENTRYPOINT ["/usr/local/bin/remote"]
