# CLAUDE.md

Guia para Claude Code (e qualquer agente) trabalhando neste repositório.

## O que é este projeto

`mssql-localdb-mcp` — servidor MCP (Model Context Protocol) escrito em Rust que dá a agentes de IA acesso completo ao Microsoft SQL Server Express LocalDB no Windows: gestão de instâncias, execução de scripts T-SQL, descoberta de bancos soltos em pastas de projeto, introspecção de schema.

Documentos de referência (ler antes de qualquer mudança estrutural):
- `docs/PLANNING.md` — roadmap, fases, escopo de cada milestone.
- `docs/ARCHITECTURE.md` — módulos, fluxo de dados, decisões técnicas e por quê.
- `docs/MCP_SPEC.md` — contrato exato de cada tool/resource/prompt MCP exposto (fonte da verdade para nomes, schemas de input/output).
- `docs/SECURITY.md` — modelo de ameaça e guardrails obrigatórios.

Se um destes documentos divergir do código, o código está errado (ou o doc está desatualizado — atualize o doc na mesma PR).

## Regras não negociáveis

1. **Plataforma**: Windows apenas. Não adicionar cfg cross-platform "por via das dúvidas" — LocalDB não existe fora de Windows. `#[cfg(windows)]` implícito no projeto todo, não precisa espalhar a anotação.
2. **stdout é sagrado**: transporte MCP via stdio usa stdout pro protocolo JSON-RPC. Nenhum `println!`, log, ou saída de subprocess pode vazar pro stdout. Logging sempre via `tracing` configurado pra stderr.
3. **Sem SQL Auth**: só Windows Integrated Authentication. Não implementar campo de senha em nenhuma tool ou config. Isso é decisão de segurança deliberada, não lacuna.
4. **Guardrail de destrutivo é obrigatório**: qualquer tool que rode DDL/DML classificado como destrutivo (`security::classify`) precisa do campo `confirm: true` no input. Não remover essa checagem "pra simplificar". Ver `docs/SECURITY.md`.
5. **Scan de pasta é restrito a allowlist**: `db_scan_folder` nunca varre fora das raízes configuradas em `config.toml`. Não adicionar scan recursivo de `C:\` inteiro nem fallback "se não configurado, varre tudo".
6. **rmcp é o SDK oficial**: não trocar por reimplementação própria do protocolo MCP nem por outro crate de terceiros.

## Convenções de código

- Edition 2021, `cargo fmt` + `cargo clippy --all-targets -- -D warnings` limpos antes de qualquer commit.
- Erros: `thiserror` para tipos de erro de domínio, `anyhow` só em `main.rs`/bordas de processo.
- Toda função que chama `SqlLocalDB.exe` ou executa SQL deve ser testável sem instância real quando possível (parsing de saída isolado da execução do processo).
- Sem comentário óbvio. Comentário só quando explica um porquê não óbvio (workaround de versão do LocalDB, limitação do tiberius, etc.).
- Não adicionar abstração/config genérica além do que a fase atual do roadmap pede.

## Testes

- Testes de integração reais (`tests/`) criam e destroem instância LocalDB temporária — nunca reusar instância do usuário.
- Rodar `cargo test` localmente exige LocalDB instalado (padrão em máquina dev Windows com Visual Studio ou SQL Server tools).
- CI roda em `windows-latest`; verificar disponibilidade de LocalDB no runner antes de assumir presente (ver `docs/PLANNING.md`, seção CI).

## Antes de abrir PR / publicar release

- Atualizar `docs/MCP_SPEC.md` se qualquer tool/resource/prompt mudou de nome, schema ou comportamento.
- Rodar `cargo audit` e `cargo deny check`.
- Tag semver (`vX.Y.Z`) dispara publish automático no MCP Registry — não criar tag manualmente sem revisar `server.json`.
