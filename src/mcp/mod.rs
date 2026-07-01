use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::Config;
use crate::{discovery, localdb, security, sql};

fn default_instance() -> String {
    "MSSQLLocalDB".to_string()
}

fn ok_json<T: serde::Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string(value)
        .map_err(|e| McpError::internal_error(format!("falha ao serializar resultado: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn confirmation_required(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), Some(json!({"code": "CONFIRMATION_REQUIRED"})))
}

fn localdb_error(e: localdb::LocalDbError) -> McpError {
    McpError::internal_error(e.to_string(), Some(json!({"code": "INSTANCE_NOT_FOUND"})))
}

fn sql_error(e: sql::SqlError) -> McpError {
    match e {
        sql::SqlError::ConfirmationRequired(batches) => McpError::invalid_params(
            "ação destrutiva requer confirm=true",
            Some(json!({"code": "CONFIRMATION_REQUIRED", "destructive_batches": batches})),
        ),
        sql::SqlError::NotReadOnly => McpError::invalid_params(
            "statement não é somente leitura — use sql_execute_statement/sql_execute_script",
            Some(json!({"code": "NOT_READ_ONLY"})),
        ),
        other => McpError::internal_error(other.to_string(), Some(json!({"code": "SQL_ERROR"}))),
    }
}

fn security_error(e: security::SecurityError) -> McpError {
    match e {
        security::SecurityError::AllowlistEmpty => {
            McpError::invalid_params(e.to_string(), Some(json!({"code": "CONFIG_MISSING"})))
        }
        security::SecurityError::PathNotAllowed => McpError::invalid_params(
            "path fora das raízes permitidas em config.scan_allowlist",
            Some(json!({"code": "PATH_NOT_ALLOWED"})),
        ),
        security::SecurityError::Io(err) => McpError::internal_error(err.to_string(), None),
    }
}

/// Escapa aspas simples pra uso dentro de um literal de string T-SQL
/// (`N'...'`). Necessário porque paths e nomes de banco vindos do agente
/// são interpolados em DDL gerado aqui (`db_attach`/`db_detach`).
fn escape_sql_literal(s: &str) -> String {
    s.replace('\'', "''")
}

/// Escapa `]` pra uso dentro de um identificador T-SQL entre colchetes
/// (`[...]`).
fn escape_identifier(s: &str) -> String {
    s.replace(']', "]]")
}

#[derive(Debug, Deserialize, JsonSchema)]
struct InstanceArgs {
    instance: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateInstanceArgs {
    instance: String,
    version: Option<String>,
    start: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct StopInstanceArgs {
    instance: String,
    kill: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DeleteInstanceArgs {
    instance: String,
    confirm: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ExecuteScriptArgs {
    instance: Option<String>,
    database: String,
    script: String,
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ExecuteQueryArgs {
    instance: Option<String>,
    database: String,
    sql: String,
    max_rows: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ExecuteStatementArgs {
    instance: Option<String>,
    database: String,
    sql: String,
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ScanFolderArgs {
    root: String,
    max_depth: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AttachArgs {
    instance: Option<String>,
    mdf_path: String,
    database_name: Option<String>,
    ldf_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DetachArgs {
    instance: Option<String>,
    database: String,
    confirm: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListTablesArgs {
    instance: Option<String>,
    database: String,
    schema: Option<String>,
}

#[derive(Clone)]
pub struct McpServer {
    config: Arc<Config>,
    // Lido pelo código gerado por #[tool_handler]; o analisador de
    // dead-code não enxerga esse uso através da expansão da macro.
    #[allow(dead_code)]
    tool_router: ToolRouter<McpServer>,
}

#[tool_router]
impl McpServer {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            tool_router: Self::tool_router(),
        }
    }

    // --- Grupo A: gestão de instância -----------------------------------

    #[tool(description = "Lista todas as instâncias LocalDB registradas na máquina")]
    async fn localdb_list_instances(&self) -> Result<CallToolResult, McpError> {
        let instances = localdb::list_instances().await.map_err(localdb_error)?;
        ok_json(&json!({ "instances": instances }))
    }

    #[tool(description = "Lista versões do LocalDB instaladas na máquina")]
    async fn localdb_versions(&self) -> Result<CallToolResult, McpError> {
        let versions = localdb::versions().await.map_err(localdb_error)?;
        ok_json(&json!({ "versions": versions }))
    }

    #[tool(description = "Detalhe de uma instância LocalDB: versão, estado, pipe name, owner")]
    async fn localdb_info(
        &self,
        Parameters(InstanceArgs { instance }): Parameters<InstanceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let info = localdb::info(&instance).await.map_err(localdb_error)?;
        ok_json(&info)
    }

    #[tool(description = "Cria uma nova instância LocalDB")]
    async fn localdb_create_instance(
        &self,
        Parameters(args): Parameters<CreateInstanceArgs>,
    ) -> Result<CallToolResult, McpError> {
        localdb::create(&args.instance, args.version.as_deref(), args.start.unwrap_or(false))
            .await
            .map_err(localdb_error)?;
        ok_json(&json!({ "created": true, "instance": args.instance }))
    }

    #[tool(description = "Inicia uma instância LocalDB parada")]
    async fn localdb_start_instance(
        &self,
        Parameters(InstanceArgs { instance }): Parameters<InstanceArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pipe_name = localdb::start(&instance).await.map_err(localdb_error)?;
        ok_json(&json!({ "started": true, "pipe_name": pipe_name }))
    }

    #[tool(description = "Para uma instância LocalDB rodando")]
    async fn localdb_stop_instance(
        &self,
        Parameters(args): Parameters<StopInstanceArgs>,
    ) -> Result<CallToolResult, McpError> {
        localdb::stop(&args.instance, args.kill.unwrap_or(false))
            .await
            .map_err(localdb_error)?;
        ok_json(&json!({ "stopped": true }))
    }

    #[tool(description = "Apaga uma instância LocalDB (não apaga os arquivos de banco anexados). Destrutivo — exige confirm=true")]
    async fn localdb_delete_instance(
        &self,
        Parameters(args): Parameters<DeleteInstanceArgs>,
    ) -> Result<CallToolResult, McpError> {
        if !args.confirm {
            return Err(confirmation_required("apagar a instância requer confirm=true"));
        }
        localdb::delete(&args.instance).await.map_err(localdb_error)?;
        ok_json(&json!({ "deleted": true }))
    }

    // --- Grupo B: canal de script/SQL ------------------------------------

    #[tool(description = "Roda script T-SQL multi-batch (separador GO em linha própria) na mesma sessão. Batch destrutivo exige confirm=true")]
    async fn sql_execute_script(
        &self,
        Parameters(args): Parameters<ExecuteScriptArgs>,
    ) -> Result<CallToolResult, McpError> {
        let instance = args.instance.unwrap_or_else(default_instance);
        let pipe_name = localdb::ensure_running(&instance).await.map_err(localdb_error)?;
        let batches = sql::execute_script(&pipe_name, &args.database, &args.script, args.confirm.unwrap_or(false))
            .await
            .map_err(sql_error)?;
        ok_json(&json!({ "batches": batches }))
    }

    #[tool(description = "Roda um SELECT (somente leitura) e retorna as linhas como JSON")]
    async fn sql_execute_query(
        &self,
        Parameters(args): Parameters<ExecuteQueryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let instance = args.instance.unwrap_or_else(default_instance);
        let pipe_name = localdb::ensure_running(&instance).await.map_err(localdb_error)?;
        let max_rows = args.max_rows.unwrap_or(self.config.default_max_rows);
        let result = sql::execute_query(&pipe_name, &args.database, &args.sql, max_rows)
            .await
            .map_err(sql_error)?;
        ok_json(&result)
    }

    #[tool(description = "Roda um statement DML/DDL único. Destrutivo exige confirm=true")]
    async fn sql_execute_statement(
        &self,
        Parameters(args): Parameters<ExecuteStatementArgs>,
    ) -> Result<CallToolResult, McpError> {
        let instance = args.instance.unwrap_or_else(default_instance);
        let pipe_name = localdb::ensure_running(&instance).await.map_err(localdb_error)?;
        let result = sql::execute_statement(&pipe_name, &args.database, &args.sql, args.confirm.unwrap_or(false))
            .await
            .map_err(sql_error)?;
        ok_json(&result)
    }

    // --- Grupo C: descoberta e metadata -----------------------------------

    #[tool(description = "Varre uma pasta em busca de .mdf/.ldf soltos. Restrito às raízes em config.scan_allowlist")]
    async fn db_scan_folder(
        &self,
        Parameters(args): Parameters<ScanFolderArgs>,
    ) -> Result<CallToolResult, McpError> {
        let root = PathBuf::from(&args.root);
        let validated = security::validate_path(&root, &self.config.scan_allowlist).map_err(security_error)?;
        let max_depth = args.max_depth.unwrap_or(self.config.scan_max_depth);

        let found = discovery::scan_folder(&validated, max_depth, &HashSet::new());

        ok_json(&json!({ "found": found }))
    }

    #[tool(description = "Anexa um .mdf solto (achado via db_scan_folder) como banco de dados na instância")]
    async fn db_attach(&self, Parameters(args): Parameters<AttachArgs>) -> Result<CallToolResult, McpError> {
        let mdf_path = PathBuf::from(&args.mdf_path);
        let validated_mdf =
            security::validate_path(&mdf_path, &self.config.scan_allowlist).map_err(security_error)?;

        let instance = args.instance.unwrap_or_else(default_instance);
        let pipe_name = localdb::ensure_running(&instance).await.map_err(localdb_error)?;

        let database_name = args.database_name.unwrap_or_else(|| {
            validated_mdf
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("attached_db")
                .to_string()
        });

        let mut files = format!("(FILENAME = N'{}')", escape_sql_literal(&validated_mdf.to_string_lossy()));

        if let Some(ldf) = &args.ldf_path {
            let validated_ldf = security::validate_path(&PathBuf::from(ldf), &self.config.scan_allowlist)
                .map_err(security_error)?;
            files.push_str(&format!(", (FILENAME = N'{}')", escape_sql_literal(&validated_ldf.to_string_lossy())));
        }

        let statement = format!(
            "CREATE DATABASE [{}] ON {} FOR ATTACH",
            escape_identifier(&database_name),
            files
        );

        // CREATE DATABASE ... FOR ATTACH é aditivo (security::classify já
        // trata como ReadOnly), então confirm=false é seguro aqui.
        sql::execute_statement(&pipe_name, "master", &statement, false)
            .await
            .map_err(sql_error)?;

        ok_json(&json!({ "attached": true, "database": database_name }))
    }

    #[tool(description = "Desanexa um banco de dados da instância (sp_detach_db). Destrutivo — exige confirm=true")]
    async fn db_detach(&self, Parameters(args): Parameters<DetachArgs>) -> Result<CallToolResult, McpError> {
        if !args.confirm {
            return Err(confirmation_required("desanexar banco requer confirm=true"));
        }

        let instance = args.instance.unwrap_or_else(default_instance);
        let pipe_name = localdb::ensure_running(&instance).await.map_err(localdb_error)?;

        let statement = format!("EXEC sp_detach_db N'{}'", escape_sql_literal(&args.database));
        sql::execute_statement(&pipe_name, "master", &statement, true)
            .await
            .map_err(sql_error)?;

        ok_json(&json!({ "detached": true }))
    }

    #[tool(description = "Lista tabelas do banco (via INFORMATION_SCHEMA)")]
    async fn db_list_tables(&self, Parameters(args): Parameters<ListTablesArgs>) -> Result<CallToolResult, McpError> {
        let instance = args.instance.unwrap_or_else(default_instance);
        let pipe_name = localdb::ensure_running(&instance).await.map_err(localdb_error)?;

        let sql_text = match &args.schema {
            Some(schema) => format!(
                "SELECT TABLE_SCHEMA AS [schema], TABLE_NAME AS [name] FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_TYPE = 'BASE TABLE' AND TABLE_SCHEMA = N'{}' ORDER BY TABLE_SCHEMA, TABLE_NAME",
                escape_sql_literal(schema)
            ),
            None => "SELECT TABLE_SCHEMA AS [schema], TABLE_NAME AS [name] FROM INFORMATION_SCHEMA.TABLES \
                      WHERE TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_SCHEMA, TABLE_NAME"
                .to_string(),
        };

        let result = sql::execute_query(&pipe_name, &args.database, &sql_text, 10_000)
            .await
            .map_err(sql_error)?;

        let tables: Vec<Value> = result
            .rows
            .iter()
            .map(|row| {
                json!({
                    "schema": row.first().cloned().unwrap_or(Value::Null),
                    "name": row.get(1).cloned().unwrap_or(Value::Null),
                    "row_count_estimate": Value::Null,
                })
            })
            .collect();

        ok_json(&json!({ "tables": tables }))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "Servidor MCP para SQL Server Express LocalDB no Windows. Gerencia instâncias \
                 (localdb_*), executa T-SQL (sql_*) e descobre/anexa bancos soltos em pastas \
                 (db_*). Toda ação destrutiva exige confirm=true."
                    .to_string(),
            )
    }
}
