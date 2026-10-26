//! Persistent, read-only structural project index for DUDE.
use ignore::{WalkBuilder, gitignore::{Gitignore, GitignoreBuilder}};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, hash::{Hash, Hasher}, path::{Component, Path}, time::{SystemTime, UNIX_EPOCH}};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_FILES: usize = 100_000;
const INDEX_FILE: &str = "index.json";
const INDEX_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct IndexedSymbol {
    name: String,
    kind: String,
    line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct IndexedFile {
    path: String,
    bytes: u64,
    modified_nanos: u128,
    fingerprint: String,
    language: String,
    symbols: Vec<String>,
    // V1 indices are readable until the next refresh migrates them.
    #[serde(default)]
    declarations: Vec<IndexedSymbol>,
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
fn symbols(content: &str) -> (Vec<String>, Vec<IndexedSymbol>) {
    let prefixes = ["pub fn ","async fn ","fn ","pub struct ","struct ","pub enum ","enum ",
        "pub trait ","trait ","pub type ","type ","pub const ","const ","pub static ","static ",
        "class ","interface ","def ","function ","export function ","export class ",
        "export interface ","export type ","func ","CREATE TABLE ","CREATE VIEW "];
    let mut labels = Vec::new();
    let mut declarations = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        for prefix in prefixes {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
                if !name.is_empty() {
                    labels.push(format!("{prefix}{name}"));
                    let kind = if prefix.contains("fn ") || matches!(prefix, "def " | "func " | "function " | "export function ") {
                        "function"
                    } else if prefix.contains("struct ") || prefix.contains("class ") {
                        "struct"
                    } else if prefix.contains("enum ") { "enum" }
                    else if prefix.contains("trait ") || prefix.contains("interface ") { "interface" }
                    else if prefix.contains("TABLE ") || prefix.contains("VIEW ") { "sql" }
                    else { "declaration" };
                    declarations.push(IndexedSymbol { name, kind: kind.into(), line: index + 1 });
                }
                break;
            }
        }
        if labels.len() >= 100 { break; }
    }
    (labels, declarations)
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
/// A bounded index lookup: navigation hints, never verified source evidence.
pub fn search(workspace: &Path, query: &str, limit: usize) -> Result<String, String> {
    let root = workspace.canonicalize().map_err(|e| e.to_string())?;
    reject_symlink(&root.join(".dude"))?;
    let index = load(&root.join(".dude").join(INDEX_FILE))?;
    if index.version != INDEX_VERSION || index.workspace != root.to_string_lossy()
        || index.files.is_empty() {
        return Err("No current structural index for this workspace. Use /index start".into());
    }
    let terms: Vec<String> = query.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| w.chars().count() >= 3).map(str::to_lowercase)
        .filter(|w| !matches!(w.as_str(), "que" | "para" | "com" | "dos" | "das" |
            "the" | "and" | "from" | "como" | "uma" | "por" | "seja" | "cite" |
            "analise" | "análise" | "projeto" | "atual"))
        .take(24).collect();
    let mut ranked: Vec<(i32, &str, &IndexedFile)> = Vec::new();
    for (path, file) in &index.files {
        let relative = Path::new(path);
        if file.path.as_str() != path.as_str() || relative.components().any(|c| !matches!(c, Component::Normal(_)))
            || excluded(relative) { continue; }
        let path_lower = path.to_lowercase();
        let names = file.declarations.iter().map(|s| s.name.as_str())
            .collect::<Vec<_>>().join(" ").to_lowercase();
        let mut score = if path == "src/main.rs" { 9 }
            else if path.starts_with("src/") { 3 } else { 0 };
        if matches!(path.as_str(), "src/router.rs" | "src/roles.rs" | "src/providers.rs" |
            "src/investigation.rs" | "src/evidence.rs" | "src/file_tools.rs") { score += 2; }
        if file.language == "documentation" { score -= 2; }
        for term in &terms {
            if path_lower.contains(term.as_str()) { score += 12; }
            if names.contains(term.as_str()) { score += 7; }
            if file.language.contains(term.as_str()) { score += 2; }
        }
        ranked.push((score, path, file));
    }
    ranked.sort_by(|a,b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    let mut result = String::from("STRUCTURAL INDEX NAVIGATION (metadata only; not source evidence):\n");
    for (_, path, file) in ranked.into_iter().take(limit.clamp(1, 12)) {
        let matches: Vec<&IndexedSymbol> = file.declarations.iter()
            .filter(|d| terms.iter().any(|t| d.name.to_lowercase().contains(t)))
            .take(3).collect();
        let selected: Vec<&IndexedSymbol> = if matches.is_empty() {
            file.declarations.iter().take(2).collect()
        } else { matches };
        let display = selected.iter().map(|d| format!("{} {}:{}", d.kind, d.name, d.line))
            .collect::<Vec<_>>().join(", ");
        result.push_str(&format!("- {path} [{}]{}\n", file.language,
            if display.is_empty() { String::new() } else { format!(" — {display}") }));
    }
    result.push_str("Index paths and declaration lines are navigation hints only. Use native read_file on relevant paths to verify current contents and obtain citeable evidence.");
    Ok(result)
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
    // Honor local .gitignore files even for workspaces that are not themselves
    // git repositories (common while testing or indexing unpacked sources).
    walker.hidden(false).git_ignore(true).git_exclude(true).parents(true)
        .follow_links(false).require_git(false);
    // IMPORTANT: check each directory BEFORE descending into it. Merely
    // skipping ignored files in the iteration still traverses a large ignored
    // tree, making indexing slow and potentially inspecting private paths.
    let filter_root = root.clone();
    walker.filter_entry(move |entry| {
        if entry.depth() == 0 { return true; }
        let path = entry.path();
        let relative = match path.strip_prefix(&filter_root) {
            Ok(relative) => relative,
            Err(_) => return false,
        };
        if excluded(relative) { return false; }
        let Some(kind) = entry.file_type() else { return false };
        if kind.is_symlink() { return false; }
        !rules.matched_path_or_any_parents(path, kind.is_dir()).is_ignore()
    });
    for entry in walker.build() {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        let path = entry.path();
        if path == root { continue; }
        let Some(kind) = entry.file_type() else { continue };
        if kind.is_dir() || !kind.is_file() { continue; }
        let relative = path.strip_prefix(&root).unwrap_or(path);
        if next.len() >= MAX_FILES { capped = true; break; }
        let meta = match fs::metadata(path) { Ok(m) => m, Err(_) => continue };
        if meta.len() > MAX_FILE_BYTES { skipped_large += 1; continue; }
        let modified_nanos = meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos()).unwrap_or(0);
        let rel = relative.to_string_lossy().replace('\\', "/");
        if old.version == INDEX_VERSION
            && let Some(previous) = old.files.get(&rel)
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
        let (symbols, declarations) = symbols(content);
        next.insert(rel.clone(), IndexedFile { path: rel, bytes: meta.len(), modified_nanos,
            fingerprint, language: language(path), symbols, declarations });
    }
    if capped {
        return Err(format!("Workspace exceeds the {MAX_FILES}-file index limit; refusing to replace the saved index with an incomplete scan."));
    }
    let added = next.keys().filter(|p| !old.files.contains_key(*p)).count();
    let updated = next.iter().filter(|(p,f)| old.files.get(*p).is_some_and(|prior| prior != *f)).count();
    let removed = old.files.keys().filter(|p| !next.contains_key(*p)).count();
    let changed = added + updated + removed > 0 || old.version != INDEX_VERSION || old.workspace != root.to_string_lossy().to_string();
    old.version = INDEX_VERSION;
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
        assert_eq!(saved.version, INDEX_VERSION);
        assert_eq!(saved.files["src/lib.rs"].declarations[0],
            IndexedSymbol { name: "hello".into(), kind: "function".into(), line: 1 });
        assert_eq!(saved.files["src/lib.rs"].declarations[1].line, 2);
        let nav = search(&root,"hello",5).unwrap();
        assert!(nav.contains("src/lib.rs"));
        assert!(nav.contains("function hello:1"));
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn respects_gitignore_and_workspace_dudeignore() {
        let root = std::env::temp_dir().join(format!("dude-index-ignore-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("visible.rs"), "pub fn visible() {}").unwrap();
        fs::write(root.join("ignored.rs"), "pub fn hidden() {}").unwrap();
        fs::write(root.join("private.rs"), "pub fn sensitive() {}").unwrap();
        fs::write(root.join(".gitignore"), "ignored.rs\n").unwrap();
        fs::write(root.join(".dudeignore"), "private.rs\n").unwrap();
        refresh(&root).unwrap();
        let data = load(&root.join(".dude/index.json")).unwrap();
        assert!(data.files.contains_key("visible.rs"));
        assert!(!data.files.contains_key("ignored.rs"));
        assert!(!data.files.contains_key("private.rs"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dudeignore_prunes_nested_directories_and_removes_previous_entries() {
        let root = std::env::temp_dir().join(format!(
            "dude-index-ignore-dirs-{}",
            std::process::id(),
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("generated/deep")).unwrap();
        fs::create_dir_all(root.join("src/private")).unwrap();
        fs::create_dir_all(root.join("src/public")).unwrap();
        fs::write(root.join("generated/deep/secret.rs"), "fn secret() {}").unwrap();
        fs::write(root.join("src/private/internal.rs"), "fn internal() {}").unwrap();
        fs::write(root.join("src/public/visible.rs"), "fn visible() {}").unwrap();
        fs::write(root.join("debug.log"), "debug trace").unwrap();

        let first = refresh(&root).unwrap();
        assert!(first.message.contains("4 files"));
        fs::write(root.join(".dudeignore"), "generated/\nsrc/private/\n*.log\n").unwrap();

        // A new scan must prune ignored directories and remove anything that
        // was indexed before the exclusion rules were introduced.
        let updated = refresh(&root).unwrap();
        assert!(updated.changed);
        assert!(updated.message.contains("removed 3"));
        let saved = load(&root.join(".dude/index.json")).unwrap();
        assert_eq!(saved.files.len(), 1);
        assert!(saved.files.contains_key("src/public/visible.rs"));
        assert!(!saved.files.contains_key("generated/deep/secret.rs"));
        assert!(!saved.files.contains_key("src/private/internal.rs"));
        assert!(!saved.files.contains_key("debug.log"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dudeignore_can_exclude_nested_directory_by_root_pattern() {
        let root = std::env::temp_dir().join(format!(
            "dude-index-ignore-root-{}",
            std::process::id(),
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs/private")).unwrap();
        fs::create_dir_all(root.join("docs/public")).unwrap();
        fs::write(root.join("docs/private/secret.md"), "sensitive").unwrap();
        fs::write(root.join("docs/public/readme.md"), "public").unwrap();
        fs::write(root.join(".dudeignore"), "/docs/private/\n").unwrap();
        refresh(&root).unwrap();
        let saved = load(&root.join(".dude/index.json")).unwrap();
        assert!(saved.files.contains_key("docs/public/readme.md"));
        assert!(!saved.files.contains_key("docs/private/secret.md"));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_persistent_index_state_outside_workspace_via_symlink() {
        use std::os::unix::fs::symlink;
        let dir = std::env::temp_dir().join(format!("dude-index-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("workspace")).unwrap();
        fs::create_dir_all(dir.join("external")).unwrap();
        symlink(dir.join("external"), dir.join("workspace/.dude")).unwrap();
        assert!(refresh(&dir.join("workspace")).is_err());
        assert!(status(&dir.join("workspace")).is_err());
        assert!(!dir.join("external/index.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn v1_index_migrates_and_does_not_serve_stale_navigation() {
        let root = std::env::temp_dir().join(format!("dude-index-v1-{}",std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".dude")).unwrap();
        fs::write(root.join("file.rs"), "pub fn migrated() {}\n").unwrap();
        let old = serde_json::json!({
            "version":1, "workspace":root.to_string_lossy(), "updated_unix":1,
            "files":{"file.rs":{"path":"file.rs","bytes":0,"modified_nanos":0,
                "fingerprint":"old","language":"rust","symbols":["pub fnmigrated"]}}
        });
        fs::write(root.join(".dude/index.json"),old.to_string()).unwrap();
        assert!(load(&root.join(".dude/index.json")).unwrap().files["file.rs"].declarations.is_empty());
        assert!(search(&root,"migrated",5).is_err());
        refresh(&root).unwrap();
        let updated = load(&root.join(".dude/index.json")).unwrap();
        assert_eq!(updated.files["file.rs"].declarations[0].name, "migrated");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn index_hints_are_not_source_evidence_and_are_workspace_scoped() {
        let base = std::env::temp_dir().join(format!("dude-index-nav-{}",std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let a = base.join("a");
        let b = base.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(b.join(".dude")).unwrap();
        fs::write(a.join("worker.rs"), "fn process_order() { /* PRIVATE_BODY */ }\n").unwrap();
        refresh(&a).unwrap();
        let hints = search(&a,"process_order",4).unwrap();
        assert!(hints.contains("worker.rs"));
        assert!(hints.contains("function process_order:1"));
        assert!(!hints.contains("PRIVATE_BODY"));
        fs::copy(a.join(".dude/index.json"),b.join(".dude/index.json")).unwrap();
        assert!(search(&b,"process_order",4).is_err());
        let _ = fs::remove_dir_all(&base);
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
