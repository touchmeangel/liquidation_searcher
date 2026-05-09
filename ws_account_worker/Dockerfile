FROM rustlang/rust:nightly-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./

COPY connections/Cargo.toml ./connections/Cargo.toml
COPY protocols/Cargo.toml ./protocols/Cargo.toml
COPY add_worker/Cargo.toml ./add_worker/Cargo.toml
COPY worker/Cargo.toml ./worker/Cargo.toml
COPY searcher/Cargo.toml ./searcher/Cargo.toml
COPY rem_worker/Cargo.toml ./rem_worker/Cargo.toml
COPY check_worker/Cargo.toml ./check_worker/Cargo.toml
COPY sync/Cargo.toml ./sync/Cargo.toml
COPY ws_subscription_worker/Cargo.toml ./ws_subscription_worker/Cargo.toml
COPY ws_account_worker/Cargo.toml ./ws_account_worker/Cargo.toml

RUN mkdir -p \
    connections/src \
    protocols/src \
    add_worker/src \
    worker/src \
    searcher/src \
    rem_worker/src \
    check_worker/src \
    sync/src \
    ws_subscription_worker/src \
    ws_account_worker/src && \
  touch \
    connections/src/lib.rs \
    protocols/src/lib.rs \
    add_worker/src/main.rs \
    worker/src/main.rs \
    searcher/src/main.rs \
    rem_worker/src/main.rs \
    check_worker/src/main.rs \
    sync/src/main.rs \
    ws_subscription_worker/src/main.rs \
    ws_account_worker/src/main.rs

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS deps
ARG PACKAGE=ws_account_worker

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --package ${PACKAGE} --recipe-path recipe.json

FROM deps AS builder
ARG PACKAGE=ws_account_worker

COPY Cargo.toml Cargo.lock ./
COPY connections ./connections
COPY protocols ./protocols
COPY add_worker ./add_worker
COPY worker ./worker
COPY searcher ./searcher
COPY rem_worker ./rem_worker
COPY check_worker ./check_worker
COPY sync ./sync
COPY ws_subscription_worker ./ws_subscription_worker
COPY ws_account_worker ./ws_account_worker

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --package ${PACKAGE} && \
    cp /app/target/release/${PACKAGE} /app/binary

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /app/binary /usr/local/bin/app

USER nonroot

CMD ["/usr/local/bin/app"]