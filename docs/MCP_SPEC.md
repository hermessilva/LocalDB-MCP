# MCP Spec — mssql-localdb-mcp

Fonte da verdade do contrato exposto pelo servidor. Qualquer mudança de nome, schema de input/output ou comportamento de uma tool/resource/prompt precisa atualizar este documento na mesma PR.

Convenções gerais:
- Todo erro retornado ao client segue `McpError { code, message, data? }`. `message` sempre em texto acionável pro agente (nunca stack trace cru).
- Toda tool que referencia uma instância aceita `instance: string`; se omitido, default é `MSSQLLocalDB` (instância padrão criada pela instalação do LocalDB).
- Toda tool que executa SQL aceita `timeout_secs?: number` (default de `config.default_query_timeout_secs`).
- `[Fase 1]` / `[Fase 2]` indica em qual milestone a tool é implementada (ver `docs/PLANNING.md`).

---

## Tools

### Grupo A — Gestão de instância

#### `localdb_list_instances` `[Fase 1]`
Lista todas as instâncias LocalDB registradas na máquina.
- **Input**: `{}`
- **Output**: `{ instances: [{ name: string, is_default: bool }] }`

#### `localdb_versions` `[Fase 1]`
Lista versões do LocalDB instaladas.
- **Input**: `{}`
- **Output**: `{ versions: string[] }`

#### `localdb_info` `[Fase 1]`
Detalhe de uma instância.
- **Input**: `{ instance: string }`
- **Output**: `{ name: string, version: string, state: "Running"|"Stopped", pipe_name: string | null, owner: string, auto_create: bool, shared: bool, shared_name: string | null, last_start_time: string | null }`

#### `localdb_create_instance` `[Fase 1]`
- **Input**: `{ instance: string, version?: string, start?: bool }`
- **Output**: `{ created: bool, instance: string }`

#### `localdb_start_instance` `[Fase 1]`
- **Input**: `{ instance: string }`
- **Output**: `{ started: bool, pipe_name: string }`

#### `localdb_stop_instance` `[Fase 1]`
- **Input**: `{ instance: string, kill?: bool }` (`kill` força encerramento imediato)
- **Output**: `{ stopped: bool }`

#### `localdb_delete_instance` `[Fase 1]`
Destrutivo (remove instância, não os arquivos de banco anexados).
- **Input**: `{ instance: string, confirm: bool }`
- **Output**: `{ deleted: bool }`
- **Guard**: erro `CONFIRMATION_REQUIRED` se `confirm != true`.

#### `localdb_share_instance` `[Fase 2]`
- **Input**: `{ instance: string, shared_name: string }`
- **Output**: `{ shared: bool }`

#### `localdb_unshare_instance` `[Fase 2]`
- **Input**: `{ instance: string }`
- **Output**: `{ unshared: bool }`

#### `localdb_trace` `[Fase 2]`
- **Input**: `{ instance: string, enabled: bool }`
- **Output**: `{ trace_enabled: bool }`

---

### Grupo B — Canal de script/SQL

#### `sql_execute_script` `[Fase 1]`
Roda script T-SQL multi-batch (separador `GO` em linha própria), na mesma sessão/conexão.
- **Input**: `{ instance: string, database: string, script: string, confirm?: bool }`
- **Output**: `{ batches: [{ index: number, messages: string[], rows_affected: number | null, error: string | null }] }`
- **Guard**: se qualquer batch contiver statement `Destructive` (ver `security::classify`) e `confirm != true`, nenhum batch executa; retorna erro `CONFIRMATION_REQUIRED` listando os batches destrutivos identificados (por índice).

#### `sql_execute_query` `[Fase 1]`
Só aceita statements classificados `ReadOnly`.
- **Input**: `{ instance: string, database: string, sql: string, max_rows?: number }`
- **Output**: `{ columns: [{ name: string, type: string }], rows: any[][], truncated: bool }`
- **Guard**: erro `NOT_READ_ONLY` se `security::classify(sql) != ReadOnly`.

#### `sql_execute_statement` `[Fase 1]`
Statement único DML/DDL.
- **Input**: `{ instance: string, database: string, sql: string, confirm?: bool }`
- **Output**: `{ rows_affected: number | null, messages: string[] }`
- **Guard**: igual a `sql_execute_script`, mas por statement único.

#### `sql_begin_transaction` `[Fase 2]`
- **Input**: `{ instance: string, database: string }`
- **Output**: `{ transaction_id: string }`

#### `sql_commit` `[Fase 2]`
- **Input**: `{ transaction_id: string }`
- **Output**: `{ committed: bool }`

#### `sql_rollback` `[Fase 2]`
- **Input**: `{ transaction_id: string }`
- **Output**: `{ rolled_back: bool }`

#### `sql_bulk_insert` `[Fase 2]`
- **Input**: `{ instance: string, database: string, table: string, columns: string[], rows: any[][] }`
- **Output**: `{ rows_inserted: number }`

---

### Grupo C — Descoberta e metadata

