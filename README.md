# Estafeta

A unified platform notification service. Estafeta consolidates notifications from
dozens of systems — email, codes, messages, calendar, and more — into a single
gRPC-based Rust service backed by NATS JetStream and PostgreSQL.

Estafeta provides a single notification bus that any system can publish to and any
client can subscribe to, enabling a unified notification widget across every
application in your platform.

## Architecture

```
Producers ──gRPC──▶ Estafeta ──▶ NATS JetStream (durable ingestion)
                                        │
                         ┌──────────────┴──────────────┐
                         ▼                              ▼
                  Processor Consumer              Delivery Consumers
                  (persist to PG,                 (email, push, sms,
                   resolve prefs,                  webhook — one per
                   fan-out to NATS Core)           channel)
                         │
                         ▼
                  NATS Core rt.user.{uid}
                         │
                         ▼
                  gRPC Server-Stream ──▶ Connected Clients
```

All Estafeta instances are **stateless**. Multiple instances form JetStream pull
consumer groups for horizontal scaling. Real-time fan-out uses NATS Core pub/sub
so every instance that has a connected user receives the event.

## Features

- **Unified ingestion** — any service publishes notifications via gRPC with
  schema-validated payloads
- **Schema registry** — services register JSON schemas for their notification
  types at runtime; payloads are validated on ingestion
- **Per-service notification levels** — each service defines its own severity
  levels (critical, warning, info, etc.)
- **Per-user preferences** — users configure which services, types, and channels
  they want, with minimum severity thresholds and mute rules
- **Notification lifecycle** — read/unread, snooze with timed wake-up,
  dismiss, TTL-based expiration, priority escalation
- **Multi-channel delivery** — email (SMTP/SES), push (FCM/APNs), SMS,
  webhooks with HMAC signing, each with independent retry and backoff
- **Real-time streaming** — gRPC server-streaming backed by NATS Core pub/sub
  with per-service filtering
- **Runtime configuration** — admins manage services, schemas, and global
  policies; users manage their own preferences — all at runtime via gRPC
- **JWT authentication** — validates tokens via Ory Hydra JWKS with
  authorization checks via Ory Keto
- **Multi-tenancy ready** — nullable `tenant_id` on every table

## Quick Start

### Prerequisites

- Rust 1.85+
- Docker and Docker Compose
- `protoc` (Protocol Buffers compiler)

### Start infrastructure

```bash
docker compose up -d
```

This starts PostgreSQL, NATS (with JetStream), and the Ory stack (Hydra, Kratos, Keto).

### Build and run

```bash
# Set required environment variables
export ESTAFETA_DATABASE_URL="postgres://estafeta:estafeta@localhost:5432/estafeta"
export ESTAFETA_NATS_URL="nats://localhost:4222"
export ESTAFETA_HYDRA_JWKS_URL="http://localhost:4444/.well-known/jwks.json"
export ESTAFETA_KETO_URL="http://localhost:4466"

# Build
cargo build --release

# Run (migrations are applied automatically on startup)
cargo run --release --bin estafeta
```

The gRPC server starts on port `50051` by default.

### Run tests

```bash
# Unit tests (no infrastructure needed)
cargo test --lib -p estafeta-server

# Integration tests (requires Docker)
cargo test -p estafeta-server
```

## Configuration

All configuration is via environment variables prefixed with `ESTAFETA_`.
Nested configs use `__` as separator.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ESTAFETA_DATABASE_URL` | yes | — | PostgreSQL connection string |
| `ESTAFETA_NATS_URL` | yes | — | NATS server URL |
| `ESTAFETA_HYDRA_JWKS_URL` | yes | — | Ory Hydra JWKS endpoint |
| `ESTAFETA_KETO_URL` | yes | — | Ory Keto read API URL |
| `ESTAFETA_GRPC_PORT` | no | `50051` | gRPC listen port |
| `ESTAFETA_DATABASE_MAX_CONNECTIONS` | no | `10` | PG connection pool size |
| `ESTAFETA_JWT_ISSUER` | no | — | Expected JWT issuer claim |
| `ESTAFETA_LOG_LEVEL` | no | `info` | Log level filter |
| `ESTAFETA_SMTP__HOST` | no | — | SMTP server host |
| `ESTAFETA_SMTP__PORT` | no | `587` | SMTP server port |
| `ESTAFETA_SMTP__USERNAME` | no | — | SMTP auth username |
| `ESTAFETA_SMTP__PASSWORD` | no | — | SMTP auth password |
| `ESTAFETA_SMTP__FROM_ADDRESS` | no | — | Sender email address |

See [docs/configuration.md](docs/configuration.md) for the full reference.

## gRPC API

Estafeta exposes five gRPC services:

| Service | Purpose |
|---------|---------|
| `AdminService` | Register/manage producer services, global policies |
| `SchemaRegistryService` | Register notification types with JSON schemas, severity levels |
| `NotificationService` | Send, query, and manage notification lifecycle |
| `UserConfigService` | Per-user preferences, mute rules, device registration, channel config |
| `StreamingService` | Real-time server-streaming of notification events |

Proto definitions are in [`proto/estafeta/v1/`](proto/estafeta/v1/).
See [docs/api.md](docs/api.md) for the full API reference.

## Project Structure

```
estafeta/
├── proto/estafeta/v1/           # Protobuf service definitions
├── crates/
│   ├── estafeta-proto/          # Generated gRPC/protobuf code
│   ├── estafeta-server/         # Main service binary and library
│   │   └── src/
│   │       ├── auth/            # JWT validation, Ory Keto authorization
│   │       ├── cache/           # In-process moka caches
│   │       ├── db/              # PostgreSQL query layer
│   │       ├── delivery/        # Multi-channel delivery (email, push, SMS, webhook)
│   │       ├── grpc/            # gRPC service implementations
│   │       ├── lifecycle/       # State machine, background scheduler
│   │       ├── nats/            # JetStream setup, publishing, message types
│   │       └── processing/      # Notification processor, preference resolver, schema validation
│   └── estafeta-migrations/     # SQL migrations
├── docs/                        # Documentation
├── docker-compose.yml           # Development infrastructure
└── Dockerfile                   # Production container image
```

## Documentation

- [Architecture](docs/architecture.md) — system design, data flow, key decisions
- [API Reference](docs/api.md) — all gRPC services and RPCs
- [Configuration](docs/configuration.md) — environment variables, delivery channels
- [Deployment](docs/deployment.md) — running in production, scaling, observability
- [Schema Registry](docs/schema-registry.md) — registering notification types and schemas
- [Notification Lifecycle](docs/lifecycle.md) — states, transitions, snooze, escalation
- [Delivery Channels](docs/delivery.md) — email, push, SMS, webhook configuration

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
