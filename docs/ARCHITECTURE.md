# Arquitetura — mssql-localdb-mcp

## 1. Visão geral do processo

```
Client MCP (Claude Desktop/Code, etc.)
        │  stdio (JSON-RPC via stdin/stdout)
        ▼
┌───────────────────────────────────────────────┐
│  mssql-localdb-mcp.exe                         │
│                                                 │
│  main.rs ── bootstrap tracing(stderr), config  │
│  mcp/     ── handlers rmcp (tools/resources/prompts)
│  localdb/ ── wrapper SqlLocalDB.exe            │
│  sql/     ── tiberius client (named pipe)      │
│  discovery/ ── scan pastas (.mdf/.ldf)         │
│  security/ ── classificador de risco, allowlist│
│  config/  ── TOML + env                        │
└───────────────────────────────────────────────┘
        │                          │
        ▼                          ▼
  SqlLocalDB.exe (subprocess)   named pipe → sqlservr.exe (instância LocalDB)
```

O processo é **stateless entre chamadas de tool** exceto pela sessão SQL (Fase 2, transações) e pelo pool de conexões por instância. Cada tool call é uma requisição JSON-RPC independente vinda do client MCP.

## 2. Módulos

### `main.rs`
Bootstrap: inicializa `tracing` apontando pra stderr, carrega `config::Config`, monta o servidor `rmcp` com transporte stdio, registra handlers, roda o loop.

Regra crítica: **nada escreve em stdout** exceto o próprio `rmcp` via transporte. Nenhum `println!`, nenhum log default do subprocess `SqlLocalDB.exe` deve vazar (captura stdout/stderr do subprocess e redireciona pra `tracing`, nunca repassa direto).

### `mcp/`
Um handler por tool/resource/prompt, seguindo o contrato exato de `docs/MCP_SPEC.md`. Cada handler:
1. Valida input (schema já garantido pelo `rmcp` via derive, validação semântica extra aqui — ex: path dentro da allowlist).
2. Chama o módulo de domínio correspondente (`localdb::`, `sql::`, `discovery::`).
3. Mapeia erro de domínio pra erro MCP (`McpError`) com mensagem útil ao agente, nunca stack trace cru.

### `localdb/`
Wrapper fino sobre `SqlLocalDB.exe`:
- `Command::new("SqlLocalDB.exe").arg(...)`, captura stdout/stderr, parse de texto (a saída do CLI é texto fixo em inglês na maioria das versões — testar localização/idioma do SO como risco conhecido).
- Funções: `list_instances`, `versions`, `info(name)`, `create(name, version)`, `start(name)`, `stop(name, kill: bool)`, `delete(name)`, `share(name, shared_name)`, `unshare(name)`, `trace(name, on: bool)`.
- `info()` retorna struct parseada (nome, versão, estado, named pipe, owner, auto-create, shared) — é daqui que `sql::` pega o pipe name pra conectar.
- Erros de parsing isolados de erros de execução do processo (testável sem `SqlLocalDB.exe` real, usando fixtures de saída capturada).

### `sql/`
- `Connection`: abre `NamedPipeClient` (tokio) pro pipe retornado por `localdb::info`, entrega pro `tiberius::Client::connect`.
- Pool simples por instância+database (evita reconectar a cada chamada; TTL de ociosidade fecha conexão).
- `execute_script(sql: &str) -> Vec<BatchResult>`: split por linha `GO` (case-insensitive, respeitando que `GO` só conta como separador quando é a linha inteira), roda cada batch sequencialmente, agrega mensagens `PRINT`/erros/rowcount por batch.
- `execute_query(sql: &str, max_rows: usize) -> QueryResult`: só aceita statement classificado como leitura (ver `security::classify`), serializa rows pra JSON preservando tipos (datetime, decimal, varbinary → base64, etc.).
- `execute_statement(sql: &str, confirm: bool)`: chama `security::classify` primeiro; se destrutivo e `confirm != true`, retorna erro MCP pedindo confirmação explícita (não executa).

### `discovery/`
- `scan_folder(root: &Path) -> Vec<FoundDatabase>`: valida `root` contra allowlist do config (canonicaliza, rejeita `..`, rejeita se fora de qualquer raiz permitida), `walkdir` com profundidade máxima configurável, filtra `.mdf`/`.ldf`, cruza com `sys.databases` de instâncias ativas pra marcar `already_attached: bool`.
- Sem follow de symlink por padrão (evita loop).

