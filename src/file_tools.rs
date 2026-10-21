use std::{fs, path::{Path, PathBuf}};
use serde_json::{json, Value};

const MAX_BYTES: usize = 32_768;
const MAX_ENTRIES: usize = 150;

pub fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"list_files","description":"List files in a workspace directory (read-only).","parameters":{"type":"object","properties":{"path":{"type":"string","description":"Relative directory path, default ."}}}}}),
        json!({"type":"function","function":{"name":"read_file","description":"Read a text file, optionally limited to line range. Similar to sed -n, but read-only.","parameters":{"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer"},"end_line":{"type":"integer"}},"required":["path"]}}}),
        json!({"type":"function","function":{"name":"project_map","description":"Read-only compact repository map: paths and approximate Rust, PHP, Java, C++ and Python declarations; use before reading many source files.","parameters":{"type":"object","properties":{"path":{"type":"string","description":"Relative directory path (default .)"}}}}}),
        json!({"type":"function","function":{"name":"search_text","description":"Search text in workspace files, like grep -n. No regex; case-sensitive literal search.","parameters":{"type":"object","properties":{"path":{"type":"string","description":"File or directory relative path, default ."},"pattern":{"type":"string"}},"required":["pattern"]}}})
    ]
}

fn resolve(root: &Path, relative: &str) -> Result<PathBuf,String> {
    let p = Path::new(relative);
    if p.is_absolute() { return Err("Absolute paths are disabled".into()); }
    if p.components().any(|c| {
        let v = c.as_os_str().to_string_lossy();
        v == ".env" || v.starts_with(".env.") || v == ".dude" || v == ".git" || v == ".ssh" ||
        v == ".aws" || v == ".config" || v == "id_rsa" || v == "id_ed25519"
    }) {
        return Err("Sensitive paths are blocked from AI tools".into());
    }
    let target = root.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(root) { return Err("Path outside workspace denied".into()); }
    Ok(target)
}

fn scan(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if files.len() >= MAX_ENTRIES { return Ok(()); }
    if path.is_file() { files.push(path.to_owned()); return Ok(()); }
    if !path.is_dir() { return Err("Not a file or directory".into()); }
    let mut entries: Vec<_> = fs::read_dir(path).map_err(|e| e.to_string())?
        .filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        if files.len() >= MAX_ENTRIES { break; }
        if entry.file_name().is_some_and(|n| {
            let v = n.to_string_lossy();
            v == ".dude" || v == ".git" || v == "target" || v == "node_modules" || v == ".env" ||
            v.starts_with(".env.") || v == ".ssh" || v == ".aws" || v == ".config" ||
            v == "id_rsa" || v == "id_ed25519"
        }) { continue; }
        // Never recurse into symlinks or leave the workspace.
        if fs::symlink_metadata(&entry).map_err(|e| e.to_string())?.file_type().is_symlink() { continue; }
        if entry.is_dir() { scan(&entry, files)?; }
        else if entry.is_file() { files.push(entry); }
    }
    Ok(())
}

fn read_limited(path: &Path) -> Result<String,String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 { return Err("File exceeds 1 MiB limit".into()); }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "Not UTF-8 text".to_string())?;
    Ok(text)
}

