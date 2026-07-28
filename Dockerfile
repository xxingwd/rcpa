FROM rust:1-bookworm AS backend-build
WORKDIR /app

COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY tests ./tests
COPY migrations ./migrations
COPY static ./static
COPY config.example.yaml ./

RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app

ENV TZ=Asia/Shanghai

RUN apt-get update \
  && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates tzdata \
  && rm -rf /var/lib/apt/lists/*

COPY --from=backend-build /app/target/release/rcpa /app/rcpa
COPY config.example.yaml /app/config.example.yaml

RUN mkdir -p /data

VOLUME ["/data"]

EXPOSE 15000

ENTRYPOINT ["/app/rcpa"]
CMD ["--data-dir", "/data", "--port", "15000", "--log-level", "info"]
