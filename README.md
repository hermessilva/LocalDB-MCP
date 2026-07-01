# mssql-localdb-mcp

Rust [MCP](https://modelcontextprotocol.io) server for SQL Server Express LocalDB on Windows. Gives AI agents (Claude Desktop, Claude Code, and any MCP client) full control over LocalDB instances: create/manage instances, run any T-SQL script, find loose database files in project folders and attach them, introspect schema.

> Status: MVP (Phase 1) functional and tested against real LocalDB — see `docs/PLANNING.md` for the roadmap and current phase.

## Why

Existing MCP tools for SQL Server assume an already-configured remote server and require an external runtime (Python, .NET, Node). This project is:
- **Single Rust binary**, no external runtime, no Docker.
- **LocalDB-focused**: local dev workflow — find a loose `.mdf` in a project folder, attach it, run a script, no need to open SSMS.
- **Windows-only by design**, not a generic multi-database abstraction.

## Requirements

- Windows with SQL Server Express LocalDB installed (`SqlLocalDB.exe` on `PATH`).
- A compatible MCP client (Claude Desktop, Claude Code, etc.).

## Installation

### Via MCP Registry

Published on the [official MCP Registry](https://registry.modelcontextprotocol.io) as `io.github.hermessilva/mssql-localdb-mcp`. Install through any MCP client that supports the registry.

### Via Claude Code plugin

The [`claude-plugin/`](claude-plugin/) directory in this repo is a self-contained Claude Code plugin (bundles the Windows binary directly, no separate download). Load it directly with:

```powershell
claude --plugin-dir path\to\LocalDB-MCP\claude-plugin
```

or install it from the [official Claude Code plugin marketplace](https://github.com/anthropics/claude-plugins-official) once listed there.

### From source

Requires [Rust](https://rustup.rs) and LocalDB installed.

```powershell
git clone https://github.com/hermessilva/LocalDB-MCP.git
cd LocalDB-MCP
cargo build --release
```

Binary at `target\release\mssql-localdb-mcp.exe`.

### Configure `config.toml`

`db_scan_folder` requires at least one explicitly allowed root — without it, the tool refuses to run. Create `%APPDATA%\mssql-localdb-mcp\config.toml`:

```toml
# Windows paths in TOML need single quotes (literal string) — double
# quotes interpret \U... as a unicode escape and break parsing.
scan_allowlist = ['C:\Users\YourUser\source\repos']
scan_max_depth = 6
default_query_timeout_secs = 30
default_max_rows = 1000
```

### Configure in your MCP client

**Claude Desktop / Claude Code** (`claude_desktop_config.json` or equivalent):

```json
{
  "mcpServers": {
    "mssql-localdb": {
      "command": "C:\\path\\to\\LocalDB-MCP\\target\\release\\mssql-localdb-mcp.exe"
    }
  }
}
```

**Claude Code via CLI:**

```powershell
claude mcp add mssql-localdb -- "C:\path\to\LocalDB-MCP\target\release\mssql-localdb-mcp.exe"
```

After that, the agent has access to the `localdb_*`, `sql_*`, and `db_*` tools — see [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) for the full list.

## Documentation

- [`docs/PLANNING.md`](docs/PLANNING.md) — roadmap, phases, scope of each milestone.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — modules, technical decisions, data flow.
- [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) — exact contract of every tool/resource/prompt exposed.
- [`docs/SECURITY.md`](docs/SECURITY.md) — threat model and guardrails.
- [`CLAUDE.md`](CLAUDE.md) — guide for AI agents working in this repository.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to contribute.
- [`CHANGELOG.md`](CHANGELOG.md) — change history.

## Security — read before using

- Windows Integrated Authentication only (no SQL Auth).
- Every destructive action (DROP, TRUNCATE, DELETE, ALTER, etc.) requires explicit confirmation (`confirm: true`) — never executes silently.
- Database discovery (`db_scan_folder`) only scans folders explicitly allowed in `config.toml` (allowlist).
- Full detail in [`docs/SECURITY.md`](docs/SECURITY.md).

## License

[MIT](LICENSE).
