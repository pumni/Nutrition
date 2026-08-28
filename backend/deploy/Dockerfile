FROM rust:1.98.0-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922 AS builder

WORKDIR /workspace
COPY . .
RUN cargo build --release --locked --manifest-path backend/Cargo.toml -p api-http -p worker

FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241 AS runtime-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
ENV RUST_LOG=info
USER 10001:10001

FROM runtime-base AS api-http-runtime

COPY --from=builder /workspace/backend/target/release/api-http /app/api-http
ENV APP_BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/app/api-http"]

FROM runtime-base AS worker-runtime

COPY --from=builder /workspace/backend/target/release/worker /app/worker
ENTRYPOINT ["/app/worker"]
