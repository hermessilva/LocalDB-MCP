use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("path fora das raízes permitidas em config.scan_allowlist")]
    PathNotAllowed,
    #[error(
        "config.scan_allowlist está vazio — configure ao menos uma raiz antes de usar esta ferramenta"
    )]
    AllowlistEmpty,
    #[error("erro de E/S ao validar path: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    ReadOnly,
    Destructive,
}

/// Palavras-chave que classificam um statement como destrutivo. v1: keyword
/// matching sobre o texto sem comentários — cobre os casos comuns, mas tem
/// falso-negativo conhecido para SQL dinâmico ofuscado em string
/// (`EXEC('DELETE FROM...')`). Migrar para `sqlparser-rs` (AST real) fecha
/// esse gap — ver docs/PLANNING.md Fase 2.
const DESTRUCTIVE_KEYWORDS: &[&str] = &[
    "DROP", "TRUNCATE", "ALTER", "DELETE", "UPDATE", "INSERT", "MERGE", "GRANT", "REVOKE", "DENY",
    "EXEC", "EXECUTE", "CREATE",
];

static LINE_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--[^\n]*").unwrap());
static BLOCK_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/").unwrap());
static FOR_ATTACH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)CREATE\s+DATABASE\b.*\bFOR\s+ATTACH\b").unwrap());
static WORD_BOUNDARY: LazyLock<Regex> = LazyLock::new(|| {
    let alternation = DESTRUCTIVE_KEYWORDS.join("|");
    Regex::new(&format!(r"(?i)\b({alternation})\b")).unwrap()
});

fn strip_comments(sql: &str) -> String {
    let no_line = LINE_COMMENT.replace_all(sql, "");
    BLOCK_COMMENT.replace_all(&no_line, "").into_owned()
}

/// Classifica um trecho de T-SQL como `ReadOnly` ou `Destructive`.
///
/// `CREATE DATABASE ... FOR ATTACH` é tratado como não-destrutivo (é
/// aditivo, não sobrescreve nada existente) mesmo contendo a palavra
/// `CREATE`.
pub fn classify(sql: &str) -> RiskLevel {
    let cleaned = strip_comments(sql);

    if FOR_ATTACH.is_match(&cleaned) {
        return RiskLevel::ReadOnly;
    }

    if WORD_BOUNDARY.is_match(&cleaned) {
        RiskLevel::Destructive
    } else {
        RiskLevel::ReadOnly
    }
}

/// Valida que `path` está contido em alguma raiz de `allowlist` (após
/// canonicalização de ambos). Nunca segue link fora da allowlist.
pub fn validate_path(path: &Path, allowlist: &[PathBuf]) -> Result<PathBuf, SecurityError> {
    if allowlist.is_empty() {
        return Err(SecurityError::AllowlistEmpty);
    }

    let canonical = path.canonicalize()?;

    for root in allowlist {
        let Ok(root_canonical) = root.canonicalize() else {
            continue;
        };
        if canonical.starts_with(&root_canonical) {
            return Ok(canonical);
        }
    }

    Err(SecurityError::PathNotAllowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_is_read_only() {
        assert_eq!(classify("SELECT * FROM Pedidos"), RiskLevel::ReadOnly);
    }

    #[test]
    fn delete_is_destructive() {
        assert_eq!(
            classify("DELETE FROM Pedidos WHERE Id = 1"),
            RiskLevel::Destructive
        );
    }

    #[test]
    fn destructive_keyword_inside_comment_is_ignored() {
        assert_eq!(
            classify("-- DROP TABLE Pedidos\nSELECT 1"),
            RiskLevel::ReadOnly
        );
    }

    #[test]
    fn create_database_for_attach_is_read_only() {
        assert_eq!(
            classify("CREATE DATABASE [cliente] ON (FILENAME = 'c:\\cliente.mdf') FOR ATTACH"),
            RiskLevel::ReadOnly
        );
    }

    #[test]
    fn create_database_without_attach_is_destructive() {
        assert_eq!(classify("CREATE DATABASE ScanTest"), RiskLevel::Destructive);
    }

    #[test]
    fn create_table_is_destructive() {
        assert_eq!(
            classify("CREATE TABLE Pedidos (Id INT)"),
            RiskLevel::Destructive
        );
    }

    #[test]
    fn drop_database_is_destructive() {
        assert_eq!(classify("DROP DATABASE [cliente]"), RiskLevel::Destructive);
    }
}
