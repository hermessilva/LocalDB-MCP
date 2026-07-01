# Planning — mssql-localdb-mcp

## 1. Vision

Rust MCP server, single binary, Windows-only, that gives an AI agent full control over SQL Server Express LocalDB: instance management, execution of any T-SQL script/command supported by LocalDB, and discovery of loose database files in project folders (common scenario: several folders with `.mdf`/`.ldf` not attached to any instance).

### Why it exists
Today's MCP servers for SQL Server (Python, .NET, Node) assume a remote/already-configured connection and require an external runtime. None of them are: (a) self-contained native Rust, (b) focused on the full LocalDB lifecycle (create instance → find loose database → attach → run script), (c) published on the official MCP Registry as a standalone binary.

### Non-goals (out of scope for v1)
- Full SQL Server support (on-prem/Azure) — LocalDB only.
- SQL Authentication — Windows Integrated only.
- Linux/macOS — LocalDB doesn't exist there.
- GUI — MCP server only (stdio protocol).

## 2. Tech stack (summary — detail in ARCHITECTURE.md)

| Area | Choice | Reason |
|---|---|---|
| MCP SDK | `rmcp` (official, modelcontextprotocol/rust-sdk) | Official, maintained SDK, ~2.0 on crates.io |
| SQL driver | `tiberius` | Mature async TDS driver in Rust |
| DB transport channel | Named pipe (`tokio::net::windows::named_pipe`) | LocalDB uses a named pipe by default, not TCP |
| Instance management | `SqlLocalDB.exe` wrapper via `std::process::Command` | More stable across versions than direct FFI into `SQLUserInstance.dll` |
| Folder scan | `walkdir` | Simple, tested, enough for the expected volume |
| SQL risk classification | keyword/regex now, `sqlparser-rs` later | Real AST is more reliable than regex |
| Async runtime | `tokio` | Ecosystem standard, required by tiberius |
| Logging | `tracing` → stderr | stdout reserved for the MCP protocol |

## 3. Phases

### Phase 1 — MVP (v0.1.0)
Goal: agent can find a database in a folder, attach it, and run SQL against it.

- [x] Project skeleton (`Cargo.toml`, modules, `main.rs` with working stdio transport, basic MCP handshake)
- [x] `localdb::` wrapper: `list_instances`, `versions`, `info`, `create_instance`, `start_instance`, `stop_instance`, `delete_instance`
- [x] `sql::` client: connect via named pipe, `execute_script` (split on `GO`), `execute_query` (SELECT), `execute_statement` (with confirmation guard)
- [x] `discovery::` folder scan (`db_scan_folder`) restricted to config allowlist
- [x] `db_attach` / `db_detach`
- [x] `db_list_tables` (minimal introspection via `INFORMATION_SCHEMA`)
- [x] `config::` — TOML at `%APPDATA%\mssql-localdb-mcp\config.toml`, mandatory folder allowlist
- [x] `security::classify` — basic (regex + keywords) destructive-statement classifier
- [ ] Automated integration tests (`cargo test --test integration`): create/destroy temporary instance, attach/detach, execute_script — manually validated via a real MCP handshake (see history), but not yet an automated CI suite
- [x] README with manual install instructions (build) and Claude Desktop/Code config

Exit criterion: run locally against real LocalDB, all MVP tools working via `mcp-inspector` or Claude Desktop. **Met** — manually validated against a real instance and against a real project database (~75MB `.mdf`/`.ldf`), including the full scan → attach → introspect → detach cycle. Two bugs found and fixed in the process (see `CHANGELOG.md`).

### Phase 2 — Full surface
- [ ] Rest of instance management: `share`/`unshare`, `trace`
- [ ] `sql_begin_transaction`/`commit`/`rollback` (stateful session)
- [ ] `sql_bulk_insert` (tiberius native bulk load)
- [ ] `db_backup` / `db_restore`
- [ ] `db_list_columns`, `db_list_indexes`, `db_list_procedures`, `db_describe_object`
- [ ] `db_get_info`, `db_list_attached`
- [ ] MCP resources: `localdb://instances`, `localdb://{instance}/databases/{db}/schema`
- [ ] MCP prompts: `generate-migration-script`, `explain-schema`, `write-safe-delete`
- [ ] Audit log (`.jsonl`) of every execution
- [ ] Migrate the risk classifier from regex to `sqlparser-rs` (real AST)

