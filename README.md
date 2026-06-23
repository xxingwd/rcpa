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
cargo run -- --log-level info
```

On first start, RCPA creates `~/.rcpa/config.yaml`, `~/.rcpa/rcpa.db`, and `~/.rcpa/logs/`.

The admin UI is served from `frontend/dist`, so build the frontend before running a packaged binary:

```bash
cd frontend
npm ci
npm run build
```

## Config Model

- `providers[].models[].name` is the real upstream model name.
- `providers[].models[].aliases` are the public model names exposed by RCPA.
- API keys use `model_aliases` for private aliases that point to an existing public model name.
- Key `name` is shown in logs and dashboard views; if omitted, the key id is used.

Environment placeholders such as `${RCPA_ADMIN_TOKEN}` must exist when the process starts. Missing variables fail startup. Existing config files are validated as-is and are never silently repaired.

## Database

SQLite is the default storage backend. Schema changes are versioned through migrations. If the schema is out of date or migration order is invalid, startup fails until the database is migrated explicitly.

## Docker

Build and run:

```bash
docker build -t rcpa:local .
docker run --rm -p 15000:15000 \
  -v "$PWD/data:/data" \
  rcpa:local
```

The image stores runtime files in `/data`: `/data/config.yaml`, `/data/rcpa.db`, and `/data/logs/`. If `/data/config.yaml` is missing, RCPA creates it automatically.

## Release Layout

Binary release archives should include:

- `rcpa` binary
- `frontend/dist`
- `config.example.yaml`
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
