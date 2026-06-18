# Contributing

Keep changes small, explicit, and covered by tests when they affect routing, auth, protocol handling, pricing, logging, or migrations.

## Local Checks

Run these before opening a pull request:

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cd frontend && npm ci && npm run lint && npm run build
```

## Rules

- Do not commit real `config.yaml`, `.env` files, SQLite databases, logs, or generated build outputs.
- Supported provider protocols are exactly `completions`, `responses`, and `messages`.
- Database schema changes must use explicit versioned migrations.
- Startup should fail on missing config, missing environment variables, invalid protocol names, invalid model aliases, or schema mismatch.
- Provider model aliases expose public names; API key model aliases must point to existing public names.
