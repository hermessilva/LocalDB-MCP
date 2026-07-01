# Planejamento — mssql-localdb-mcp

## 1. Visão

Servidor MCP em Rust, binário único, Windows-only, que expõe ao agente de IA controle total sobre SQL Server Express LocalDB: gestão de instâncias, execução de qualquer script/comando T-SQL suportado pelo LocalDB, e descoberta de bancos de dados soltos em pastas de projeto (cenário comum: várias pastas com `.mdf`/`.ldf` não anexados a nenhuma instância).

### Por que existe
Servidores MCP para SQL Server hoje (Python, .NET, Node) assumem conexão remota/já configurada e exigem runtime externo. Nenhum é: (a) Rust nativo self-contained, (b) focado no ciclo de vida completo do LocalDB (criar instância → achar banco solto → anexar → rodar script), (c) publicado no MCP Registry oficial como binário standalone.

### Não-objetivos (fora de escopo v1)
- Suporte a SQL Server completo (on-prem/Azure) — só LocalDB.
- SQL Authentication — só Windows Integrated.
- Linux/macOS — LocalDB não existe lá.
- GUI — só servidor MCP (protocolo stdio).

## 2. Stack técnica (resumo — detalhe em ARCHITECTURE.md)

| Área | Escolha | Motivo |
|---|---|---|
| SDK MCP | `rmcp` (oficial, modelcontextprotocol/rust-sdk) | SDK oficial, mantido, ~0.16 na crates.io |
| Driver SQL | `tiberius` | Driver TDS async maduro em Rust |
| Canal transporte DB | Named pipe (`tokio::net::windows::named_pipe`) | LocalDB usa named pipe por padrão, não TCP |
| Gestão de instância | wrapper `SqlLocalDB.exe` via `std::process::Command` | Mais estável entre versões que FFI direto em `SQLUserInstance.dll` |
| Scan de pasta | `walkdir` | Simples, testado, suficiente pro volume esperado |
| Classificação de risco SQL | `sqlparser-rs` | AST real, mais confiável que regex |
| Runtime async | `tokio` | Padrão do ecossistema, exigido por tiberius |
| Logging | `tracing` → stderr | stdout reservado ao protocolo MCP |

## 3. Fases

### Fase 1 — MVP (v0.1.0)
Objetivo: agente consegue achar um banco numa pasta, anexar, e rodar SQL nele.

- [x] Esqueleto do projeto (`Cargo.toml`, módulos, `main.rs` com transporte stdio funcionando, handshake MCP básico)
- [x] `localdb::` wrapper: `list_instances`, `versions`, `info`, `create_instance`, `start_instance`, `stop_instance`, `delete_instance`
- [x] `sql::` client: conectar via named pipe, `execute_script` (split por `GO`), `execute_query` (SELECT), `execute_statement` (com guard de confirmação)
- [x] `discovery::` scan de pasta (`db_scan_folder`) restrito a allowlist de config
- [x] `db_attach` / `db_detach`
- [x] `db_list_tables` (introspecção mínima via `INFORMATION_SCHEMA`)
- [x] `config::` — TOML em `%APPDATA%\mssql-localdb-mcp\config.toml`, allowlist de pastas obrigatória
- [x] `security::classify` — classificador básico (regex + keywords) de statement destrutivo
- [ ] Testes de integração automatizados (`cargo test --test integration`): criar/destruir instância temporária, attach/detach, execute_script — validado manualmente via handshake MCP real (ver histórico), mas ainda não como suite automatizada no CI
- [x] README com instruções de instalação manual (build) e config no Claude Desktop/Code

Critério de saída: rodar localmente contra LocalDB real, todas as tools do MVP funcionando via `mcp-inspector` ou Claude Desktop. **Atingido** — validado manualmente contra instância real e contra um banco de projeto real (`.mdf`/`.ldf` de ~75MB), incluindo ciclo completo scan → attach → introspect → detach. Dois bugs achados e corrigidos nesse processo (ver `CHANGELOG.md`).

### Fase 2 — Superfície completa
- [ ] Resto da gestão de instância: `share`/`unshare`, `trace`
- [ ] `sql_begin_transaction`/`commit`/`rollback` (sessão stateful)
- [ ] `sql_bulk_insert` (bulk load nativo do tiberius)
- [ ] `db_backup` / `db_restore`
- [ ] `db_list_columns`, `db_list_indexes`, `db_list_procedures`, `db_describe_object`
- [ ] `db_get_info`, `db_list_attached`
- [ ] Resources MCP: `localdb://instances`, `localdb://{instance}/databases/{db}/schema`
- [ ] Prompts MCP: `generate-migration-script`, `explain-schema`, `write-safe-delete`
- [ ] Audit log (`.jsonl`) de toda execução
- [ ] Migrar classificador de risco de regex pra `sqlparser-rs` (AST real)

