# Security model — mssql-localdb-mcp

## 1. Threat model

A local MCP server, running with the privileges of the Windows user who invoked it (the same session where the MCP client — Claude Desktop/Code — is running). There's no network authentication: the server trusts the MCP client that invoked it over stdio, like any local MCP server.

The main risks don't come from the network, they come from:
1. **AI agent generating destructive SQL without clear user intent** (hallucination, prompt injection via data read from a table, ambiguous instruction).
2. **Agent scanning/reading files outside the intended scope** via `db_scan_folder`/`db_attach`.
3. **Privilege escalation inside LocalDB itself**: LocalDB grants `sysadmin` to the user who creates the instance by default — the server inherits that level inside the database.

Out of scope (not something the server solves):
- Compromise of the user's machine (if an attacker already has local code execution, MCP isn't the relevant surface).
- Network attack — there's no network listener, transport is stdio.

## 2. Implemented guardrails

### 2.1 Explicit confirmation for destructive actions
`security::classify(sql: &str) -> RiskLevel`:
- `ReadOnly`: plain `SELECT` (no `INTO`), no side effects.
- `Destructive`: `DROP`, `TRUNCATE`, `ALTER`, `DELETE`, `UPDATE` without `WHERE`, `DELETE`/`UPDATE` with `WHERE` always present but still marked destructive (doesn't try to assess "how destructive" — any write DML/DDL is destructive by default), `CREATE DATABASE ... FOR ATTACH` is not destructive (it's additive), `DROP DATABASE` is.

Every tool that runs non-`SELECT` SQL requires `confirm: true` in its input when the classification result is `Destructive`. Without that flag, the tool returns `CONFIRMATION_REQUIRED` and **executes nothing** — not even partially.

v1 uses regex/keyword matching (fast to implement, covers the common cases). Known false negative: obfuscated/dynamic SQL inside a string (`EXEC('DELETE FROM...')`) can escape regex detection. Phase 2 migrates to `sqlparser-rs` (real AST parsing) specifically to close that gap — it's not a "nice to have", it's the reason for the migration.

This is **not a sandbox** — it's a friction brake. The agent (or the user through the agent) can still confirm and run anything. The goal is to prevent accidental execution, not to block deliberate malicious use by someone who already has legitimate access.

### 2.2 Folder scan allowlist
`config.scan_allowlist` is mandatory and empty by default — `db_scan_folder` refuses to run until the user explicitly configures which roots can be scanned. There's never a "scan everything if unconfigured" fallback.

`security::validate_path`:
- Canonicalizes the received path (resolves `..`, links).
- Checks that the result has some allowlist entry as a prefix.
- Rejects paths outside that with `PATH_NOT_ALLOWED`, without detailing the directory structure outside the allowlist in the error message (avoids leaking path information via the error message).

`db_attach` uses the same validation for `mdf_path` — you can't attach a database from outside the allowlist even if the path comes from another source (e.g. suggested by the agent itself).

The scan has a configurable max depth and ignores `node_modules`, `.git`, `bin`, `obj` by default (avoids cost and noise).

### 2.3 No SQL Authentication
The server only connects via Windows Integrated Authentication (LocalDB's native model). There's no password field in any tool, config, or log. This eliminates an entire class of risk (secret in a log, secret in a config file, secret in an error message).

### 2.4 Timeout and row limit by default
Every query has a timeout (`config.default_query_timeout_secs`), and `sql_execute_query` has a default `max_rows` — prevents the agent from locking up the instance with a `TOP`/`WHERE`-less query on a large table, and prevents blowing up the agent's context with a giant result (returns `truncated: true` when it cuts off).

### 2.5 Audit log (Phase 2)
Every `sql_execute_*` writes to `%APPDATA%\mssql-localdb-mcp\audit\*.jsonl`: timestamp, tool, instance, database, SHA-256 hash of the SQL (not the content — avoids duplicating sensitive data in the log), risk classification, summarized result (success/error, rows_affected). Serves post-hoc auditing, not real-time blocking.

### 2.6 Error messages without leakage
The `SQL_ERROR` error preserves the original SQL Server message in `data` (useful for the agent to fix the query), but never includes a Rust stack trace or the server's internal file paths. `PATH_NOT_ALLOWED` doesn't echo the full allowlist (avoids disk-structure reconnaissance by trial and error).

## 3. Accepted and documented risks (not solved by the server)

- **`sysadmin` by default on the instance the user creates**: native LocalDB behavior, not something the MCP server controls. Documented in the README as a known risk; a user who needs real least-privilege should manually create a specific login and use that context — out of scope for v1 (no support for impersonation/custom logins).
- **Prompt injection via data read from the database**: if a table contains malicious text that the agent reads via `sql_execute_query` and later interprets as an instruction, that's a model/agent risk, not something the MCP server can reliably filter. Partial mitigation: the `confirm` guardrail is still required for any subsequent destructive action, so the worst case still stops at confirmation.
- **Trust in the MCP client**: if the MCP client itself is compromised, it can simply always send `confirm: true`. The guardrail assumes an honest MCP client that surfaces the confirmation to the real user (that's how Claude Desktop/Code work) — it's not protection against a malicious client.

## 4. Security review checklist (run before every release)

- [ ] No new write/DDL tool bypasses `security::classify` + the `confirm` guard.
- [ ] No new path accepted by a tool bypasses `security::validate_path` where applicable.
- [ ] `cargo audit` clean (no known CVE in a dependency).
- [ ] No new log/error message leaks config content, out-of-scope paths, or a stack trace.
- [ ] `docs/SECURITY.md` updated if the threat model changed (e.g. if a network transport is ever supported, this document needs a whole new section).
