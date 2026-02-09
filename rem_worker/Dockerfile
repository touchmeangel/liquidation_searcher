FROM rustlang/rust:nightly-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY rem_worker ./rem_worker
COPY worker ./worker
COPY add_worker ./add_worker
COPY searcher ./searcher
COPY scheduler ./scheduler
COPY connections ./connections
COPY protocols ./protocols

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo build --release --package rem_worker

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 appuser

COPY --from=builder /app/target/release/rem_worker /usr/local/bin/rem_worker

USER appuser

CMD ["rem_worker"]