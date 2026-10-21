use std::{fs, path::{Path, PathBuf}};
use serde_json::{json, Value};
use ignore::gitignore::{Gitignore, GitignoreBuilder};

const MAX_BYTES: usize = 32_768;
const MAX_ENTRIES: usize = 150;

pub fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"workspace_map","description":"Summarize workspace structure, manifests and likely entry points deterministically (read-only). Use this to orient an unfamiliar codebase before selecting files to read.","parameters":{"type":"object","properties":{}}}}),
        json!({"type":"function","function":{"name":"list_files","description":"List files in a workspace directory (read-only).","parameters":{"type":"object","properties":{"path":{"type":"string","description":"Relative directory path, default ."}}}}}),
        json!({"type":"function","function":{"name":"read_file","description":"Read a text file, optionally limited to line range. Similar to sed -n, but read-only.","parameters":{"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer"},"end_line":{"type":"integer"}},"required":["path"]}}}),
        json!({"type":"function","function":{"name":"search_text","description":"Search text in workspace files, like grep -n. No regex; case-sensitive literal search.","parameters":{"type":"object","properties":{"path":{"type":"string","description":"File or directory relative path, default ."},"pattern":{"type":"string"}},"required":["pattern"]}}})
    ]
}

/// Project-local gitignore-compatible exclusions; security exclusions still take precedence.
fn project_ignore(root: &Path) -> Result<Gitignore, String> {
    let mut builder = GitignoreBuilder::new(root);
    let file = root.join(".dudeignore");
    if file.is_file() {
        if fs::symlink_metadata(&file).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink .dudeignore".into());
        }
        if fs::metadata(&file).map_err(|e| e.to_string())?.len() > 65_536 {
            return Err(".dudeignore exceeds 64 KiB".into());
        }
        builder.add(&file).map_err(|e| format!("Invalid .dudeignore: {e}"))?;
    }
    builder.build().map_err(|e| format!("Invalid .dudeignore: {e}"))
}

fn ignored(root: &Path, target: &Path, is_dir: bool, rules: &Gitignore) -> bool {
    if target == root { return false; }
    let mut candidate = target;
    loop {
        let dir = if candidate == target { is_dir } else { true };
        if rules.matched(candidate, dir).is_ignore() { return true; }
        if candidate == root { break; }
        let Some(parent) = candidate.parent() else { break };
        if !parent.starts_with(root) { break; }
        candidate = parent;
    }
    false
}

fn resolve(root: &Path, relative: &str) -> Result<PathBuf,String> {
    let p = Path::new(relative);
    if p.is_absolute() { return Err("Absolute paths are disabled".into()); }
    if p.components().any(|c| {
        let v = c.as_os_str().to_string_lossy();
        v == ".env" || v.starts_with(".env.") || v == ".dudeignore" || v == ".dude" || v == ".git" || v == ".ssh" ||
        v == ".aws" || v == ".config" || v == "id_rsa" || v == "id_ed25519"
    }) {
        return Err("Sensitive paths are blocked from AI tools".into());
    }
    let target = root.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(root) { return Err("Path outside workspace denied".into()); }
    Ok(target)
}

