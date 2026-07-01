# mssql-localdb-mcp

Servidor [MCP](https://modelcontextprotocol.io) em Rust para SQL Server Express LocalDB no Windows. Dá a agentes de IA (Claude Desktop, Claude Code, e qualquer client MCP) controle completo sobre instâncias LocalDB: criar/gerenciar instâncias, executar qualquer script T-SQL, achar bancos de dados soltos em pastas de projeto e anexá-los, introspectar schema.

> Status: em planejamento — ver `docs/PLANNING.md` para roadmap e fase atual. Nenhum release publicado ainda.

## Por que

Ferramentas MCP existentes pra SQL Server assumem servidor remoto já configurado e exigem runtime externo (Python, .NET, Node). Este projeto é:
- **Binário único Rust**, sem runtime externo, sem Docker.
- **Focado em LocalDB**: fluxo de dev local — achar `.mdf` solto numa pasta de projeto, anexar, rodar script, sem precisar abrir SSMS.
- **Windows-only por design**, não uma abstração genérica multi-SGBD.

## Requisitos

- Windows com SQL Server Express LocalDB instalado (`SqlLocalDB.exe` no `PATH`).
- Client MCP compatível (Claude Desktop, Claude Code, etc.).

## Instalação

Ainda não publicado. Ver `docs/PLANNING.md`, Fase 3, para plano de publicação no MCP Registry oficial (`io.github.<user>/mssql-localdb-mcp`).

## Documentação

- [`docs/PLANNING.md`](docs/PLANNING.md) — roadmap, fases, escopo de cada milestone.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — módulos, decisões técnicas, fluxo de dados.
- [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) — contrato exato de cada tool/resource/prompt exposto.
- [`docs/SECURITY.md`](docs/SECURITY.md) — modelo de ameaça e guardrails.
- [`CLAUDE.md`](CLAUDE.md) — guia pra agentes de IA trabalhando neste repositório.

## Segurança — leia antes de usar

- Só Windows Integrated Authentication (sem SQL Auth).
- Toda ação destrutiva (DROP, TRUNCATE, DELETE, ALTER, etc.) exige confirmação explícita (`confirm: true`) — nunca executa silenciosamente.
- Descoberta de banco (`db_scan_folder`) só varre pastas explicitamente liberadas em `config.toml` (allowlist).
- Detalhe completo em [`docs/SECURITY.md`](docs/SECURITY.md).

## Licença

[MIT](LICENSE).
