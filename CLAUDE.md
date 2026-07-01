# CLAUDE.md

Guide for Claude Code (and any agent) working in this repository.

## What this project is

`mssql-localdb-mcp` — an MCP (Model Context Protocol) server written in Rust that gives AI agents full access to Microsoft SQL Server Express LocalDB on Windows: instance management, T-SQL script execution, discovery of loose databases in project folders, schema introspection.

Reference documents (read before any structural change):
- `docs/PLANNING.md` — roadmap, phases, scope of each milestone.
- `docs/ARCHITECTURE.md` — modules, data flow, technical decisions and why.
- `docs/MCP_SPEC.md` — exact contract of every tool/resource/prompt exposed over MCP (source of truth for names, input/output schemas).
- `docs/SECURITY.md` — threat model and mandatory guardrails.

If one of these documents diverges from the code, the code is wrong (or the doc is stale — update the doc in the same PR).

## Non-negotiable rules

1. **Platform**: Windows only. Don't add cross-platform cfg "just in case" — LocalDB doesn't exist outside Windows. `#[cfg(windows)]` is implicit across the whole project, no need to spread the annotation around.
2. **stdout is sacred**: the MCP stdio transport uses stdout for the JSON-RPC protocol. No `println!`, log, or subprocess output can leak to stdout. Logging always goes through `tracing` configured for stderr.
3. **No SQL Auth**: Windows Integrated Authentication only. Don't add a password field to any tool or config. This is a deliberate security decision, not a gap.
4. **Destructive guardrail is mandatory**: any tool running DDL/DML classified as destructive (`security::classify`) needs a `confirm: true` field in its input. Don't remove that check "to simplify". See `docs/SECURITY.md`.
5. **Folder scan is allowlist-restricted**: `db_scan_folder` never scans outside the roots configured in `config.toml`. Don't add a recursive scan of all of `C:\` or a fallback of "scan everything if unconfigured".
6. **rmcp is the official SDK**: don't swap it for a homegrown MCP protocol implementation or another third-party crate.
7. **Everything in English**: this is a public/published project — code, comments, docs, commit messages, and everything protocol-facing (tool descriptions, error messages) are all in English, regardless of the language used in conversation with the user. Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, etc.).

## Code conventions

- Edition 2024, `cargo fmt` + `cargo clippy --all-targets -- -D warnings` clean before any commit.
- Errors: `thiserror` for domain error types, `anyhow` only in `main.rs`/process boundaries.
- Every function that calls `SqlLocalDB.exe` or runs SQL should be testable without a real instance where possible (output parsing isolated from process execution).
- No obvious comments. Only comment when it explains a non-obvious why (LocalDB version workaround, tiberius limitation, etc.).
- Don't add generic abstraction/config beyond what the current roadmap phase asks for.

## Tests

- Real integration tests (`tests/`) create and destroy a temporary LocalDB instance — never reuse the user's instance.
- Running `cargo test` locally requires LocalDB installed (default on a Windows dev machine with Visual Studio or SQL Server tools).
- CI runs on `windows-latest`; confirm LocalDB availability on the runner before assuming it's present (see `docs/PLANNING.md`, CI section).

## Before opening a PR / publishing a release

- Update `docs/MCP_SPEC.md` if any tool/resource/prompt changed name, schema, or behavior.
- Run `cargo audit` and `cargo deny check`.
- A semver tag (`vX.Y.Z`) triggers automatic publish to the MCP Registry — don't create a tag manually without reviewing `server.json`.
