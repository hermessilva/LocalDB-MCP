# Modelo de segurança — mssql-localdb-mcp

## 1. Modelo de ameaça

Servidor MCP local, roda com os privilégios do usuário Windows que o invocou (o mesmo client MCP — Claude Desktop/Code — está rodando na mesma sessão). Não há autenticação de rede: o servidor confia no client MCP que o invocou via stdio, como qualquer servidor MCP local.

Riscos principais não vêm de rede, vêm de:
1. **Agente de IA gerando SQL destrutivo sem intenção clara do usuário** (alucinação, prompt injection via dado lido de uma tabela, instrução ambígua).
2. **Agente varrendo/lendo arquivos fora do escopo pretendido** via `db_scan_folder`/`db_attach`.
3. **Escalada de privilégio dentro do próprio LocalDB**: LocalDB dá `sysadmin` ao usuário que cria a instância por padrão — o servidor herda esse nível dentro do banco.

Fora de escopo (não é o servidor que resolve):
- Comprometimento da máquina do usuário (se o atacante já tem code execution local, MCP não é a superfície relevante).
- Ataque de rede — não há listener de rede, transporte é stdio.

## 2. Guardrails implementados

### 2.1 Confirmação explícita pra ação destrutiva
`security::classify(sql: &str) -> RiskLevel`:
- `ReadOnly`: `SELECT` puro (sem `INTO`), sem side-effect.
- `Destructive`: `DROP`, `TRUNCATE`, `ALTER`, `DELETE`, `UPDATE` sem `WHERE`, `DELETE`/`UPDATE` com `WHERE` sempre presente mas ainda marcado destrutivo (não tenta avaliar "quão destrutivo", qualquer DML/DDL de escrita é destrutivo por padrão), `CREATE DATABASE ... FOR ATTACH` não é destrutivo (é aditivo), `DROP DATABASE` é.

Toda tool que executa SQL não-`SELECT` exige `confirm: true` no input quando o resultado da classificação é `Destructive`. Sem esse flag, a tool retorna `CONFIRMATION_REQUIRED` e **não executa nada** — nem parcialmente.

v1 usa regex/keyword matching (rápido de implementar, cobre os casos comuns). Falso-negativo conhecido: SQL ofuscado/dinâmico dentro de string (`EXEC('DELETE FROM...')`) pode escapar detecção por regex. Fase 2 migra pra `sqlparser-rs` (parse real do AST) especificamente pra fechar esse gap — não é um "nice to have", é a razão da migração.

Isso **não é um sandbox** — é um freio de fricção. O agente (ou o usuário através do agente) ainda pode confirmar e executar qualquer coisa. O objetivo é evitar execução acidental, não impedir uso malicioso deliberado por quem já tem acesso legítimo.

### 2.2 Allowlist de pastas pro scan
`config.scan_allowlist` é obrigatório e vazio por padrão — `db_scan_folder` recusa rodar até o usuário configurar explicitamente quais raízes podem ser varridas. Nunca há fallback "varre tudo se não configurado".

`security::validate_path`:
- Canonicaliza o path recebido (resolve `..`, links).
- Confere que o resultado tem como prefixo alguma entrada da allowlist.
- Rejeita paths fora disso com `PATH_NOT_ALLOWED`, sem detalhar estrutura de diretório fora da allowlist na mensagem de erro (evita vazar informação de path via mensagem de erro).

`db_attach` usa a mesma validação pro `mdf_path` — não dá pra anexar banco de fora da allowlist mesmo que o path venha de outra fonte (ex: sugerido pelo próprio agente).

Scan tem profundidade máxima configurável e ignora por padrão `node_modules`, `.git`, `bin`, `obj` (evita custo e ruído).

### 2.3 Sem SQL Authentication
Servidor só conecta via Windows Integrated Authentication (o modelo nativo do LocalDB). Não existe campo de senha em nenhuma tool, config ou log. Isso elimina uma classe inteira de risco (secret em log, secret em config file, secret em mensagem de erro).

### 2.4 Timeout e limite de linhas por padrão
Toda query tem timeout (`config.default_query_timeout_secs`) e `sql_execute_query` tem `max_rows` default — evita o agente travar a instância com query sem `TOP`/`WHERE` em tabela grande, e evita estourar contexto do agente com resultado gigante (retorna `truncated: true` quando corta).

### 2.5 Audit log (Fase 2)
Todo `sql_execute_*` grava em `%APPDATA%\mssql-localdb-mcp\audit\*.jsonl`: timestamp, tool, instância, database, hash SHA-256 do SQL (não o conteúdo — evita duplicar dado sensível no log), classificação de risco, resultado resumido (sucesso/erro, rows_affected). Serve pra auditoria pós-fato, não pra bloqueio em tempo real.

### 2.6 Mensagens de erro sem vazamento
Erro `SQL_ERROR` preserva a mensagem original do SQL Server em `data` (útil pro agente corrigir a query), mas nunca inclui stack trace do Rust nem caminho de arquivo interno do servidor. `PATH_NOT_ALLOWED` não ecoa a allowlist completa (evita reconhecimento de estrutura de disco por tentativa e erro).

## 3. Riscos aceitos e documentados (não resolvidos pelo servidor)

- **`sysadmin` por padrão na instância criada pelo usuário**: comportamento nativo do LocalDB, não algo que o servidor MCP controla. Documentar no README como risco conhecido; usuário que precisa de least-privilege real deve criar login específico manualmente e usar esse contexto — fora do escopo v1 (não há suporte a impersonation/login customizado).
- **Prompt injection via dado lido do banco**: se uma tabela contém texto malicioso que o agente lê via `sql_execute_query` e depois interpreta como instrução, isso é um risco do modelo/agente, não algo que o servidor MCP consegue filtrar de forma confiável. Mitigação parcial: guardrail de `confirm` continua exigido pra qualquer ação subsequente destrutiva, então o pior caso ainda para na confirmação.
- **Confiança no client MCP**: se o client MCP em si estiver comprometido, ele pode simplesmente mandar `confirm: true` sempre. O guardrail assume um client MCP honesto que expõe a confirmação pro usuário real (é assim que Claude Desktop/Code funcionam) — não é proteção contra client malicioso.

## 4. Checklist de revisão de segurança (rodar antes de cada release)

- [ ] Nenhuma tool nova de escrita/DDL sem passar por `security::classify` + guard de `confirm`.
- [ ] Nenhum path novo aceito por tool sem passar por `security::validate_path` quando aplicável.
- [ ] `cargo audit` limpo (sem CVE conhecida em dependência).
- [ ] Nenhum log/mensagem de erro novo vazando conteúdo de config, path fora de escopo, ou stack trace.
- [ ] `docs/SECURITY.md` atualizado se o modelo de ameaça mudou (ex: se algum dia suportar transporte de rede, este documento precisa de seção nova inteira).
