---
title: "Deployment Guide"
description: "Building, running, and operating Estafeta in production."
updated_at: "2026-07-28"
---

# Deployment Guide

This document covers building, running, and operating Estafeta in production.

## Container Image

The project includes a multi-stage `Dockerfile` based on `rust:1.85-bookworm`:

```
docker build -t estafeta .
```

The build stage installs `protobuf-compiler`, copies manifests first for
dependency-layer caching, then compiles the release binary. The runtime stage
uses `debian:bookworm-slim` with only `ca-certificates` installed.

The entrypoint is the `estafeta` binary, which runs the gRPC server and all
background tasks in a single process. The default exposed port is 50051.

---

## Required Infrastructure

### PostgreSQL

Estafeta requires PostgreSQL 17 (or compatible). The server runs embedded
SQLx migrations on startup from `crates/estafeta-migrations/migrations/`, so
no separate migration step is needed.

### NATS with JetStream

NATS 2.11+ with JetStream enabled is required. The server creates one stream
and its consumer on startup:

| Stream | Subjects | Retention | Purpose |
|---|---|---|---|
| `NOTIFICATIONS` | `notif.ingest.>` | WorkQueue | Ingestion pipeline |

Consumer:

| Consumer | Stream | Filter Subject | Ack Wait |
|---|---|---|---|
| `processor` | NOTIFICATIONS | `notif.ingest.>` | 30s |

The consumer is durable with `max_deliver: 5`.

Real-time streaming uses NATS Core subjects (`rt.user.{user_id}`) for
low-latency pub/sub -- these do not require JetStream.

### Ory Stack

- **Hydra** (v2.3+) -- OAuth2/OIDC provider. Estafeta fetches the JWKS
  endpoint to validate JWTs.
- **Keto** (v0.13+) -- Relation-based access control. The read API
  (port 4466) is used for permission checks.
- **Kratos** (v1.3+) -- Identity management. Not directly consumed by
  Estafeta but assumed for user identity in the broader Sunbeam platform.

---

## Horizontal Scaling

Estafeta instances are stateless. You can run multiple replicas behind a gRPC
load balancer.

### JetStream Consumer Groups

The JetStream consumer is durable and uses WorkQueue retention. When multiple
instances connect to the same consumer, messages are distributed across
instances automatically. No leader election or coordination is needed.

### Lifecycle Scheduler

The background scheduler (snooze wake-up, TTL expiry, escalation) runs on
every instance but uses `SELECT FOR UPDATE SKIP LOCKED` to prevent duplicate
processing. It is safe to run on all replicas concurrently.

### Scaling Guidelines

Estafeta's resource profile is straightforward since all work happens in a
single process with no per-channel worker partitioning:

- **CPU**: Scale replicas based on gRPC request rate and processor throughput.
- **Memory**: The primary memory consumers are the in-process moka caches
  (up to 30,000 entries across three caches) and active gRPC streaming
  connections.
- **Database connections**: Size the pool so that
  `pool_size * replica_count < max_connections - headroom`.

---

## Health Checks and Readiness

The gRPC server starts accepting connections only after:

1. PostgreSQL connection pool is established
2. Database migrations are applied
3. NATS connection is established and JetStream stream/consumer is created
4. JWKS keys are fetched from Hydra

If any of these steps fail, the process exits with a non-zero code. For
container orchestrators, use a gRPC health probe or a TCP check on the gRPC
port as a liveness indicator.

---

## Observability

### Structured Logging

Estafeta uses `tracing` with `tracing-subscriber` configured for JSON output.
Every log line includes structured fields:

```json
{"timestamp":"...","level":"INFO","fields":{"message":"notification processed","notification_id":"...","recipient":"user123"},"target":"estafeta_server::processor"}
```

The log level is controlled by the `ESTAFETA_LOG_LEVEL` environment variable
(default: `info`). This is passed directly to `tracing_subscriber`'s
`EnvFilter`, so it supports per-module directives like
`estafeta_server::processor=debug,info`.

### Metrics

No metrics exporter is bundled yet. The NATS monitoring endpoint
(port 8222 in the development compose file) provides stream and consumer
statistics. PostgreSQL connection pool metrics are available through
standard `pg_stat_activity` queries.

---

## Database

### Connection Pool

The pool is managed by SQLx with a configurable maximum via
`ESTAFETA_DATABASE_MAX_CONNECTIONS` (default: 10). In production, size the
pool based on your replica count and PostgreSQL `max_connections`. A rule of
thumb: `(pool_size * replica_count) < max_connections - headroom`.

### Migrations

Migrations run automatically on startup via `sqlx::migrate!`. They are
embedded in the binary at compile time from
`crates/estafeta-migrations/migrations/`. There is no need to run a separate
migration tool in production -- just deploy the new binary and it will apply
pending migrations before accepting traffic.

For zero-downtime deploys, ensure migrations are backward-compatible
(additive columns with defaults, new tables) so old and new instances can
coexist.

---

## NATS

### Stream Retention

The NOTIFICATIONS stream uses `WorkQueue` retention. Messages are deleted
once the consumer has acknowledged them. This keeps storage bounded without
explicit TTL configuration.

### Storage

JetStream is configured with file-based storage. Provision persistent
volumes for the NATS data directory (`/data` in the compose file) to survive
container restarts.

### Consumer Configuration

- **ack_wait:** 30s for the processor. If a message is not acknowledged
  within this window it is redelivered.
- **max_deliver:** 5. After 5 redelivery attempts the message is dropped.

---

## Security

### TLS

The gRPC server does not terminate TLS itself. Place it behind a
TLS-terminating load balancer or sidecar proxy (Envoy, Istio, etc.) in
production.

### JWT Validation

Tokens are validated against RSA256 keys fetched from the Hydra JWKS
endpoint. Keys are cached in memory and refreshed every 30 minutes. An
on-demand refresh is triggered when an unknown `kid` is encountered.

An optional `ESTAFETA_JWT_ISSUER` can be set to enforce the `iss` claim.

### Keto Permissions

Authorization is checked per-RPC. The Keto read API is called over HTTP
(not gRPC). If Keto is unreachable, affected RPCs return `INTERNAL`.

---

## Docker Compose (Development)

The included `docker-compose.yml` provides a complete local environment:

| Service | Image | Ports |
|---|---|---|
| `postgres` | postgres:17 | 5432 |
| `nats` | nats:2.11 | 4222 (client), 8222 (monitoring) |
| `kratos` | oryd/kratos:v1.3 | 4433, 4434 |
| `hydra` | oryd/hydra:v2.3 | 4444, 4445 |
| `keto` | oryd/keto:v0.13 | 4466 (read), 4467 (write) |

Persistent volumes are used for PostgreSQL data (`pgdata`) and NATS
JetStream data (`natsdata`).

All Ory services share the same PostgreSQL instance with separate databases.
Kratos and Hydra depend on PostgreSQL being healthy before starting.

To start:

```
docker compose up -d
```

Then run the Estafeta server locally:

```
export ESTAFETA_DATABASE_URL=postgres://estafeta:estafeta@localhost/estafeta
export ESTAFETA_NATS_URL=nats://localhost:4222
export ESTAFETA_HYDRA_JWKS_URL=http://localhost:4444/.well-known/jwks.json
export ESTAFETA_KETO_URL=http://localhost:4466
cargo run --release --bin estafeta
```