#### `db_scan_folder` `[Fase 1]`
Varre pasta em busca de `.mdf`/`.ldf` soltos. Restrito à allowlist de `config.toml`.
- **Input**: `{ root: string, max_depth?: number }`
- **Output**: `{ found: [{ path: string, kind: "mdf"|"ldf", size_bytes: number, modified_at: string, already_attached: bool, likely_database_name: string | null }] }`
- **Guard**: erro `PATH_NOT_ALLOWED` se `root` fora de `config.scan_allowlist` (canonicalizado).

#### `db_list_attached` `[Fase 2]`
- **Input**: `{ instance: string }`
- **Output**: `{ databases: [{ name: string, state: string, size_mb: number }] }`

#### `db_get_info` `[Fase 2]`
- **Input**: `{ instance: string, database: string }`
- **Output**: `{ name: string, compatibility_level: number, recovery_model: string, files: [{ logical_name: string, physical_path: string, size_mb: number, type: "ROWS"|"LOG" }] }`

#### `db_attach` `[Fase 1]`
- **Input**: `{ instance: string, mdf_path: string, database_name?: string, ldf_path?: string }`
- **Output**: `{ attached: bool, database: string }`
- **Guard**: `mdf_path` validado contra `security::validate_path` (mesma allowlist do scan).

#### `db_detach` `[Fase 1]`
- **Input**: `{ instance: string, database: string, confirm: bool }`
- **Output**: `{ detached: bool }`
- **Guard**: erro `CONFIRMATION_REQUIRED` se `confirm != true`.

#### `db_backup` `[Fase 2]`
- **Input**: `{ instance: string, database: string, backup_path: string }`
- **Output**: `{ backed_up: bool, backup_path: string }`

#### `db_restore` `[Fase 2]`
- **Input**: `{ instance: string, database: string, backup_path: string, confirm: bool }`
- **Output**: `{ restored: bool }`
- **Guard**: destrutivo se `database` já existe (sobrescreve) — `confirm` obrigatório nesse caso.

#### `db_list_tables` `[Fase 1]`
- **Input**: `{ instance: string, database: string, schema?: string }`
- **Output**: `{ tables: [{ schema: string, name: string, row_count_estimate: number | null }] }`

#### `db_list_columns` `[Fase 2]`
- **Input**: `{ instance: string, database: string, table: string, schema?: string }`
- **Output**: `{ columns: [{ name: string, type: string, nullable: bool, is_primary_key: bool, default: string | null }] }`

#### `db_list_indexes` `[Fase 2]`
- **Input**: `{ instance: string, database: string, table: string, schema?: string }`
- **Output**: `{ indexes: [{ name: string, is_unique: bool, is_primary_key: bool, columns: string[] }] }`

#### `db_list_procedures` `[Fase 2]`
- **Input**: `{ instance: string, database: string, schema?: string }`
- **Output**: `{ procedures: [{ schema: string, name: string, parameters: string[] }] }`

#### `db_describe_object` `[Fase 2]`
Introspecção genérica (tabela, view, procedure, function) — atalho pra não precisar chamar tool específica.
- **Input**: `{ instance: string, database: string, object_name: string, schema?: string }`
- **Output**: `{ object_type: string, definition: string | null, columns: array | null, parameters: array | null }`

---

## Resources

### `localdb://instances` `[Fase 2]`
Lista viva de instâncias (mesma forma de `localdb_list_instances`), subscribable — client recebe notificação quando instância muda de estado.

### `localdb://{instance}/databases/{database}/schema` `[Fase 2]`
Schema navegável do banco: tabelas, colunas, índices, procedures em uma árvore JSON única (equivalente a agregação de `db_list_tables` + `db_list_columns` + `db_list_indexes` + `db_list_procedures`).

---

## Prompts

### `generate-migration-script` `[Fase 2]`
Argumentos: `{ instance: string, database: string, change_description: string }`. Gera prompt guiado que injeta schema atual (via resource acima) e pede script de migração seguro (idempotente, com `IF NOT EXISTS`).

### `explain-schema` `[Fase 2]`
Argumentos: `{ instance: string, database: string }`. Prompt que injeta schema completo e pede explicação em linguagem natural das relações entre tabelas.

### `write-safe-delete` `[Fase 2]`
Argumentos: `{ instance: string, database: string, table: string, intent: string }`. Prompt que força o agente a gerar primeiro um `SELECT` equivalente ao `WHERE` pretendido antes de montar o `DELETE`, reduzindo risco de exclusão em massa acidental.

---

## Códigos de erro padronizados

| Código | Quando |
|---|---|
| `CONFIRMATION_REQUIRED` | Statement/ação destrutiva sem `confirm: true` |
| `NOT_READ_ONLY` | `sql_execute_query` recebeu statement não-SELECT |
| `PATH_NOT_ALLOWED` | Path fora da allowlist de `config.scan_allowlist` |
| `INSTANCE_NOT_FOUND` | Instância referenciada não existe |
| `INSTANCE_NOT_RUNNING` | Operação exige instância rodando e ela está parada (tools sem auto-start) |
| `DATABASE_NOT_FOUND` | Database referenciado não existe/não está anexado |
| `SQL_ERROR` | Erro retornado pelo próprio SQL Server (mensagem original preservada em `data`) |
| `CONFIG_MISSING` | Ex: `scan_allowlist` vazio ao chamar `db_scan_folder` |
