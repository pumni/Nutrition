FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder

WORKDIR /workspace
COPY . .
RUN cargo build --release --locked --manifest-path backend/Cargo.toml -p api-http -p worker

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS runtime-base

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