Critério de saída: `docs/MCP_SPEC.md` cobre 100% da superfície implementada; nenhuma tool "TODO".

### Fase 3 — Publicação
- [ ] Assinatura de código (Authenticode) do binário release — **adiado**, sem certificado disponível; v0.1.0 sai sem assinar (usuário vê SmartScreen no primeiro run). Ver backlog.
- [x] Empacotamento `mcpb` (`mcpb/manifest.json` + binário) — montado em CI, ver `.github/workflows/release.yml`
- [x] `server.json` na raiz do repo, namespace `io.github.hermessilva/mssql-localdb-mcp`
- [x] GitHub Actions: `ci.yml` (fmt/clippy/test/build em push/PR) + `release.yml` (build + empacota mcpb + GitHub Release + publish no MCP Registry via `mcp-publisher` + GitHub OIDC, dispara em tag `vX.Y.Z`) — **não testado em CI real ainda**, primeira tag vai validar o workflow de ponta a ponta
- [x] Docs de comunidade: `README.md` completo, `CONTRIBUTING.md`, `LICENSE`, exemplos de config (Claude Desktop, Claude Code)
- [x] `CHANGELOG.md`
- [x] `scripts/prepare-release.ps1` — bump de versão + build local + empacota `.mcpb` pra smoke test antes de tag

Critério de saída: `io.github.hermessilva/mssql-localdb-mcp` instalável via MCP Registry oficial, binário assinado, sem warning de SmartScreen bloqueante. **Parcialmente atingido** — infra pronta, mas publicação real (push da tag `v0.1.0`) ainda não disparada, e assinatura de código fica pra release futura.

### Fase 4 — Robustez
- [ ] MARS (Multiple Active Result Sets), se suportado pelo tiberius/LocalDB
- [ ] Telemetria opcional opt-in (nunca por default)
- [ ] CI de integração real contra LocalDB no runner `windows-latest`
- [ ] `cargo audit` + `cargo deny` no pipeline
- [ ] Benchmark de scan de pasta grande (milhares de `.mdf`), paralelizar com `rayon` se necessário

## 4. Backlog de decisões pendentes (revisar antes de cada fase)

- Suporte a `sqlcmd` variables (`:setvar`, `$(VAR)`) no `sql_execute_script` — v1 não suporta, avaliar demanda real antes de adicionar.
- Se `SQLUserInstance.dll` (API nativa) compensa substituir o wrapper CLI — só revisitar se o wrapper CLI mostrar limitação real (parsing frágil, performance).
- Formato exato de paginação em `sql_execute_query` (`OFFSET/FETCH` vs `TOP` vs cursor) — decidido: `max_rows` com truncamento simples (v1), sem cursor/paginação real.
- Assinatura de código (Authenticode): sem certificado disponível em 2026-07-01. v0.1.0 publica sem assinar. Revisitar quando houver certificado (EV reduz fricção de SmartScreen mais rápido que OV) — orçar isso antes de qualquer push de marketing do projeto, já que SmartScreen sem assinatura afugenta usuário leigo.
- `.github/workflows/release.yml` foi escrito mas nunca disparado de verdade (nenhuma tag `vX.Y.Z` criada ainda) — primeira tag real deve ser tratada como dry-run, com atenção a: presença de LocalDB no runner (ver seção 5), nome exato do asset `mcp-publisher` (confirmado `mcp-publisher_windows_amd64.tar.gz` contendo `mcp-publisher.exe` em 2026-07-01, mas repo upstream pode mudar convenção), e se o namespace `io.github.hermessilva` já está validado via GitHub OIDC no MCP Registry.

## 5. CI

- Runner `windows-latest`: confirmar presença de LocalDB antes de assumir (pode exigir `winget install Microsoft.SQLServer.2022.LocalDB` ou similar no workflow). **Ainda não confirmado** — `ci.yml`/`release.yml` rodam `cargo test`/`cargo build` mas nenhum já rodou de verdade num runner GitHub; testes de integração real com LocalDB podem falhar no CI se o runner não tiver a instância pré-instalada.
- Pipeline: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (unit), `cargo test --test integration` (exige LocalDB), `cargo audit`.

## 6. Métrica de sucesso do projeto

- MVP funcional localmente validado por uso real (dogfooding) antes de qualquer publicação.
- Zero dependência de runtime externo pro usuário final (só o binário + LocalDB já instalado, que é pré-requisito natural).
- Listado no MCP Registry oficial, instalável em 1 comando/config pelos clients MCP populares (Claude Desktop, Claude Code).
