# Estafeta

> *Tem uma carta para si.*

A unified notification inbox for your entire platform. Estafeta consolidates
notifications from dozens of producer systems into a single, queryable inbox
consumed exclusively through gRPC query and streaming APIs. No outbound
email. No push. No SMS. Just one inbox that every client reads from.

Notification fatigue happens when every service rolls its own alerts, badges,
and delivery pipelines. Estafeta eliminates the problem at the source: every
notification lives in one place, with consistent state tracking, grouping, and
user-controlled preferences -- so users see exactly what matters and nothing
they have already dealt with.

## Architecture

```mermaid
graph LR
    Producers -->|gRPC| Estafeta
    Estafeta -->|publish| JS[NATS JetStream]
    JS --> Processor[Processor Consumer<br>validate, persist to PG,<br>resolve prefs,<br>fan-out to NATS Core]
    Processor --> PG[(PostgreSQL)]
    Processor --> NC[NATS Core<br>rt.user.uid]
    NC --> Stream[gRPC Server-Stream]
    Stream --> Clients[Connected Clients]
```

All Estafeta instances are **stateless**. Multiple instances form JetStream pull
consumer groups for horizontal scaling. Real-time fan-out uses NATS Core pub/sub
so every instance that has a connected user receives the event.

## Features

- **Unified inbox** -- any service publishes notifications via gRPC; clients
  consume them from a single inbox through query or streaming APIs
- **Three-state read tracking** -- notifications progress through
  **unseen** -> **unread** -> **read**. *Unseen* drives the bell badge count.
  *Unread* means the user has seen the notification in a dropdown but has not
  interacted with it. This distinction lets you build accurate badge counters
  and bold-title lists with one data model.
- **Additional states** -- **snoozed** (timed reappearance), **dismissed**
  (user-hidden), and **expired** (TTL elapsed) round out the lifecycle
- **Schema registry** -- services register JSON schemas for their notification
  types at runtime; payloads are validated on ingestion
- **Per-service notification levels** -- each service defines its own severity
  levels (critical, warning, info, etc.)
- **Per-user preferences** -- users configure which services, types, and
  severity thresholds they care about, plus mute rules, catch-up mode, and
  sort mode via `UserConfigService`
- **Deep linking** -- every notification can carry an `action_url` and an
  `icon` field, making it trivial to build rich, clickable notification UIs
- **Bulk actions** -- `DismissAllInGroup` lets users triage entire notification
  groups in one call
- **Escalation actions** -- `EscalationAction` can **resurface**, **bump**, or
  **elevate** a notification that has not been acted on within a configured
  window
- **Real-time streaming** -- gRPC server-streaming backed by NATS Core pub/sub
  with per-service filtering
- **Runtime configuration** -- admins manage services, schemas, and global
  policies; users manage their own preferences -- all at runtime via gRPC
- **JWT authentication** -- validates tokens via Ory Hydra JWKS with
  authorization checks via Ory Keto
- **Multi-tenancy ready** -- nullable `tenant_id` on every table

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
| `ESTAFETA_DATABASE_URL` | yes | -- | PostgreSQL connection string |
| `ESTAFETA_NATS_URL` | yes | -- | NATS server URL |
| `ESTAFETA_HYDRA_JWKS_URL` | yes | -- | Ory Hydra JWKS endpoint |
| `ESTAFETA_KETO_URL` | yes | -- | Ory Keto read API URL |
| `ESTAFETA_GRPC_PORT` | no | `50051` | gRPC listen port |
| `ESTAFETA_DATABASE_MAX_CONNECTIONS` | no | `10` | PG connection pool size |
| `ESTAFETA_JWT_ISSUER` | no | -- | Expected JWT issuer claim |
| `ESTAFETA_LOG_LEVEL` | no | `info` | Log level filter |

See [docs/configuration.md](docs/configuration.md) for the full reference.

## gRPC API

Estafeta exposes five gRPC services:

| Service | Purpose |
|---------|---------|
| `AdminService` | Register/manage producer services, global policies |
| `SchemaRegistryService` | Register notification types with JSON schemas, severity levels |
| `NotificationService` | Query, acknowledge, snooze, dismiss, and bulk-manage notifications |
| `UserConfigService` | Per-user preferences, mute rules, catch-up mode, sort mode |
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

- [Architecture](docs/architecture.md) -- system design, data flow, key decisions
- [API Reference](docs/api.md) -- all gRPC services and RPCs
- [Configuration](docs/configuration.md) -- environment variables and tuning
- [Deployment](docs/deployment.md) -- running in production, scaling, observability
- [Schema Registry](docs/schema-registry.md) -- registering notification types and schemas
- [Notification Lifecycle](docs/lifecycle.md) -- states, transitions, snooze, escalation

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