### `security/`
- `classify(sql: &str) -> RiskLevel` (`ReadOnly | Destructive`): v1 via regex/keywords (`DROP`, `TRUNCATE`, `ALTER`, `DELETE`, `UPDATE` sem cláusula `WHERE`, `DROP DATABASE`, etc.); Fase 2 migra pra `sqlparser-rs` (AST real, elimina falso-negativo de regex).
- `validate_path(path: &Path, allowlist: &[PathBuf]) -> Result<PathBuf>`: canonicaliza e confere prefixo contra allowlist.
- `AuditLog`: append-only `.jsonl` em `%APPDATA%\mssql-localdb-mcp\audit\`, um registro por execução (timestamp, tool, hash do SQL, resultado resumido). Fase 2.

### `config/`
- `Config` carregado de `%APPDATA%\mssql-localdb-mcp\config.toml`, com override por env var (`MSSQL_LOCALDB_MCP_*`).
- Campos mínimos v1: `scan_allowlist: Vec<PathBuf>` (obrigatório, vazio = `db_scan_folder` sempre erro explicando que precisa configurar), `default_query_timeout_secs`, `default_max_rows`, `log_level`.

## 3. Decisões técnicas e por quê

| Decisão | Alternativa considerada | Por que essa escolha |
|---|---|---|
| Wrapper CLI `SqlLocalDB.exe` em vez de FFI `SQLUserInstance.dll` | Bind direto via `windows` crate + header `msoledbsql.h` | CLI é estável entre versões do LocalDB, documentado publicamente; FFI exige manter binding sincronizado com mudanças de ABI não documentadas publicamente como API estável |
| Named pipe em vez de TCP | Forçar LocalDB a expor TCP | LocalDB usa named pipe por padrão; forçar TCP exige passo de config extra no lado do usuário, quebra o "funciona out of the box" |
| `sqlparser-rs` em vez de regex puro pra classificação de risco | Regex only | Regex tem falso-negativo (ex: `DELETE` dentro de comentário/string escapa `WHERE` detection); AST é correto por construção. v1 aceita regex como ponte, Fase 2 migra |
| stdio como transporte único v1 | HTTP/SSE | Clients MCP desktop (Claude Desktop/Code) usam stdio como padrão pra servidor local; HTTP adicionaria superfície de rede desnecessária pra um servidor que só faz sentido local |
| Sem SQL Auth | Suportar usuário/senha | LocalDB é sempre local, Windows Integrated é o modelo nativo; adicionar senha é superfície de vazamento de secret sem ganho real de funcionalidade |

## 4. Fluxo de dados — exemplo `db_scan_folder` → `db_attach` → `sql_execute_query`

1. Agente chama `db_scan_folder({ root: "D:\\Projetos\\Cliente" })`.
2. `mcp/` valida via `security::validate_path` contra `config.scan_allowlist`.
3. `discovery::scan_folder` percorre, acha `cliente.mdf` não anexado, retorna item com path e metadata.
4. Agente chama `db_attach({ instance: "MSSQLLocalDB", mdf_path: "D:\\Projetos\\Cliente\\cliente.mdf" })`.
5. `mcp/` pede `localdb::info("MSSQLLocalDB")` pro pipe name (inicia a instância se não estiver rodando), abre `sql::Connection`, roda `CREATE DATABASE ... FOR ATTACH`.
6. Agente chama `sql_execute_query({ instance: "MSSQLLocalDB", database: "cliente", sql: "SELECT TOP 10 * FROM Pedidos" })`.
7. `security::classify` confirma `ReadOnly`, `sql::execute_query` roda, serializa resultado, retorna JSON pro agente.

## 5. O que NÃO fazer (armadilhas conhecidas)

- Não abrir conexão nova a cada linha de `execute_script` — uma conexão, múltiplos batches sequenciais na mesma sessão (necessário pra `GO` funcionar como o SSMS espera, inclusive variáveis de sessão como `SET` persistindo entre batches).
- Não confiar cegamente na saída de `SqlLocalDB.exe` em qualquer idioma — se o SO estiver em PT-BR, mensagens podem vir traduzidas; usar `info` em modo que force código de retorno + parsing tolerante, ou fixar `LANG`/`chcp` do subprocess se possível.
- Não deixar `db_scan_folder` sem limite de profundidade — pasta de projeto pode ter `node_modules` gigante misturado, precisa profundidade máxima configurável e ignore-list básica (`node_modules`, `.git`, `bin`, `obj`).
