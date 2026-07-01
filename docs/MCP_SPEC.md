# MCP Spec — mssql-localdb-mcp

Source of truth for the contract exposed by the server. Any change to a tool/resource/prompt's name, input/output schema, or behavior must update this document in the same PR.

General conventions:
- Every error returned to the client follows `McpError { code, message, data? }`. `message` is always actionable text for the agent (never a raw stack trace).
- Every tool referencing an instance accepts `instance: string`; if omitted, the default is `MSSQLLocalDB` (the default instance created by the LocalDB installer).
- Every tool that executes SQL accepts `timeout_secs?: number` (defaults to `config.default_query_timeout_secs`).
- `[Phase 1]` / `[Phase 2]` indicates which milestone implements the tool (see `docs/PLANNING.md`).

---

## Tools

### Group A — Instance management

#### `localdb_list_instances` `[Phase 1]`
Lists all LocalDB instances registered on the machine.
- **Input**: `{}`
- **Output**: `{ instances: [{ name: string, is_default: bool }] }`

#### `localdb_versions` `[Phase 1]`
Lists installed LocalDB versions.
- **Input**: `{}`
- **Output**: `{ versions: string[] }`

#### `localdb_info` `[Phase 1]`
Detail of an instance.
- **Input**: `{ instance: string }`
- **Output**: `{ name: string, version: string, state: "Running"|"Stopped", pipe_name: string | null, owner: string, auto_create: bool, shared: bool, shared_name: string | null, last_start_time: string | null }`

#### `localdb_create_instance` `[Phase 1]`
- **Input**: `{ instance: string, version?: string, start?: bool }`
- **Output**: `{ created: bool, instance: string }`

#### `localdb_start_instance` `[Phase 1]`
- **Input**: `{ instance: string }`
- **Output**: `{ started: bool, pipe_name: string }`

#### `localdb_stop_instance` `[Phase 1]`
- **Input**: `{ instance: string, kill?: bool }` (`kill` forces immediate shutdown)
- **Output**: `{ stopped: bool }`

#### `localdb_delete_instance` `[Phase 1]`
Destructive (removes the instance, not the attached database files).
- **Input**: `{ instance: string, confirm: bool }`
- **Output**: `{ deleted: bool }`
- **Guard**: `CONFIRMATION_REQUIRED` error if `confirm != true`.

#### `localdb_share_instance` `[Phase 2]`
- **Input**: `{ instance: string, shared_name: string }`
- **Output**: `{ shared: bool }`

#### `localdb_unshare_instance` `[Phase 2]`
- **Input**: `{ instance: string }`
- **Output**: `{ unshared: bool }`

#### `localdb_trace` `[Phase 2]`
- **Input**: `{ instance: string, enabled: bool }`
- **Output**: `{ trace_enabled: bool }`

---

### Group B — Script/SQL channel

#### `sql_execute_script` `[Phase 1]`
Runs a multi-batch T-SQL script (`GO` on its own line as separator), on the same session/connection.
- **Input**: `{ instance: string, database: string, script: string, confirm?: bool }`
- **Output**: `{ batches: [{ index: number, messages: string[], rows_affected: number | null, error: string | null }] }`
- **Guard**: if any batch contains a `Destructive` statement (see `security::classify`) and `confirm != true`, no batch executes; returns `CONFIRMATION_REQUIRED` listing the identified destructive batches (by index).

#### `sql_execute_query` `[Phase 1]`
Only accepts statements classified as `ReadOnly`.
- **Input**: `{ instance: string, database: string, sql: string, max_rows?: number }`
- **Output**: `{ columns: [{ name: string, type: string }], rows: any[][], truncated: bool }`
- **Guard**: `NOT_READ_ONLY` error if `security::classify(sql) != ReadOnly`.

#### `sql_execute_statement` `[Phase 1]`
Single DML/DDL statement.
- **Input**: `{ instance: string, database: string, sql: string, confirm?: bool }`
- **Output**: `{ rows_affected: number | null, messages: string[] }`
- **Guard**: same as `sql_execute_script`, but for a single statement.

#### `sql_begin_transaction` `[Phase 2]`
- **Input**: `{ instance: string, database: string }`
- **Output**: `{ transaction_id: string }`

#### `sql_commit` `[Phase 2]`
- **Input**: `{ transaction_id: string }`
- **Output**: `{ committed: bool }`

#### `sql_rollback` `[Phase 2]`
- **Input**: `{ transaction_id: string }`
- **Output**: `{ rolled_back: bool }`

#### `sql_bulk_insert` `[Phase 2]`
- **Input**: `{ instance: string, database: string, table: string, columns: string[], rows: any[][] }`
- **Output**: `{ rows_inserted: number }`

---

### Group C — Discovery and metadata

