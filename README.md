# RCPA

RCPA is a Rust LLM gateway with a local admin UI, request logging, usage analytics, API key isolation, and SQLite-backed persistence.

## Protocols

Supported request protocols are:

- `completions`
- `responses`
- `messages`

## Quick Start

Start the server:

```bash
cargo run -- --token local-admin-token --port 15000 --data-dir ~/.rcpa --log-level info
```

On first start, RCPA creates `~/.rcpa/config.yaml`, `~/.rcpa/rcpa.db`, and `~/.rcpa/logs/`.

The admin UI is served from `frontend/dist`, so build the frontend before running a packaged binary:

```bash
cd frontend
npm ci
npm run build
```

## Config Model

Platform runtime settings are CLI-only:

- `--token`: admin UI/API token
- `--port`: HTTP listen port
- `--data-dir`: runtime directory containing `config.yaml`, `rcpa.db`, and `logs/`
- `--log-level`: process log level

Gateway settings live in `config.yaml`:

- `upstream.timeout_secs` is the global timeout applied to every upstream provider request.
- `providers[]` is the provider-level object. `name`, `api_key`, `models[]`, `priority`, `status`, and `headers` are shared for that provider.
- `providers[].endpoints[]` defines upstream operation endpoints. Each endpoint declares exactly one `protocol` and one `base_url`.
- `providers[].endpoints[].base_url` is sent to the upstream as-is.
- `providers[].endpoints[].protocol` is operation-level: use `completions` for `/v1/chat/completions`; the other values are `responses`, `messages`, and `embeddings`.
- `providers[].endpoints[].protocol` values must be unique within a provider.
- `providers[].models[].name` is the real upstream model name.
- `providers[].models[].aliases` are the public model names exposed by RCPA.
- Provider `priority` controls tiered routing: lower values are preferred first. Providers with the same `priority` are load-balanced round-robin. Degraded fallback also prefers lower priority values.
- `keys[]` defines client API keys, allowed model rules, optional `allowed_providers`, and private `model_aliases`.
- API keys use `model_aliases` for private aliases that point to an existing public model name.
- Key `name` is shown in logs and dashboard views; if omitted, the key id is used.
- Requests must carry `model`; there is no config-driven default model fallback.

Existing config files are validated as-is and are never silently repaired. Platform fields such as `server`, `admin`, and `database` are not valid in `config.yaml`.

When upgrading from a provider-level timeout configuration, remove every
`providers[].timeout_secs` field and add one required global setting:

```yaml
upstream:
  timeout_secs: 300
```

This is a breaking configuration and management API change in `v0.0.11`:

- Existing YAML is not migrated automatically; startup fails until the global
  `upstream` section is present and all provider-level timeout fields are removed.
- `POST /v1/admin/providers` rejects `timeout_secs`, and provider responses no
  longer include that field.
- All providers now share the same timeout; per-provider timeout overrides are
  intentionally unsupported.

There is no database migration and no change to the public LLM protocol routes.

## Database

SQLite is the default storage backend. Schema changes are versioned through migrations. If the schema is out of date or migration order is invalid, startup fails until the database is migrated explicitly.

Request and response bodies use a fixed retention policy: successful request bodies are cleared after 24 hours and failed request bodies after 7 days. Cleanup runs daily at 03:00 Asia/Shanghai in batches of 500; request log rows, metadata, token usage, cost, latency, and analytics metrics are retained. After cleanup, storage maintenance truncates the WAL and runs `VACUUM` when bodies were cleared or freelist pages are available, then truncates the WAL again so reclaimed space is returned to the filesystem.

## Docker

Build and run:

```bash
docker build -t rcpa:local .
docker run --rm -p 15000:15000 \
  -v "$PWD/data:/data" \
  rcpa:local --token local-admin-token --data-dir /data --port 15000
```

The image stores runtime files in `/data`: `/data/config.yaml`, `/data/rcpa.db`, and `/data/logs/`. If `/data/config.yaml` is missing, RCPA creates it automatically.

For Docker Compose, create `.env` from `.env.example`, set a strong `RCPA_ADMIN_TOKEN`, then run:

```bash
docker compose up -d
```

The included `docker-compose.yaml` pulls `ghcr.io/xxingwd/rcpa:latest`, exposes port `15000`, and stores runtime data in `./rcpa`.

## Release Layout

Binary release archives should include:

- `rcpa` binary
- `frontend/dist`
- `config.example.yaml`
- `docker-compose.yaml`
- `.env.example`
- `README.md`
- `CONTRIBUTING.md`
- `LICENSE`

Run the binary from the archive root so the server can find `frontend/dist`.

## Development

```bash
cargo check
cargo test
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cd frontend && npm ci && npm run lint && npm run build
```

Do not commit real `config.yaml`, `.env` files, SQLite databases, logs, or generated build directories.
