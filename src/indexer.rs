//! Persistent, read-only structural project index for DUDE.
use ignore::{WalkBuilder, gitignore::{Gitignore, GitignoreBuilder}};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, hash::{Hash, Hasher}, path::Path, time::{SystemTime, UNIX_EPOCH}};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_FILES: usize = 100_000;
const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct IndexedFile {
    path: String,
    bytes: u64,
    modified_nanos: u128,
    fingerprint: String,
    language: String,
    symbols: Vec<String>,
}
#[derive(Debug, Default, Serialize, Deserialize)]
struct IndexData {
    version: u32,
    workspace: String,
    updated_unix: u64,
    files: BTreeMap<String, IndexedFile>,
}
pub struct RefreshReport { pub message: String, pub changed: bool }

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }

fn excluded(path: &Path) -> bool {
    path.components().any(|part| {
        let n = part.as_os_str().to_string_lossy();
        matches!(n.as_ref(), ".git" | ".dude" | ".dudeignore" | "target" | "node_modules" | "vendor" | "dist" |
            "build" | ".ssh" | ".aws" | ".config" | ".idea" | ".vscode" | "coverage" |
            "__pycache__" | ".next" | ".venv" | "venv")
            || n == ".env" || n.starts_with(".env.") || n == "id_rsa" || n == "id_ed25519"
            || n.ends_with(".pem") || n.ends_with(".key")
    })
}
fn language(path: &Path) -> String {
    match path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "rs"=>"rust", "py"=>"python", "js"|"jsx"|"mjs"|"cjs"=>"javascript", "ts"|"tsx"=>"typescript",
        "php"=>"php", "java"=>"java", "kt"=>"kotlin", "go"=>"go", "rb"=>"ruby", "cs"=>"csharp",
        "c"|"h"=>"c", "cpp"|"cc"|"cxx"|"hpp"=>"cpp", "sql"=>"sql", "sh"|"bash"=>"shell",
        "html"|"htm"=>"html", "css"|"scss"=>"css", "json"=>"json", "toml"=>"toml",
        "yaml"|"yml"=>"yaml", "md"|"rst"=>"documentation", "xml"=>"xml", "swift"=>"swift",
        "dart"=>"dart", "vue"=>"vue", "svelte"=>"svelte", "txt"=>"text", ""=>"unknown", _=>"other"
    }.to_string()
}
fn symbols(content: &str) -> Vec<String> {
    let prefixes = ["pub fn ","async fn ","fn ","pub struct ","struct ","pub enum ","enum ",
        "pub trait ","trait ","pub type ","type ","pub const ","const ","pub static ","static ",
        "class ","interface ","def ","function ","export function ","export class ",
        "export interface ","export type ","func ","CREATE TABLE ","CREATE VIEW "];
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        for prefix in prefixes {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
                if !name.is_empty() { out.push(format!("{}{}", prefix.trim(), name)); }
                break;
            }
        }
        if out.len() >= 100 { break; }
    }
    out
}
fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() =>
            Err(format!("Indexador não acessa caminhos simbólicos: {}", path.display())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Não foi possível verificar {}: {error}", path.display())),
    }
}

