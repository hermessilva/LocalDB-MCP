use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("path outside the roots allowed in config.scan_allowlist")]
    PathNotAllowed,
    #[error("config.scan_allowlist is empty — configure at least one root before using this tool")]
    AllowlistEmpty,
    #[error("I/O error validating path: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    ReadOnly,
    Destructive,
}

/// Keywords that classify a statement as destructive. v1: keyword matching
/// over the comment-stripped text — covers the common cases, but has a
/// known false negative for obfuscated dynamic SQL inside a string
/// (`EXEC('DELETE FROM...')`). Migrating to `sqlparser-rs` (real AST)
/// closes that gap — see docs/PLANNING.md Phase 2.
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

/// Classifies a T-SQL snippet as `ReadOnly` or `Destructive`.
///
/// `CREATE DATABASE ... FOR ATTACH` is treated as non-destructive (it's
/// additive, doesn't overwrite anything existing) even though it contains
/// the word `CREATE`.
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

/// Validates that `path` is contained within some root of `allowlist`
/// (after canonicalizing both). Never follows a link outside the allowlist.
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
        assert_eq!(classify("SELECT * FROM Orders"), RiskLevel::ReadOnly);
    }

    #[test]
    fn delete_is_destructive() {
        assert_eq!(
            classify("DELETE FROM Orders WHERE Id = 1"),
            RiskLevel::Destructive
        );
    }

    #[test]
    fn destructive_keyword_inside_comment_is_ignored() {
        assert_eq!(
            classify("-- DROP TABLE Orders\nSELECT 1"),
            RiskLevel::ReadOnly
        );
    }

    #[test]
    fn create_database_for_attach_is_read_only() {
        assert_eq!(
            classify("CREATE DATABASE [client] ON (FILENAME = 'c:\\client.mdf') FOR ATTACH"),
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
            classify("CREATE TABLE Orders (Id INT)"),
            RiskLevel::Destructive
        );
    }

    #[test]
    fn drop_database_is_destructive() {
        assert_eq!(classify("DROP DATABASE [client]"), RiskLevel::Destructive);
    }
}