fn scan(root: &Path, path: &Path, files: &mut Vec<PathBuf>, rules: &Gitignore) -> Result<(), String> {
    if files.len() >= MAX_ENTRIES { return Ok(()); }
    if ignored(root, path, path.is_dir(), rules) { return Ok(()); }
    if path.is_file() { files.push(path.to_owned()); return Ok(()); }
    if !path.is_dir() { return Err("Not a file or directory".into()); }
    let mut entries: Vec<_> = fs::read_dir(path).map_err(|e| e.to_string())?
        .filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        if files.len() >= MAX_ENTRIES { break; }
        if entry.file_name().is_some_and(|n| {
            let v = n.to_string_lossy();
            v == ".dude" || v == ".dudeignore" || v == ".git" || v == "target" || v == "node_modules" || v == ".env" ||
            v.starts_with(".env.") || v == ".ssh" || v == ".aws" || v == ".config" ||
            v == "id_rsa" || v == "id_ed25519"
        }) { continue; }
        // Never recurse into symlinks or leave the workspace.
        if fs::symlink_metadata(&entry).map_err(|e| e.to_string())?.file_type().is_symlink() { continue; }
        if ignored(root, &entry, entry.is_dir(), rules) { continue; }
        if entry.is_dir() { scan(root, &entry, files, rules)?; }
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

/// Deterministic lightweight reconnaissance; no repository files are sent here.
pub fn workspace_map(root: &Path) -> Result<String, String> {
    let rules = project_ignore(root)?;
    const DEPTH: usize = 3;
    const MAX_ITEMS: usize = 400;
    const MARKERS: &[&str] = &[
        "Cargo.toml", "package.json", "composer.json", "pom.xml", "build.gradle",
        "settings.gradle", "CMakeLists.txt", "Makefile", "go.mod", "pyproject.toml",
        "requirements.txt", "Dockerfile", "docker-compose.yml", "compose.yaml",
        "README.md", "README", "meson.build", "build.sbt",
    ];
    const ENTRY: &[&str] = &[
        "main.rs", "lib.rs", "main.py", "app.py", "manage.py", "index.ts",
        "index.js", "main.ts", "main.cpp", "main.cc", "main.c", "App.tsx",
        "application.properties", "routes.yaml", "routes.php",
    ];
    let mut dirs = vec![(root.to_path_buf(), 0usize)];
    let mut manifests = Vec::new();
    let mut entries = Vec::new();
    let mut structure = Vec::new();
    let mut inspected = 0usize;
    let mut capped = false;
    while let Some((dir, depth)) = dirs.pop() {
        let mut listing: Vec<_> = fs::read_dir(&dir).map_err(|e| e.to_string())?
            .filter_map(Result::ok).collect();
        listing.sort_by_key(|entry| entry.file_name());
        let mut subdirs = Vec::new();
        for entry in listing {
            if inspected >= MAX_ITEMS { capped = true; break; }
            let ty = match entry.file_type() { Ok(ty) => ty, Err(_) => continue };
            if ty.is_symlink() { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            if matches!(name.as_str(), ".git" | ".dude" | ".dudeignore" | "target" | "node_modules" | ".ssh" | ".aws" | ".config" | "vendor" | "dist" | "build")
                || name == ".env" || name.starts_with(".env.") || name.starts_with(".git")
                || name == "id_rsa" || name == "id_ed25519" { continue; }
            inspected += 1;
            if ignored(root, &entry.path(), ty.is_dir(), &rules) { continue; }
            let rel = entry.path().strip_prefix(root).unwrap_or(root).display().to_string();
            if ty.is_dir() {
                if depth < DEPTH {
                    structure.push(format!("{rel}/"));
                    subdirs.push((entry.path(), depth + 1));
                }
            } else if ty.is_file() {
                if MARKERS.contains(&name.as_str()) { manifests.push(rel.clone()); }
                if ENTRY.contains(&name.as_str()) { entries.push(rel); }
            }
        }
        // DFS with ascending lexical order at each level.
        subdirs.reverse();
        dirs.extend(subdirs);
        if capped { break; }
    }
    let out = format!(
        "Workspace map (read-only; names are clues, not verified architecture)\nDirectories (depth <= {DEPTH}):\n{}\nProject manifests and documentation:\n{}\nPotential entry points:\n{}\nItems inspected: {inspected}{}\nNext: read relevant manifests/entry points before drawing conclusions.",
        structure.join("\n"), manifests.join("\n"), entries.join("\n"),
        if capped { " (scan capped; results incomplete)" } else { "" }
    );
    Ok(out)
}

pub fn execute(root: &Path, name: &str, args: &Value) -> Result<String,String> {
    if name == "workspace_map" { return workspace_map(root); }
    let relative = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let path = resolve(root, relative)?;
    let rules = project_ignore(root)?;
    if ignored(root, &path, path.is_dir(), &rules) { return Err("Path excluded by .dudeignore".into()); }
    match name {
        "list_files" => {
            if !path.is_dir() { return Err("Not a directory".into()); }
            let mut lines = Vec::new();
            let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
            for item in entries.take(MAX_ENTRIES) {
                let item = item.map_err(|e| e.to_string())?;
                if ignored(root, &item.path(), item.file_type().map_err(|e| e.to_string())?.is_dir(), &rules) { continue; }
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
        "search_text" => {
            let pattern = args.get("pattern").and_then(Value::as_str).ok_or("Missing pattern")?;
            if pattern.is_empty() || pattern.len()>256 { return Err("Pattern must be 1-256 characters".into()); }
            let mut files=Vec::new();
            scan(root,&path,&mut files,&rules)?;
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
mod workspace_map_tests {
    use super::*;
    #[test]
    fn discovers_entry_points_and_skips_secrets() {
        let root = std::env::temp_dir().join(format!("dude-map-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".dude")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"example\"\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join(".dude/README.md"), "secret").unwrap();
        let canonical = root.canonicalize().unwrap();
        let map = workspace_map(&canonical).unwrap();
        assert!(map.contains("Cargo.toml"));
        assert!(map.contains("src/main.rs"));
        assert!(!map.contains(".dude"));
        fs::remove_dir_all(&root).unwrap();
    }
}

#[cfg(test)]
mod dudeignore_tests {
    use super::*;
    #[test]
    fn ignores_directories_and_patterns_but_not_source() {
        let root = std::env::temp_dir().join(format!("dude-ignore-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("generated")).unwrap();
        fs::write(root.join(".dudeignore"), "generated/\n*.log\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("generated/main.rs"), "should be ignored").unwrap();
        fs::write(root.join("app.log"), "should be ignored").unwrap();
        let root = root.canonicalize().unwrap();
        let map = workspace_map(&root).unwrap();
        assert!(map.contains("src/main.rs"));
        assert!(!map.contains("generated/"));
        assert!(execute(&root, "read_file", &json!({"path":"generated/main.rs"})).is_err());
        assert!(execute(&root, "read_file", &json!({"path":"app.log"})).is_err());
        let result = execute(&root, "search_text", &json!({"pattern":"should be ignored"})).unwrap();
        assert!(result.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }
}
