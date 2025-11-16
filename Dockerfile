FROM rust:1.82 as builder

WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
COPY src ./src
COPY tests ./tests

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/lowdown /usr/local/bin/lowdown

EXPOSE 8080 7070
ENV RUST_LOG=info

ENTRYPOINT ["/usr/local/bin/lowdown"]
