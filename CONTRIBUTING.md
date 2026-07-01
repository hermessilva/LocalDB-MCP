# Contribuindo

## Requisitos

- Windows com SQL Server Express LocalDB instalado (`SqlLocalDB.exe` no `PATH`).
- Rust estável (edition 2024) via [rustup](https://rustup.rs).

## Build e testes

```powershell
cargo build
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

Os testes de integração criam e destroem instâncias LocalDB temporárias reais — não usam mock. Rodar `cargo test` localmente exige LocalDB instalado.

## Antes de abrir PR

- `cargo fmt`, `cargo clippy -D warnings` e `cargo test` limpos.
- Se mudou nome/schema/comportamento de alguma tool, resource ou prompt MCP, atualize [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) na mesma PR — é a fonte da verdade do contrato exposto.
- Se mudou modelo de ameaça ou guardrail de segurança, atualize [`docs/SECURITY.md`](docs/SECURITY.md).
- Leia [`CLAUDE.md`](CLAUDE.md) — regras não-negociáveis do projeto (stdout reservado ao protocolo MCP, sem SQL Auth, guard de confirmação obrigatório em ação destrutiva, scan restrito a allowlist).

## Estrutura do projeto

Ver [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) para os módulos e as decisões técnicas (e os porquês). Ver [`docs/PLANNING.md`](docs/PLANNING.md) para o roadmap e em qual fase cada funcionalidade está.

## Reportando bugs / sugerindo funcionalidade

Abra uma [issue](https://github.com/hermessilva/LocalDB-MCP/issues). Pra bug, inclua: versão do LocalDB (`SqlLocalDB.exe versions`), a tool/comando MCP usado, e a mensagem de erro completa.

## Segurança

Não abra issue pública pra vulnerabilidade de segurança. Ver [`docs/SECURITY.md`](docs/SECURITY.md) pro modelo de ameaça — se achar algo fora do que já está documentado como risco aceito, reporte de forma privada (ex: GitHub Security Advisory do repositório).