pub fn execute(root: &Path, name: &str, args: &Value) -> Result<String,String> {
    let relative = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let path = resolve(root, relative)?;
    match name {
        "list_files" => {
            if !path.is_dir() { return Err("Not a directory".into()); }
            let mut lines = Vec::new();
            let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
            for item in entries.take(MAX_ENTRIES) {
                let item = item.map_err(|e| e.to_string())?;
                lines.push(format!("{}{}",item.file_name().to_string_lossy(),if item.file_type().map_err(|e| e.to_string())?.is_dir() { "/" } else { "" }));
            }
            lines.sort();
            Ok(lines.join("\n"))
        }
        "read_file" => {
            if !path.is_file() { return Err("Not a file".into()); }
            let content = read_limited(&path)?;
            let start = args.get("start_line").and_then(Value::as_u64).unwrap_or(1).clamp(1,100_000) as usize;
            let end = args.get("end_line").and_then(Value::as_u64).unwrap_or((start+199) as u64).clamp(start as u64,100_000) as usize;
            let value = content.lines().enumerate()
                .filter(|(i,_)| *i+1 >= start && *i+1 <= end)
                .map(|(i,l)| format!("{}: {}",i+1,l))
                .collect::<Vec<_>>().join("\n");
            Ok(value.chars().take(MAX_BYTES).collect())
        }
        "project_map" => {
            let mut files = Vec::new();
            scan(&path, &mut files)?;
            let mut lines = Vec::new();
            for file in files {
                let rel = file.strip_prefix(root).unwrap_or(&file);
                let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "rs" | "php" | "java" | "cpp" | "hpp" | "h" | "py" | "js" | "ts") { continue; }
                lines.push(format!("{}:", rel.display()));
                if let Ok(source) = read_limited(&file) {
                    for (i, line) in source.lines().enumerate() {
                        let declaration = line.trim_start();
                        let is_signature = match ext {
                            "rs" => declaration.starts_with("pub fn ")
                                || declaration.starts_with("fn ")
                                || declaration.starts_with("pub struct ")
                                || declaration.starts_with("struct ")
                                || declaration.starts_with("pub enum ")
                                || declaration.starts_with("enum ")
                                || declaration.starts_with("impl ")
                                || declaration.starts_with("pub trait "),
                            "py" => declaration.starts_with("def ") || declaration.starts_with("class ")
                                || declaration.starts_with("async def "),
                            _ => declaration.starts_with("class ") || declaration.starts_with("interface ")
                                || declaration.starts_with("function ") || declaration.starts_with("public class ")
                                || declaration.starts_with("export function ") || declaration.starts_with("export class "),
                        };
                        if is_signature {
                            lines.push(format!("  {}: {}", i + 1, declaration.chars().take(140).collect::<String>()));
                        }
                        if lines.len() >= 650 { break; }
                    }
                }
                if lines.len() >= 650 { break; }
            }
            Ok(lines.join("\n").chars().take(MAX_BYTES).collect())
        }
        "search_text" => {
            let pattern = args.get("pattern").and_then(Value::as_str).ok_or("Missing pattern")?;
            if pattern.is_empty() || pattern.len()>256 { return Err("Pattern must be 1-256 characters".into()); }
            let mut files=Vec::new();
            scan(&path,&mut files)?;
            let mut lines=Vec::new();
            for file in files {
                let Ok(content) = read_limited(&file) else { continue };
                for (i,line) in content.lines().enumerate() {
                    if line.contains(pattern) {
                        lines.push(format!("{}:{}: {}",file.strip_prefix(root).unwrap_or(&file).display(),i+1,line));
                        if lines.len()>=100 { break; }
                    }
                }
                if lines.len()>=100 { break; }
            }
            Ok(lines.join("\n").chars().take(MAX_BYTES).collect())
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

#[cfg(test)]
mod project_map_tests {
    use super::*;
    #[test]
    fn maps_rust_declarations_without_reading_hidden_state() {
        let root = std::env::temp_dir().join(format!("dude-map-{}-{}", std::process::id(), "alpha"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub struct Engine;\nimpl Engine {\n    pub fn run(&self) {}\n}\n").unwrap();
        fs::create_dir_all(root.join(".dude")).unwrap();
        fs::write(root.join(".dude/secret.rs"), "pub fn private() {}").unwrap();
        let output = execute(&root, "project_map", &json!({"path": "."})).unwrap();
        assert!(output.contains("src/lib.rs:"));
        assert!(output.contains("pub struct Engine"));
        assert!(output.contains("pub fn run"));
        assert!(!output.contains("private"));
        let _ = fs::remove_dir_all(&root);
    }
}