#### `db_scan_folder` `[Phase 1]`
Scans a folder for loose `.mdf`/`.ldf` files. Restricted to the `config.toml` allowlist.
- **Input**: `{ root: string, max_depth?: number }`
- **Output**: `{ found: [{ path: string, kind: "mdf"|"ldf", size_bytes: number, modified_at: string, already_attached: bool, likely_database_name: string | null }] }`
- **Guard**: `PATH_NOT_ALLOWED` error if `root` is outside `config.scan_allowlist` (after canonicalization).

#### `db_list_attached` `[Phase 2]`
- **Input**: `{ instance: string }`
- **Output**: `{ databases: [{ name: string, state: string, size_mb: number }] }`

#### `db_get_info` `[Phase 2]`
- **Input**: `{ instance: string, database: string }`
- **Output**: `{ name: string, compatibility_level: number, recovery_model: string, files: [{ logical_name: string, physical_path: string, size_mb: number, type: "ROWS"|"LOG" }] }`

#### `db_attach` `[Phase 1]`
- **Input**: `{ instance: string, mdf_path: string, database_name?: string, ldf_path?: string }`
- **Output**: `{ attached: bool, database: string }`
- **Guard**: `mdf_path` validated against `security::validate_path` (same allowlist as the scan).

#### `db_detach` `[Phase 1]`
- **Input**: `{ instance: string, database: string, confirm: bool }`
- **Output**: `{ detached: bool }`
- **Guard**: `CONFIRMATION_REQUIRED` error if `confirm != true`.

#### `db_backup` `[Phase 2]`
- **Input**: `{ instance: string, database: string, backup_path: string }`
- **Output**: `{ backed_up: bool, backup_path: string }`

#### `db_restore` `[Phase 2]`
- **Input**: `{ instance: string, database: string, backup_path: string, confirm: bool }`
- **Output**: `{ restored: bool }`
- **Guard**: destructive if `database` already exists (overwrite) — `confirm` mandatory in that case.

#### `db_list_tables` `[Phase 1]`
- **Input**: `{ instance: string, database: string, schema?: string }`
- **Output**: `{ tables: [{ schema: string, name: string, row_count_estimate: number | null }] }`

#### `db_list_columns` `[Phase 2]`
- **Input**: `{ instance: string, database: string, table: string, schema?: string }`
- **Output**: `{ columns: [{ name: string, type: string, nullable: bool, is_primary_key: bool, default: string | null }] }`

#### `db_list_indexes` `[Phase 2]`
- **Input**: `{ instance: string, database: string, table: string, schema?: string }`
- **Output**: `{ indexes: [{ name: string, is_unique: bool, is_primary_key: bool, columns: string[] }] }`

#### `db_list_procedures` `[Phase 2]`
- **Input**: `{ instance: string, database: string, schema?: string }`
- **Output**: `{ procedures: [{ schema: string, name: string, parameters: string[] }] }`

#### `db_describe_object` `[Phase 2]`
Generic introspection (table, view, procedure, function) — shortcut so callers don't need to call a specific tool.
- **Input**: `{ instance: string, database: string, object_name: string, schema?: string }`
- **Output**: `{ object_type: string, definition: string | null, columns: array | null, parameters: array | null }`

---

## Resources

### `localdb://instances` `[Phase 2]`
Live instance list (same shape as `localdb_list_instances`), subscribable — client gets notified when an instance changes state.

### `localdb://{instance}/databases/{database}/schema` `[Phase 2]`
Browsable database schema: tables, columns, indexes, procedures in a single JSON tree (equivalent to aggregating `db_list_tables` + `db_list_columns` + `db_list_indexes` + `db_list_procedures`).

---

## Prompts

### `generate-migration-script` `[Phase 2]`
Arguments: `{ instance: string, database: string, change_description: string }`. Generates a guided prompt that injects the current schema (via the resource above) and asks for a safe (idempotent, `IF NOT EXISTS`) migration script.

### `explain-schema` `[Phase 2]`
Arguments: `{ instance: string, database: string }`. Prompt that injects the full schema and asks for a natural-language explanation of the relationships between tables.

### `write-safe-delete` `[Phase 2]`
Arguments: `{ instance: string, database: string, table: string, intent: string }`. Prompt that forces the agent to first generate a `SELECT` equivalent to the intended `WHERE` before building the `DELETE`, reducing the risk of an accidental mass deletion.

---

## Standardized error codes

| Code | When |
|---|---|
| `CONFIRMATION_REQUIRED` | Destructive statement/action without `confirm: true` |
| `NOT_READ_ONLY` | `sql_execute_query` received a non-SELECT statement |
| `PATH_NOT_ALLOWED` | Path outside the `config.scan_allowlist` allowlist |
| `INSTANCE_NOT_FOUND` | Referenced instance doesn't exist |
| `INSTANCE_NOT_RUNNING` | Operation requires a running instance and it's stopped (tools without auto-start) |
| `DATABASE_NOT_FOUND` | Referenced database doesn't exist / isn't attached |
| `SQL_ERROR` | Error returned by SQL Server itself (original message preserved in `data`) |
| `CONFIG_MISSING` | E.g. empty `scan_allowlist` when calling `db_scan_folder` |
