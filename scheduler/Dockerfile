FROM rustlang/rust:nightly-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY scheduler ./scheduler
COPY worker ./worker
COPY add_worker ./add_worker
COPY rem_worker ./rem_worker
COPY searcher ./searcher
COPY connections ./connections
COPY protocols ./protocols

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo build --release --package scheduler

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 appuser

COPY --from=builder /app/target/release/scheduler /usr/local/bin/scheduler

USER appuser

CMD ["scheduler"]