fn load(path: &Path) -> Result<IndexData, String> {
    reject_symlink(path)?;
    if path.exists() && fs::metadata(path).map_err(|e|e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Índice persistente excede o limite de leitura (64 MiB)".into());
    }
    if !path.exists() { return Ok(IndexData::default()); }
    serde_json::from_str(&fs::read_to_string(path).map_err(|e| format!("Cannot read index: {e}"))?)
        .map_err(|e| format!("Index JSON is invalid: {e}"))
}
pub fn status(workspace: &Path) -> Result<String, String> {
    reject_symlink(&workspace.join(".dude"))?;
    let path = workspace.join(".dude").join(INDEX_FILE);
    if !path.exists() { return Ok("No persistent index yet. Run /index start.".into()); }
    let index = load(&path)?;
    Ok(format!("Saved structural index: {} files | updated Unix timestamp: {} | version: {}",
        index.files.len(), index.updated_unix, index.version))
}
pub fn refresh(workspace: &Path) -> Result<RefreshReport, String> {
    let root = workspace.canonicalize().map_err(|e| format!("Cannot resolve workspace: {e}"))?;
    if !root.is_dir() { return Err("Workspace is not a directory".into()); }
    let state = root.join(".dude");
    reject_symlink(&state)?;
    fs::create_dir_all(&state).map_err(|e| format!("Cannot create .dude: {e}"))?;
    let index_path = state.join(INDEX_FILE);
    let mut old = load(&index_path)?;
    let mut builder = GitignoreBuilder::new(&root);
    let custom = root.join(".dudeignore");
    if custom.is_file() {
        if fs::symlink_metadata(&custom).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink .dudeignore".into());
        }
        if let Some(err) = builder.add(&custom) { return Err(format!("Invalid .dudeignore: {err}")); }
    }
    let rules: Gitignore = builder.build().map_err(|e| e.to_string())?;
    let mut next = BTreeMap::new();
    let mut skipped_large = 0usize;
    let mut capped = false;
    let mut walker = WalkBuilder::new(&root);
    walker.hidden(false).git_ignore(true).git_exclude(true).parents(true).follow_links(false);
    for entry in walker.build() {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        let path = entry.path();
        if path == root { continue; }
        let relative = path.strip_prefix(&root).unwrap_or(path);
        if excluded(relative) { continue; }
        let Some(kind) = entry.file_type() else { continue };
        if rules.matched_path_or_any_parents(path, kind.is_dir()).is_ignore() { continue; }
        if kind.is_dir() || !kind.is_file() { continue; }
        if next.len() >= MAX_FILES { capped = true; break; }
        let meta = match fs::metadata(path) { Ok(m) => m, Err(_) => continue };
        if meta.len() > MAX_FILE_BYTES { skipped_large += 1; continue; }
        let modified_nanos = meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos()).unwrap_or(0);
        let rel = relative.to_string_lossy().replace('\\', "/");
        if let Some(previous) = old.files.get(&rel)
            && previous.bytes == meta.len()
            && previous.modified_nanos == modified_nanos {
                next.insert(rel, previous.clone());
                continue;
        }
        let bytes = match fs::read(path) { Ok(b) => b, Err(_) => continue };
        if bytes.contains(&0) { continue; }
        let Ok(content) = std::str::from_utf8(&bytes) else { continue };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let fingerprint = format!("{:016x}", hasher.finish());
        next.insert(rel.clone(), IndexedFile { path: rel, bytes: meta.len(), modified_nanos,
            fingerprint, language: language(path), symbols: symbols(content) });
    }
    if capped {
        return Err(format!("Workspace exceeds the {MAX_FILES}-file index limit; refusing to replace the saved index with an incomplete scan."));
    }
    let added = next.keys().filter(|p| !old.files.contains_key(*p)).count();
    let updated = next.iter().filter(|(p,f)| old.files.get(*p).is_some_and(|prior| prior != *f)).count();
    let removed = old.files.keys().filter(|p| !next.contains_key(*p)).count();
    let changed = added + updated + removed > 0 || old.version != 1 || old.workspace != root.to_string_lossy().to_string();
    old.version = 1;
    old.workspace = root.to_string_lossy().into_owned();
    old.updated_unix = now();
    old.files = next;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let tmp = state.join(format!("index.json.{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> Result<(), String> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)
            .map_err(|e| format!("Cannot create index snapshot: {e}"))?;
        file.write_all(&serde_json::to_vec_pretty(&old).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cannot write index: {e}"))?;
        file.sync_all().map_err(|e| format!("Cannot sync index: {e}"))?;
        fs::rename(&tmp, &index_path).map_err(|e| format!("Cannot finalize index: {e}"))?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result?;
    Ok(RefreshReport { changed, message: format!(
        "Index refresh: {} files | added {added} | changed {updated} | removed {removed} | skipped oversized {skipped_large}{}",
        old.files.len(), if capped { " | LIMIT REACHED: index incomplete" } else { "" }) })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexes_symbols_and_excludes_secrets_and_generated_dirs() {
        let root = std::env::temp_dir().join(format!("dude-index-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn hello() {}\nstruct Thing;\n").unwrap();
        fs::write(root.join(".env"), "SECRET=never-index").unwrap();
        fs::write(root.join("target/generated.rs"), "fn generated() {}").unwrap();
        refresh(&root).unwrap();
        let saved = load(&root.join(".dude/index.json")).unwrap();
        assert_eq!(saved.files.len(), 1);
        assert!(saved.files["src/lib.rs"].symbols.iter().any(|s| s == "pub fn hello"));
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn detects_changes_and_deletions() {
        let root = std::env::temp_dir().join(format!("dude-index-change-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.rs"), "fn a() {}").unwrap();
        refresh(&root).unwrap();
        fs::write(root.join("a.rs"), "fn changed() {}").unwrap();
        assert!(refresh(&root).unwrap().changed);
        fs::remove_file(root.join("a.rs")).unwrap();
        assert!(refresh(&root).unwrap().message.contains("removed 1"));
        let _ = fs::remove_dir_all(&root);
    }
}
