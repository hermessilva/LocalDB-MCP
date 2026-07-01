# Architecture — mssql-localdb-mcp

## 1. Process overview

```
MCP client (Claude Desktop/Code, etc.)
        │  stdio (JSON-RPC over stdin/stdout)
        ▼
┌───────────────────────────────────────────────┐
│  mssql-localdb-mcp.exe                         │
│                                                 │
│  main.rs ── bootstrap tracing(stderr), config  │
│  mcp/     ── rmcp handlers (tools/resources/prompts)
│  localdb/ ── SqlLocalDB.exe wrapper            │
│  sql/     ── tiberius client (named pipe)      │
│  discovery/ ── folder scan (.mdf/.ldf)         │
│  security/ ── risk classifier, allowlist       │
│  config/  ── TOML + env                        │
└───────────────────────────────────────────────┘
        │                          │
        ▼                          ▼
  SqlLocalDB.exe (subprocess)   named pipe → sqlservr.exe (LocalDB instance)
```

The process is **stateless between tool calls** except for the SQL session (Phase 2, transactions). Each tool call opens its own connection and is an independent JSON-RPC request coming from the MCP client — there is no connection pool in v1 (see decisions table below).

## 2. Modules

### `main.rs`
Bootstrap: initializes `tracing` pointed at stderr, loads `config::Config`, builds the `rmcp` server with stdio transport, registers handlers, runs the loop.

Critical rule: **nothing writes to stdout** except `rmcp` itself via the transport. No `println!`, and no default log from the `SqlLocalDB.exe` subprocess may leak (subprocess stdout/stderr is captured and redirected to `tracing`, never passed through directly).

### `mcp/`
One handler per tool/resource/prompt, following the exact contract in `docs/MCP_SPEC.md`. Each handler:
1. Validates input (schema already guaranteed by `rmcp` via derive; extra semantic validation happens here — e.g. path within the allowlist).
2. Calls the corresponding domain module (`localdb::`, `sql::`, `discovery::`).
3. Maps the domain error to an MCP error (`McpError`) with a message useful to the agent, never a raw stack trace.

