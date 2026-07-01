# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/),
versionamento [SemVer](https://semver.org/lang/pt-BR/).

## [Unreleased]

### Added

- Esqueleto Fase 1 (MVP): servidor MCP funcional via `rmcp` (SDK oficial) + `tiberius` sobre named pipe com Windows Integrated Authentication.
- Gestão de instância LocalDB: `localdb_list_instances`, `localdb_versions`, `localdb_info`, `localdb_create_instance`, `localdb_start_instance`, `localdb_stop_instance`, `localdb_delete_instance`.
- Canal de script/SQL: `sql_execute_script`, `sql_execute_query`, `sql_execute_statement` — com guard de confirmação obrigatório para ação destrutiva.
- Descoberta de banco: `db_scan_folder` (restrito a allowlist), `db_attach`, `db_detach`, `db_list_tables`.
- Infra de publicação: `server.json` (MCP Registry oficial), `mcpb/manifest.json` (Desktop Extension), CI (fmt/clippy/test/build) e workflow de release automatizado via GitHub OIDC.

[Unreleased]: https://github.com/hermessilva/LocalDB-MCP/compare/master...HEAD