Exit criterion: `docs/MCP_SPEC.md` covers 100% of the implemented surface; no tool left "TODO".

### Phase 3 — Publishing
- [ ] Code signing (Authenticode) of the release binary — **deferred**, no certificate available; v0.1.0 ships unsigned (user sees SmartScreen on first run). See backlog.
- [x] `mcpb` packaging (`mcpb/manifest.json` + binary) — assembled in CI, see `.github/workflows/release.yml`
- [x] `server.json` at repo root, namespace `io.github.hermessilva/mssql-localdb-mcp`
- [x] GitHub Actions: `ci.yml` (fmt/clippy/test/build on push/PR) + `release.yml` (build + package mcpb + GitHub Release + publish to the MCP Registry via `mcp-publisher` + GitHub OIDC, triggered on tag `vX.Y.Z`)
- [x] Community docs: complete `README.md`, `CONTRIBUTING.md`, `LICENSE`, config examples (Claude Desktop, Claude Code)
- [x] `CHANGELOG.md`
- [x] `scripts/prepare-release.ps1` — version bump + local build + packages `.mcpb` for a smoke test before tagging

Exit criterion: `io.github.hermessilva/mssql-localdb-mcp` installable via the official MCP Registry, signed binary, no blocking SmartScreen warning. **Partially met** — infra ready, `v0.1.0` GitHub Release published with the `.mcpb` asset; MCP Registry publish step is being retried after fixing two dry-run bugs (see backlog below). Code signing is still deferred to a future release.

### Phase 4 — Robustness
- [ ] MARS (Multiple Active Result Sets), if supported by tiberius/LocalDB
- [ ] Optional opt-in telemetry (never on by default)
- [ ] Real integration CI against LocalDB on the `windows-latest` runner
- [ ] `cargo audit` + `cargo deny` in the pipeline
- [ ] Benchmark for scanning a large folder (thousands of `.mdf`), parallelize with `rayon` if needed

## 4. Pending decisions backlog (review before each phase)

- `sqlcmd` variable support (`:setvar`, `$(VAR)`) in `sql_execute_script` — not supported in v1, evaluate real demand before adding.
- Whether `SQLUserInstance.dll` (native API) is worth replacing the CLI wrapper — only revisit if the CLI wrapper shows a real limitation (fragile parsing, performance).
- Exact pagination format for `sql_execute_query` (`OFFSET/FETCH` vs `TOP` vs cursor) — decided: `max_rows` with simple truncation (v1), no real cursor/pagination.
- Code signing (Authenticode): no certificate available as of 2026-07-01. v0.1.0 published unsigned. Revisit once a certificate is available (EV reduces SmartScreen friction faster than OV) — budget for this before any marketing push for the project, since an unsigned binary's SmartScreen warning scares off non-technical users.
- `.github/workflows/release.yml` needed two rounds of dry-run fixes on the actual v0.1.0 tag before a clean run: a ZIP path-separator bug (`Compress-Archive` on Windows writes `\` into zip entry names instead of the spec-required `/`, breaking extraction on non-Windows/JS zip readers) and a `server.json` validation failure (`description` over the registry's 100-character limit). Both are fixed as of the v0.1.0 publish. Keep an eye on the `mcp-publisher` asset name (`mcp-publisher_windows_amd64.tar.gz` containing `mcp-publisher.exe`, confirmed 2026-07-01) in case the upstream repo changes its release convention.

## 5. CI

- `windows-latest` runner: confirm LocalDB is present before assuming so (may require `winget install Microsoft.SQLServer.2022.LocalDB` or similar in the workflow). **Still unconfirmed** — `ci.yml`/`release.yml` run `cargo test`/`cargo build`, and both have now run successfully on a real GitHub runner, but there are no LocalDB-dependent integration tests yet (current tests are pure unit tests), so this hasn't actually been exercised.
- Pipeline: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (unit), `cargo test --test integration` (requires LocalDB), `cargo audit`.

## 6. Project success metric

- MVP functional, locally validated through real use (dogfooding) before any publishing.
- Zero external runtime dependency for the end user (just the binary + LocalDB already installed, which is a natural prerequisite).
- Listed on the official MCP Registry, installable in one command/config by popular MCP clients (Claude Desktop, Claude Code).
