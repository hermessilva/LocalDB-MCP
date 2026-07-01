use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;
use tiberius::{AuthMethod, Client, Config};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use crate::security::{self, RiskLevel};

pub type TdsClient = Client<Compat<NamedPipeClient>>;

const ERROR_PIPE_BUSY: i32 = 231;
const PIPE_BUSY_RETRY_DELAY: Duration = Duration::from_millis(50);
const PIPE_BUSY_MAX_ATTEMPTS: u32 = 20;

#[derive(Debug, Error)]
pub enum SqlError {
    #[error("falha ao conectar no named pipe '{pipe}': {source}")]
    PipeConnect {
        pipe: String,
        #[source]
        source: std::io::Error,
    },
    #[error("erro do driver TDS: {0}")]
    Tiberius(#[from] tiberius::error::Error),
    #[error("statement não é somente leitura — use sql_execute_statement/sql_execute_script")]
    NotReadOnly,
    #[error("ação destrutiva requer confirm=true nos batches: {0:?}")]
    ConfirmationRequired(Vec<usize>),
}

pub type Result<T> = std::result::Result<T, SqlError>;

/// Converte o pipe name no formato TDS (`np:\\.\pipe\...`) pro formato
/// aceito pela API Win32 de named pipe (sem o prefixo `np:`).
fn to_win32_pipe_path(tds_pipe_name: &str) -> &str {
    tds_pipe_name.strip_prefix("np:").unwrap_or(tds_pipe_name)
}

async fn open_pipe(pipe_name: &str) -> Result<NamedPipeClient> {
    let path = to_win32_pipe_path(pipe_name);

    for _ in 0..PIPE_BUSY_MAX_ATTEMPTS {
        match ClientOptions::new().open(path) {
            Ok(client) => return Ok(client),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                tokio::time::sleep(PIPE_BUSY_RETRY_DELAY).await;
            }
            Err(e) => {
                return Err(SqlError::PipeConnect {
                    pipe: path.to_string(),
                    source: e,
                });
            }
        }
    }

    Err(SqlError::PipeConnect {
        pipe: path.to_string(),
        source: std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "pipe ocupado (ERROR_PIPE_BUSY) após retries",
        ),
    })
}

pub async fn connect(pipe_name: &str, database: Option<&str>) -> Result<TdsClient> {
    let pipe = open_pipe(pipe_name).await?;

    let mut config = Config::new();
    config.authentication(AuthMethod::Integrated);
    config.trust_cert();
    // LocalDB via named pipe é um canal local já confiado pelo SO (ACL do
    // próprio pipe); TLS na camada TDS não é suportado nesse cenário e a
    // negociação padrão (`Required`) falha com "No process is on the other
    // end of the pipe".
    config.encryption(tiberius::EncryptionLevel::NotSupported);
    if let Some(db) = database {
        config.database(db);
    }

    let client = Client::connect(config, pipe.compat_write()).await?;
    Ok(client)
}

/// Separa um script T-SQL em batches por `GO` em linha própria (case
/// insensitive), como o SSMS/sqlcmd fazem.
pub fn split_batches(script: &str) -> Vec<String> {
    let mut batches = Vec::new();
    let mut current = String::new();

    for line in script.lines() {
        if is_go_separator(line) {
            if !current.trim().is_empty() {
                batches.push(std::mem::take(&mut current));
            }
            current.clear();
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }

    if !current.trim().is_empty() {
        batches.push(current);
    }

    batches
}

fn is_go_separator(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("go")
}

#[derive(Debug, Serialize)]
pub struct BatchResult {
    pub index: usize,
    pub messages: Vec<String>,
    pub rows_affected: Option<u64>,
    pub error: Option<String>,
}

fn destructive_batch_indices(batches: &[String]) -> Vec<usize> {
    batches
        .iter()
        .enumerate()
        .filter(|(_, b)| security::classify(b) == RiskLevel::Destructive)
        .map(|(i, _)| i)
        .collect()
}

/// Roda um script multi-batch na mesma conexão/sessão (necessário pra `GO`
/// se comportar como no SSMS, com `SET` de sessão persistindo entre
/// batches). Se qualquer batch for destrutivo e `confirm != true`, nada é
/// executado.
///
/// `messages` fica sempre vazio nesta versão: capturar mensagens `PRINT`
/// exige lidar com tokens de baixo nível do stream TDS que o tiberius não
/// expõe por API pública — ver docs/PLANNING.md Fase 2.
pub async fn execute_script(
    pipe_name: &str,
    database: &str,
    script: &str,
    confirm: bool,
) -> Result<Vec<BatchResult>> {
    let batches = split_batches(script);

    let destructive = destructive_batch_indices(&batches);
    if !destructive.is_empty() && !confirm {
        return Err(SqlError::ConfirmationRequired(destructive));
    }

    let mut client = connect(pipe_name, Some(database)).await?;
    let mut results = Vec::with_capacity(batches.len());

    for (index, batch) in batches.iter().enumerate() {
        let result = client.execute(batch.as_str(), &[]).await;
        results.push(match result {
            Ok(exec_result) => BatchResult {
                index,
                messages: Vec::new(),
                rows_affected: Some(exec_result.total()),
                error: None,
            },
            Err(e) => BatchResult {
                index,
                messages: Vec::new(),
                rows_affected: None,
                error: Some(e.to_string()),
            },
        });
    }

    Ok(results)
}

/// Statement único DML/DDL. Mesmo guard de confirmação de `execute_script`.
pub async fn execute_statement(
    pipe_name: &str,
    database: &str,
    sql: &str,
    confirm: bool,
) -> Result<BatchResult> {
    if security::classify(sql) == RiskLevel::Destructive && !confirm {
        return Err(SqlError::ConfirmationRequired(vec![0]));
    }

    let mut client = connect(pipe_name, Some(database)).await?;
    let exec_result = client.execute(sql, &[]).await?;

    Ok(BatchResult {
        index: 0,
        messages: Vec::new(),
        rows_affected: Some(exec_result.total()),
        error: None,
    })
}

#[derive(Debug, Serialize)]
pub struct QueryColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<Value>>,
    pub truncated: bool,
}

