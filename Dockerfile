FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/estafeta-proto/Cargo.toml crates/estafeta-proto/
COPY crates/estafeta-server/Cargo.toml crates/estafeta-server/
COPY crates/estafeta-migrations/Cargo.toml crates/estafeta-migrations/

# Create dummy source files to build dependencies
RUN mkdir -p crates/estafeta-proto/src && echo "" > crates/estafeta-proto/src/lib.rs && \
    mkdir -p crates/estafeta-server/src && echo "fn main() {}" > crates/estafeta-server/src/main.rs && \
    mkdir -p crates/estafeta-migrations/src && echo "fn main() {}" > crates/estafeta-migrations/src/main.rs

# Create dummy build.rs
RUN echo "fn main() {}" > crates/estafeta-proto/build.rs

# Build dependencies only
RUN cargo build --release 2>/dev/null || true

# Copy actual source
COPY proto/ proto/
COPY crates/ crates/

# Touch source files to invalidate cache
RUN touch crates/estafeta-proto/src/lib.rs crates/estafeta-server/src/main.rs

# Build the actual binary
RUN cargo build --release --bin estafeta

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/estafeta /usr/local/bin/estafeta

ENV ESTAFETA_GRPC_PORT=50051
EXPOSE 50051

ENTRYPOINT ["estafeta"]
