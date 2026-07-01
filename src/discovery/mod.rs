use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Mdf,
    Ldf,
}

#[derive(Debug, Clone, Serialize)]
pub struct FoundDatabase {
    pub path: PathBuf,
    pub kind: FileKind,
    pub size_bytes: u64,
    pub modified_at: DateTime<Utc>,
    pub already_attached: bool,
    pub likely_database_name: Option<String>,
}

/// Pastas que nunca compensa varrer — custo alto, ruído garantido.
const IGNORED_DIR_NAMES: &[&str] = &["node_modules", ".git", "bin", "obj"];

fn is_ignored_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .map(|name| IGNORED_DIR_NAMES.iter().any(|ignored| ignored.eq_ignore_ascii_case(name)))
            .unwrap_or(false)
}

fn classify_extension(path: &Path) -> Option<FileKind> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("mdf") => Some(FileKind::Mdf),
        Some(ext) if ext.eq_ignore_ascii_case("ldf") => Some(FileKind::Ldf),
        _ => None,
    }
}

/// Varre `root` (já validado contra a allowlist pelo chamador — ver
/// `security::validate_path`) em busca de `.mdf`/`.ldf` soltos.
///
/// `attached_paths` é o conjunto de paths físicos já anexados conhecidos
/// pelo chamador. Em Fase 1, antes de `db_list_attached` existir, esse
/// conjunto normalmente vem vazio — `already_attached` fica `false` até essa
/// tool cruzar a informação de verdade (ver docs/PLANNING.md Fase 2).
///
/// Entradas inacessíveis (permissão negada, link quebrado etc.) são
/// puladas silenciosamente — comum em pastas de sistema misturadas na
/// árvore (`AppData` e afins), não pode abortar o scan inteiro.
pub fn scan_folder(root: &Path, max_depth: usize, attached_paths: &HashSet<PathBuf>) -> Vec<FoundDatabase> {
    let mut found = Vec::new();

    let walker = WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e));

    for entry in walker {
        // Uma subpasta sem permissão (comum em `AppData`, pastas de sistema
        // misturadas na árvore, etc.) não pode abortar o scan inteiro — só
        // pula essa entrada e segue.
        let Ok(entry) = entry else {
            continue;
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let Some(kind) = classify_extension(entry.path()) else {
            continue;
        };

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        let modified_at: DateTime<Utc> = metadata.modified().map(DateTime::from).unwrap_or_else(|_| Utc::now());

        let path = entry.path().to_path_buf();
        let likely_database_name = path.file_stem().and_then(|s| s.to_str()).map(String::from);
        let already_attached = attached_paths.contains(&path);

        found.push(FoundDatabase {
            path,
            kind,
            size_bytes: metadata.len(),
            modified_at,
            already_attached,
            likely_database_name,
        });
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_loose_mdf_and_ignores_node_modules() {
        let dir = std::env::temp_dir().join(format!("mssql-localdb-mcp-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("node_modules")).unwrap();
        fs::write(dir.join("cliente.mdf"), b"x").unwrap();
        fs::write(dir.join("cliente_log.ldf"), b"x").unwrap();
        fs::write(dir.join("node_modules").join("ignored.mdf"), b"x").unwrap();

        let found = scan_folder(&dir, 8, &HashSet::new());

        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|f| f.kind == FileKind::Mdf));
        assert!(found.iter().any(|f| f.kind == FileKind::Ldf));
        assert!(found.iter().all(|f| !f.path.to_string_lossy().contains("node_modules")));
    }
}
