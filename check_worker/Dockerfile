FROM rustlang/rust:nightly-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY check_worker ./check_worker
COPY worker ./worker
COPY rem_worker ./rem_worker
COPY add_worker ./add_worker
COPY ws_account_worker ./ws_account_worker
COPY ws_subscription_worker ./ws_subscription_worker
COPY sync ./sync
COPY searcher ./searcher
COPY connections ./connections
COPY protocols ./protocols

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo build --release --package check_worker

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 appuser

COPY --from=builder /app/target/release/check_worker /usr/local/bin/check_worker

USER appuser

CMD ["check_worker"]