### `localdb/`
Thin wrapper over `SqlLocalDB.exe`:
- `Command::new("SqlLocalDB.exe").arg(...)`, captures stdout/stderr, text parsing (the CLI's output is fixed English text on most versions — OS localization/language is tracked as a known risk).
- Functions: `list_instances`, `versions`, `info(name)`, `create(name, version)`, `start(name)`, `stop(name, kill: bool)`, `delete(name)`, `ensure_running(name)`. `share`/`unshare`/`trace` are Phase 2.
- `info()` returns a parsed struct (name, version, state, named pipe, owner, auto-create, shared) — this is where `sql::` gets the pipe name to connect to.
- Parsing errors are isolated from process-execution errors (testable without a real `SqlLocalDB.exe`, using captured-output fixtures).

### `sql/`
- `connect(pipe_name, database)`: opens a `NamedPipeClient` (tokio) on the pipe returned by `localdb::info`/`start`, hands it to `tiberius::Client::connect`. Every call opens a fresh connection — no pool in v1; that's a deliberate simplicity trade-off for the MVP, not a correctness gap (see architecture decisions below).
- `execute_script(pipe_name, database, script, confirm) -> Vec<BatchResult>`: splits on a `GO` line (case-insensitive, only counts as a separator when it's the whole line), runs each batch sequentially on the same connection (so session `SET` state persists across batches like it does in SSMS), aggregates rowcount/error per batch.
- `execute_query(pipe_name, database, sql, max_rows) -> QueryResult`: only accepts a statement classified as read-only (see `security::classify`), serializes rows to JSON preserving types (datetime, decimal, varbinary → base64, etc.).
- `execute_statement(pipe_name, database, sql, confirm)`: calls `security::classify` first; if destructive and `confirm != true`, returns an MCP error asking for explicit confirmation (does not execute).

### `discovery/`
- `scan_folder(root: &Path, max_depth: usize, attached_paths: &HashSet<PathBuf>) -> Vec<FoundDatabase>`: assumes `root` is already validated against the config allowlist by the caller (`security::validate_path`), walks with `walkdir` up to a configurable max depth, filters `.mdf`/`.ldf`, cross-checks against `attached_paths` to mark `already_attached: bool`. Inaccessible entries (permission denied, etc.) are skipped, not fatal — see decisions below.
- No symlink following by default (avoids loops).

### `security/`
- `classify(sql: &str) -> RiskLevel` (`ReadOnly | Destructive`): v1 via regex/keywords (`DROP`, `TRUNCATE`, `ALTER`, `DELETE`, `UPDATE`, `INSERT`, `CREATE`, etc., with `CREATE DATABASE ... FOR ATTACH` special-cased as read-only since it's additive); Phase 2 migrates to `sqlparser-rs` (real AST, eliminates regex false negatives).
- `validate_path(path: &Path, allowlist: &[PathBuf]) -> Result<PathBuf>`: canonicalizes both sides and checks prefix against the allowlist.
- `AuditLog`: append-only `.jsonl` under `%APPDATA%\mssql-localdb-mcp\audit\`, one record per execution (timestamp, tool, SQL hash, summarized result). Phase 2.

### `config/`
- `Config` loaded from `%APPDATA%\mssql-localdb-mcp\config.toml`, with env var override (`MSSQL_LOCALDB_MCP_*`).
- v1 minimal fields: `scan_allowlist: Vec<PathBuf>` (required, empty = `db_scan_folder` always errors explaining it needs configuring), `default_query_timeout_secs`, `default_max_rows`, `log_level`, `scan_max_depth`.

## 3. Technical decisions and why

| Decision | Alternative considered | Why this choice |
|---|---|---|
| `SqlLocalDB.exe` CLI wrapper instead of `SQLUserInstance.dll` FFI | Direct bind via the `windows` crate + `msoledbsql.h` header | The CLI is stable across LocalDB versions and publicly documented; FFI requires keeping the binding in sync with ABI changes that aren't publicly documented as a stable API |
| Named pipe instead of TCP | Force LocalDB to expose TCP | LocalDB uses a named pipe by default; forcing TCP requires an extra config step on the user's side, breaking "works out of the box" |
| Keyword/regex now, `sqlparser-rs` later for risk classification | Regex only, forever | Regex has false negatives (e.g. `DELETE` inside a comment/string escapes `WHERE` detection); an AST is correct by construction. v1 accepts regex as a bridge, Phase 2 migrates |
| No connection pool in v1 | Pool per instance+database | A fresh connection per call is simpler and correct; pooling is a performance optimization to add once real usage shows it matters, not a v1 requirement |
| stdio as the only v1 transport | HTTP/SSE | Desktop MCP clients (Claude Desktop/Code) use stdio as the default for a local server; HTTP would add unnecessary network surface for a server that only makes sense locally |
| No SQL Auth | Support username/password | LocalDB is always local, Windows Integrated is the native model; adding a password is secret-leak surface with no real functional gain |

## 4. Data flow — example `db_scan_folder` → `db_attach` → `sql_execute_query`

1. Agent calls `db_scan_folder({ root: "D:\\Projects\\Client" })`.
2. `mcp/` validates via `security::validate_path` against `config.scan_allowlist`.
3. `discovery::scan_folder` walks the tree, finds an unattached `client.mdf`, returns an item with path and metadata.
4. Agent calls `db_attach({ instance: "MSSQLLocalDB", mdf_path: "D:\\Projects\\Client\\client.mdf" })`.
5. `mcp/` calls `localdb::ensure_running("MSSQLLocalDB")` for the pipe name (starts the instance if it isn't running), opens a `sql::` connection, runs `CREATE DATABASE ... FOR ATTACH`.
6. Agent calls `sql_execute_query({ instance: "MSSQLLocalDB", database: "client", sql: "SELECT TOP 10 * FROM Orders" })`.
7. `security::classify` confirms `ReadOnly`, `sql::execute_query` runs, serializes the result, returns JSON to the agent.

## 5. What NOT to do (known pitfalls)

- Don't open a new connection per line of `execute_script` — one connection, multiple sequential batches in the same session (needed for `GO` to behave the way SSMS expects, including session variables like `SET` persisting across batches).
- Don't blindly trust `SqlLocalDB.exe` output in any language — if the OS is set to a non-English locale, messages may come back translated; use `info` in a way that's tolerant of that, or pin the subprocess's `LANG`/`chcp` if possible.
- Don't leave `db_scan_folder` without a depth limit — a project folder can have a giant `node_modules` mixed in, needs a configurable max depth and a basic ignore-list (`node_modules`, `.git`, `bin`, `obj`).
