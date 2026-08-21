FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder

WORKDIR /workspace
COPY . .
RUN cargo build --release --locked -p api-http -p worker

FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241 AS runtime-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
ENV RUST_LOG=info
USER 10001:10001

FROM runtime-base AS api-http-runtime

COPY --from=builder /workspace/target/release/api-http /app/api-http
ENV APP_BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/app/api-http"]

FROM runtime-base AS worker-runtime

COPY --from=builder /workspace/target/release/worker /app/worker
ENTRYPOINT ["/app/worker"]
