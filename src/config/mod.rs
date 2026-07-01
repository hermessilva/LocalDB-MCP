use std::path::PathBuf;

use serde::Deserialize;

/// Configuração carregada de `%APPDATA%\mssql-localdb-mcp\config.toml`,
/// com override por variável de ambiente `MSSQL_LOCALDB_MCP_*`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Raízes permitidas para `db_scan_folder`. Vazio por padrão — a tool
    /// recusa rodar até o usuário configurar isso explicitamente.
    pub scan_allowlist: Vec<PathBuf>,
    pub default_query_timeout_secs: u64,
    pub default_max_rows: usize,
    pub log_level: String,
    pub scan_max_depth: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scan_allowlist: Vec::new(),
            default_query_timeout_secs: 30,
            default_max_rows: 1_000,
            log_level: "info".to_string(),
            scan_max_depth: 8,
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path()?;

        let mut config = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("falha ao ler {}: {e}", path.display()))?;
            toml::from_str(&text)
                .map_err(|e| anyhow::anyhow!("config.toml inválido em {}: {e}", path.display()))?
        } else {
            Config::default()
        };

        config.apply_env_overrides();
        Ok(config)
    }

    pub fn config_path() -> anyhow::Result<PathBuf> {
        let base = directories::BaseDirs::new()
            .ok_or_else(|| anyhow::anyhow!("não foi possível determinar %APPDATA%"))?;
        Ok(base
            .config_dir()
            .join("mssql-localdb-mcp")
            .join("config.toml"))
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("MSSQL_LOCALDB_MCP_LOG_LEVEL") {
            self.log_level = v;
        }
        if let Ok(v) = std::env::var("MSSQL_LOCALDB_MCP_DEFAULT_MAX_ROWS") {
            if let Ok(n) = v.parse() {
                self.default_max_rows = n;
            }
        }
        if let Ok(v) = std::env::var("MSSQL_LOCALDB_MCP_QUERY_TIMEOUT_SECS") {
            if let Ok(n) = v.parse() {
                self.default_query_timeout_secs = n;
            }
        }
    }
}
