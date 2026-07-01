# MSSQL LocalDB MCP — Claude Code plugin

Bundles the [mssql-localdb-mcp](https://github.com/hermessilva/LocalDB-MCP) MCP server as a Claude Code plugin: instance management, T-SQL execution, and discovery of loose `.mdf`/`.ldf` files for SQL Server Express LocalDB on Windows.

This directory is the plugin root referenced by the marketplace entry (`source.path`). It bundles the compiled Windows binary directly — no separate download step, no network access required to start the server.

- Platform: Windows only (LocalDB doesn't exist elsewhere).
- Security model, tool list, and full documentation: see the [main repository](https://github.com/hermessilva/LocalDB-MCP), especially [`docs/SECURITY.md`](https://github.com/hermessilva/LocalDB-MCP/blob/master/docs/SECURITY.md) and [`docs/MCP_SPEC.md`](https://github.com/hermessilva/LocalDB-MCP/blob/master/docs/MCP_SPEC.md).
- Requires: SQL Server Express LocalDB installed, and a `config.toml` with `scan_allowlist` set for folder-discovery tools to work — see the main README.

## Updating the bundled binary

The binary in `server/mssql-localdb-mcp.exe` is rebuilt and re-committed here on each release. It's built from the same source as the tagged release with the matching version in `.claude-plugin/plugin.json`.
