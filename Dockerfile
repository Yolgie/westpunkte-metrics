# syntax=docker/dockerfile:1.7

FROM rust:1.96-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin westpunkte-metrics && \
    cp /app/target/release/westpunkte-metrics /usr/local/bin/westpunkte-metrics

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /usr/local/bin/westpunkte-metrics /usr/local/bin/westpunkte-metrics
EXPOSE 9090
ENTRYPOINT ["/usr/local/bin/westpunkte-metrics"]
