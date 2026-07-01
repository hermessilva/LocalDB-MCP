# Contributing

## Requirements

- Windows with SQL Server Express LocalDB installed (`SqlLocalDB.exe` on `PATH`).
- Stable Rust (edition 2024) via [rustup](https://rustup.rs).

## Build and test

```powershell
cargo build
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Integration tests create and destroy real temporary LocalDB instances — no mocks. Running `cargo test` locally requires LocalDB installed.

## Before opening a PR

- `cargo fmt`, `cargo clippy -D warnings`, and `cargo test` clean.
- If you changed a tool/resource/prompt's name, schema, or behavior, update [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) in the same PR — it's the source of truth for the exposed contract.
- If you changed the threat model or a security guardrail, update [`docs/SECURITY.md`](docs/SECURITY.md).
- Read [`CLAUDE.md`](CLAUDE.md) — the project's non-negotiable rules (stdout reserved for the MCP protocol, no SQL Auth, mandatory confirmation guard on destructive actions, allowlist-restricted scan).

## Project structure

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the modules and technical decisions (and the reasoning behind them). See [`docs/PLANNING.md`](docs/PLANNING.md) for the roadmap and which phase each feature is in.

## Reporting bugs / suggesting features

Open an [issue](https://github.com/hermessilva/LocalDB-MCP/issues). For a bug, include: LocalDB version (`SqlLocalDB.exe versions`), the MCP tool/command used, and the full error message.

## Security

Don't open a public issue for a security vulnerability. See [`docs/SECURITY.md`](docs/SECURITY.md) for the threat model — if you find something outside what's already documented as an accepted risk, report it privately (e.g. via the repository's GitHub Security Advisory).