/// Só aceita statements classificados `ReadOnly` (ver `security::classify`).
pub async fn execute_query(
    pipe_name: &str,
    database: &str,
    sql: &str,
    max_rows: usize,
) -> Result<QueryResult> {
    if security::classify(sql) != RiskLevel::ReadOnly {
        return Err(SqlError::NotReadOnly);
    }

    let mut client = connect(pipe_name, Some(database)).await?;
    let stream = client.simple_query(sql).await?;
    let result_sets = stream.into_results().await?;

    let first_set = result_sets.into_iter().next().unwrap_or_default();

    let columns = first_set
        .first()
        .map(|row| {
            row.columns()
                .iter()
                .map(|c| QueryColumn {
                    name: c.name().to_string(),
                    type_name: format!("{:?}", c.column_type()),
                })
                .collect()
        })
        .unwrap_or_default();

    let truncated = first_set.len() > max_rows;
    let rows = first_set.iter().take(max_rows).map(row_to_json).collect();

    Ok(QueryResult {
        columns,
        rows,
        truncated,
    })
}

fn row_to_json(row: &tiberius::Row) -> Vec<Value> {
    (0..row.columns().len())
        .map(|idx| cell_to_json(row, idx))
        .collect()
}

fn cell_to_json(row: &tiberius::Row, idx: usize) -> Value {
    use tiberius::ColumnType::*;

    let column_type = row.columns()[idx].column_type();

    match column_type {
        Null => Value::Null,
        Bit | Bitn => opt_to_json(row.try_get::<bool, _>(idx)),
        Int1 => opt_to_json(row.try_get::<u8, _>(idx)),
        Int2 => opt_to_json(row.try_get::<i16, _>(idx)),
        Int4 | Intn => opt_to_json(row.try_get::<i32, _>(idx)),
        Int8 => opt_to_json(row.try_get::<i64, _>(idx)),
        Float4 => opt_to_json(row.try_get::<f32, _>(idx)),
        Float8 | Floatn => opt_to_json(row.try_get::<f64, _>(idx)),
        Money | Money4 | Decimaln | Numericn => row
            .try_get::<tiberius::numeric::Numeric, _>(idx)
            .ok()
            .flatten()
            .map(|n| json!(n.to_string()))
            .unwrap_or(Value::Null),
        Guid => row
            .try_get::<tiberius::Uuid, _>(idx)
            .ok()
            .flatten()
            .map(|u| json!(u.to_string()))
            .unwrap_or(Value::Null),
        BigVarBin | BigBinary | Image => row
            .try_get::<&[u8], _>(idx)
            .ok()
            .flatten()
            .map(|b| json!(base64::engine::general_purpose::STANDARD.encode(b)))
            .unwrap_or(Value::Null),
        Datetime | Datetime4 | Datetimen | Datetime2 => row
            .try_get::<chrono::NaiveDateTime, _>(idx)
            .ok()
            .flatten()
            .map(|d| json!(d.to_string()))
            .unwrap_or(Value::Null),
        Daten => row
            .try_get::<chrono::NaiveDate, _>(idx)
            .ok()
            .flatten()
            .map(|d| json!(d.to_string()))
            .unwrap_or(Value::Null),
        Timen => row
            .try_get::<chrono::NaiveTime, _>(idx)
            .ok()
            .flatten()
            .map(|d| json!(d.to_string()))
            .unwrap_or(Value::Null),
        DatetimeOffsetn => row
            .try_get::<chrono::DateTime<chrono::Utc>, _>(idx)
            .ok()
            .flatten()
            .map(|d| json!(d.to_rfc3339()))
            .unwrap_or(Value::Null),
        _ => row
            .try_get::<&str, _>(idx)
            .ok()
            .flatten()
            .map(|s| json!(s))
            .unwrap_or(Value::Null),
    }
}

fn opt_to_json<T: serde::Serialize>(value: tiberius::Result<Option<T>>) -> Value {
    value
        .ok()
        .flatten()
        .map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_batches_by_go() {
        let script = "SELECT 1\nGO\nSELECT 2\nGO\n";
        let batches = split_batches(script);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].trim(), "SELECT 1");
        assert_eq!(batches[1].trim(), "SELECT 2");
    }

    #[test]
    fn script_without_go_is_single_batch() {
        let batches = split_batches("SELECT 1");
        assert_eq!(batches.len(), 1);
    }

    #[test]
    fn go_is_case_insensitive_and_trims_whitespace() {
        let batches = split_batches("SELECT 1\n  Go  \nSELECT 2");
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn strips_np_prefix_from_pipe_name() {
        assert_eq!(
            to_win32_pipe_path(r"np:\\.\pipe\LOCALDB#ABC\tsql\query"),
            r"\\.\pipe\LOCALDB#ABC\tsql\query"
        );
    }
}
