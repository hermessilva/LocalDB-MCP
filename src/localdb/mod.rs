use std::collections::HashMap;

use serde::Serialize;
use thiserror::Error;
use tokio::process::Command;

const EXE: &str = "SqlLocalDB.exe";
const DEFAULT_INSTANCE: &str = "MSSQLLocalDB";

#[derive(Debug, Error)]
pub enum LocalDbError {
    #[error("SqlLocalDB.exe não encontrado no PATH — LocalDB está instalado?")]
    ExecutableNotFound,
    #[error("SqlLocalDB.exe falhou: {0}")]
    CommandFailed(String),
    #[error("instância '{0}' iniciada mas sem pipe name na saída de SqlLocalDB.exe")]
    NoPipeName(String),
    #[error("erro de E/S: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, LocalDbError>;

#[derive(Debug, Clone, Serialize)]
pub struct InstanceSummary {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstanceInfo {
    pub name: String,
    pub version: String,
    pub state: String,
    pub pipe_name: Option<String>,
    pub owner: String,
    pub auto_create: bool,
    pub shared: bool,
    pub shared_name: Option<String>,
    pub last_start_time: Option<String>,
}

async fn run(args: &[&str]) -> Result<String> {
    let output = Command::new(EXE).args(args).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LocalDbError::ExecutableNotFound
        } else {
            LocalDbError::Io(e)
        }
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let message = if stderr.trim().is_empty() { stdout } else { stderr };
        return Err(LocalDbError::CommandFailed(message.trim().to_string()));
    }

    Ok(stdout)
}

fn parse_kv_block(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some(idx) = line.find(':') {
            let key = line[..idx].trim().to_string();
            let value = line[idx + 1..].trim().to_string();
            if !key.is_empty() {
                map.insert(key, value);
            }
        }
    }
    map
}

fn non_empty(v: Option<&String>) -> Option<String> {
    v.filter(|s| !s.is_empty()).cloned()
}

pub async fn list_instances() -> Result<Vec<InstanceSummary>> {
    let out = run(&["info"]).await?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|name| InstanceSummary {
            name: name.to_string(),
            is_default: name.eq_ignore_ascii_case(DEFAULT_INSTANCE),
        })
        .collect())
}

pub async fn versions() -> Result<Vec<String>> {
    let out = run(&["versions"]).await?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

pub async fn info(name: &str) -> Result<InstanceInfo> {
    let out = run(&["info", name]).await?;
    let kv = parse_kv_block(&out);
    let get = |k: &str| kv.get(k).cloned().unwrap_or_default();

    Ok(InstanceInfo {
        name: name.to_string(),
        version: get("Version"),
        state: get("State"),
        pipe_name: non_empty(kv.get("Instance pipe name")),
        owner: get("Owner"),
        auto_create: get("Auto-create").eq_ignore_ascii_case("yes"),
        shared: non_empty(kv.get("Shared name")).is_some(),
        shared_name: non_empty(kv.get("Shared name")),
        last_start_time: non_empty(kv.get("Last start time")),
    })
}

pub async fn create(name: &str, version: Option<&str>, start: bool) -> Result<()> {
    let mut args = vec!["create", name];
    if let Some(v) = version {
        args.push(v);
    }
    if start {
        args.push("-s");
    }
    run(&args).await?;
    Ok(())
}

/// Inicia a instância e retorna o pipe name usado para conectar via `sql::`.
pub async fn start(name: &str) -> Result<String> {
    run(&["start", name]).await?;
    info(name)
        .await?
        .pipe_name
        .ok_or_else(|| LocalDbError::NoPipeName(name.to_string()))
}

pub async fn stop(name: &str, kill: bool) -> Result<()> {
    let mut args = vec!["stop", name];
    if kill {
        args.push("-k");
    }
    run(&args).await?;
    Ok(())
}

pub async fn delete(name: &str) -> Result<()> {
    run(&["delete", name]).await?;
    Ok(())
}

/// Garante que a instância está rodando e retorna o pipe name pra conectar.
/// Inicia a instância automaticamente se estiver parada (`auto_create` do
/// LocalDB já cobre a maioria dos casos, mas isso torna explícito).
pub async fn ensure_running(name: &str) -> Result<String> {
    let current = info(name).await?;
    if current.state.eq_ignore_ascii_case("running") {
        if let Some(pipe) = current.pipe_name {
            return Ok(pipe);
        }
    }
    start(name).await
}
