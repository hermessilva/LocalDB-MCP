# mssql-localdb-mcp

Servidor [MCP](https://modelcontextprotocol.io) em Rust para SQL Server Express LocalDB no Windows. Dá a agentes de IA (Claude Desktop, Claude Code, e qualquer client MCP) controle completo sobre instâncias LocalDB: criar/gerenciar instâncias, executar qualquer script T-SQL, achar bancos de dados soltos em pastas de projeto e anexá-los, introspectar schema.

> Status: MVP (Fase 1) funcional e testado contra LocalDB real — ver `docs/PLANNING.md` para roadmap e fase atual. Ainda sem release publicado no MCP Registry.

## Por que

Ferramentas MCP existentes pra SQL Server assumem servidor remoto já configurado e exigem runtime externo (Python, .NET, Node). Este projeto é:
- **Binário único Rust**, sem runtime externo, sem Docker.
- **Focado em LocalDB**: fluxo de dev local — achar `.mdf` solto numa pasta de projeto, anexar, rodar script, sem precisar abrir SSMS.
- **Windows-only por design**, não uma abstração genérica multi-SGBD.

## Requisitos

- Windows com SQL Server Express LocalDB instalado (`SqlLocalDB.exe` no `PATH`).
- Client MCP compatível (Claude Desktop, Claude Code, etc.).

## Instalação

Ainda não publicado no MCP Registry — por enquanto, build a partir do código fonte. Ver `docs/PLANNING.md`, Fase 3, para o plano de publicação (`io.github.hermessilva/mssql-localdb-mcp`).

### 1. Build

Requer [Rust](https://rustup.rs) e LocalDB instalado.

```powershell
git clone https://github.com/hermessilva/LocalDB-MCP.git
cd LocalDB-MCP
cargo build --release
```

Binário em `target\release\mssql-localdb-mcp.exe`.

### 2. Configurar `config.toml`

`db_scan_folder` exige ao menos uma raiz liberada explicitamente — sem isso, a tool recusa rodar. Crie `%APPDATA%\mssql-localdb-mcp\config.toml`:

```toml
# Paths do Windows em TOML precisam de aspas simples (string literal) —
# aspas duplas interpretam \U... como escape unicode e quebram o parse.
scan_allowlist = ['C:\Users\SeuUsuario\source\repos']
scan_max_depth = 6
default_query_timeout_secs = 30
default_max_rows = 1000
```

### 3. Configurar no client MCP

**Claude Desktop / Claude Code** (`claude_desktop_config.json` ou equivalente):

```json
{
  "mcpServers": {
    "mssql-localdb": {
      "command": "C:\\caminho\\para\\LocalDB-MCP\\target\\release\\mssql-localdb-mcp.exe"
    }
  }
}
```

**Claude Code via CLI:**

```powershell
claude mcp add mssql-localdb -- "C:\caminho\para\LocalDB-MCP\target\release\mssql-localdb-mcp.exe"
```

Depois disso, o agente já tem acesso às tools `localdb_*`, `sql_*` e `db_*` — ver [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) pra lista completa.

## Documentação

- [`docs/PLANNING.md`](docs/PLANNING.md) — roadmap, fases, escopo de cada milestone.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — módulos, decisões técnicas, fluxo de dados.
- [`docs/MCP_SPEC.md`](docs/MCP_SPEC.md) — contrato exato de cada tool/resource/prompt exposto.
- [`docs/SECURITY.md`](docs/SECURITY.md) — modelo de ameaça e guardrails.
- [`CLAUDE.md`](CLAUDE.md) — guia pra agentes de IA trabalhando neste repositório.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — como contribuir.
- [`CHANGELOG.md`](CHANGELOG.md) — histórico de mudanças.

## Segurança — leia antes de usar

- Só Windows Integrated Authentication (sem SQL Auth).
- Toda ação destrutiva (DROP, TRUNCATE, DELETE, ALTER, etc.) exige confirmação explícita (`confirm: true`) — nunca executa silenciosamente.
- Descoberta de banco (`db_scan_folder`) só varre pastas explicitamente liberadas em `config.toml` (allowlist).
- Detalhe completo em [`docs/SECURITY.md`](docs/SECURITY.md).

## Licença

[MIT](LICENSE).
