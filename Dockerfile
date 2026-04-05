FROM rust:1.88-bookworm AS builder

WORKDIR /app

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Copy everything (rely on .dockerignore for exclusions)
COPY . .

# Build the binary
RUN cargo build --release --bin estafeta

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/estafeta /usr/local/bin/estafeta

ENV ESTAFETA_GRPC_PORT=50051
EXPOSE 50051

ENTRYPOINT ["estafeta"]
