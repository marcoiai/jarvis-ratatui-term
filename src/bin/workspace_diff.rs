//! Deterministic two-directory comparison with an opt-in copy-only resolution.
//! Never deletes, overwrites, follows symlinks, or runs model-generated commands.
//! Usage: cargo run --bin workspace_diff -- SOURCE DESTINATION [--file relative/path] [--apply]
use serde::Serialize;
use std::{env, fs, io::Write, path::{Component, Path, PathBuf}, process::{Command, ExitCode}};

#[derive(Serialize)]
struct DiffReport {
    source: String,
    destination: String,
    file: String,
    result: String,
    applied: bool,
    warning: Option<String>,
    diff: Option<DiffEvidence>,
}
#[derive(Serialize)]
struct DiffEvidence {
    format: String,
    content: String,
    truncated: bool,
    added_lines: usize,
    removed_lines: usize,
}

fn unified_diff(src: &Path, dst: &Path) -> Result<Option<DiffEvidence>, String> {
    // Explicitly bypass external diff drivers and avoid shell invocation.
    let output = Command::new("git")
        .args(["-c", "diff.external=", "diff", "--no-index", "--no-ext-diff", "--no-textconv",
               "--no-color", "--unified=3", "--"])
        .arg(src).arg(dst)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output().map_err(|e| format!("cannot start git diff: {e}"))?;
    match output.status.code() {
        Some(0) => return Ok(None),
        Some(1) => {}, // git diff --no-index reports differences using exit code 1
        _ => return Err(format!("git diff failed: {}", String::from_utf8_lossy(&output.stderr))),
    }
    let all = String::from_utf8_lossy(&output.stdout);
    let limit = 32 * 1024;
    let end = all.char_indices().map(|(i, _)| i).take_while(|&i| i <= limit).last().unwrap_or(0);
    let end = if all.len() <= limit { all.len() } else { end };
    let excerpt = &all[..end];
    let added_lines = all.lines().filter(|line| line.starts_with('+') && !line.starts_with("+++")).count();
    let removed_lines = all.lines().filter(|line| line.starts_with('-') && !line.starts_with("---")).count();
    Ok(Some(DiffEvidence { format: "unified".into(), content: excerpt.into(),
        truncated: end < all.len(), added_lines, removed_lines }))
}

fn safe_rel(raw: &str) -> Result<&Path, String> {
    let rel = Path::new(raw);
    if rel.as_os_str().is_empty() || rel.is_absolute() { return Err("expected a relative file path".into()); }
    for part in rel.components() {
        match part {
            Component::Normal(name) if !matches!(name.to_str(), Some(".git" | ".dude" | "target" | ".venv")) => {}
            _ => return Err("unsafe path segment, protected data, or traversal".into()),
        }
    }
    Ok(rel)
}
fn checked_file(root: &Path, rel: &Path) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    for segment in rel.components() {
        current.push(segment);
        if let Ok(meta) = fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() { return Err(format!("symbolic link rejected: {}", current.display())); }
        }
    }
    Ok(current)
}
fn inspect(source: &Path, target: &Path, relative: &str, apply: bool) -> Result<DiffReport, String> {
    let src_root = source.canonicalize().map_err(|e| format!("source root: {e}"))?;
    let dst_root = target.canonicalize().map_err(|e| format!("destination root: {e}"))?;
    if !src_root.is_dir() || !dst_root.is_dir() { return Err("both roots must be directories".into()); }
    if src_root == dst_root { return Err("source and destination are identical".into()); }
    // Canonical roots may be nested. Reject that to avoid recursive self-copies.
    if src_root.starts_with(&dst_root) || dst_root.starts_with(&src_root) {
        return Err("nested source and destination are not supported".into());
    }
    let rel = safe_rel(relative)?;
    let src = checked_file(&src_root, rel)?;
    let dst = checked_file(&dst_root, rel)?;
    let mut result = DiffReport {
        source: src_root.display().to_string(), destination: dst_root.display().to_string(),
        file: relative.into(), result: String::new(), applied: false, warning: None, diff: None,
    };
    if !src.exists() {
        result.result = if dst.exists() { "destination_only" } else { "missing_both" }.into();
        return Ok(result);
    }
    let metadata = fs::symlink_metadata(&src).map_err(|e| e.to_string())?;
    if !metadata.is_file() { return Err("source must be a regular file (no directories or links)".into()); }
    if metadata.len() > 8 * 1024 * 1024 { return Err("source larger than 8 MiB; manual review required".into()); }
    let bytes = fs::read(&src).map_err(|e| format!("reading source: {e}"))?;
    if dst.exists() {
        let metadata = fs::symlink_metadata(&dst).map_err(|e| e.to_string())?;
        if !metadata.is_file() { return Err("destination is not a regular file".into()); }
        result.result = if fs::read(&dst).map_err(|e| e.to_string())? == bytes { "identical" } else { "different" }.into();
        if result.result == "different" {
            result.diff = unified_diff(&src, &dst)?;
        }
        if apply { return Err("refusing to overwrite an existing destination, even if identical".into()); }
        return Ok(result);
    }
    result.result = "source_only".into();
    result.warning = Some("Copy is allowed only when explicitly requested; no conflict resolution is attempted".into());
    if apply {
        let parent = dst.parent().ok_or("destination has no parent")?;
        if !parent.is_dir() { return Err("destination parent does not exist; refusing to create directories".into()); }
        // Re-check parent after canonicalization to reject symlink escapes or changed paths.
        let canon_parent = parent.canonicalize().map_err(|e| e.to_string())?;
        if !canon_parent.starts_with(&dst_root) { return Err("destination parent escapes root".into()); }
        let mut file = fs::OpenOptions::new().write(true).create_new(true)
            .open(&dst).map_err(|e| format!("destination changed or cannot create safely: {e}"))?;
        file.write_all(&bytes).map_err(|e| format!("write failed (check partial new file): {e}"))?;
        file.sync_all().map_err(|e| format!("sync failed (check new file): {e}"))?;
        result.applied = true;
    }
    Ok(result)
}
fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 4 || args.get(2).map(String::as_str) != Some("--file")
        || (args.len() == 5 && args[4] != "--apply") || args.len() > 5 {
        eprintln!("Usage: workspace_diff SOURCE DESTINATION --file RELATIVE_PATH [--apply]");
        return ExitCode::from(2);
    }
    match inspect(Path::new(&args[0]), Path::new(&args[1]), &args[3], args.get(4).map(String::as_str) == Some("--apply")) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(json) => { println!("{json}"); ExitCode::SUCCESS },
            Err(e) => { eprintln!("JSON: {e}"); ExitCode::FAILURE }
        },
        Err(e) => { eprintln!("Safety check: {e}"); ExitCode::FAILURE }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("dude-diff-test-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = fs::remove_dir_all(&root);
        let src = root.join("src"); let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap(); fs::create_dir_all(&dst).unwrap();
        (root, src, dst)
    }
    #[test]
    fn dry_run_and_create_new_only() {
        let (root, src, dst) = fixture();
        fs::write(src.join("example.txt"), "safe").unwrap();
        let dry = inspect(&src, &dst, "example.txt", false).unwrap();
        assert_eq!(dry.result, "source_only");
        assert!(!dst.join("example.txt").exists());
        assert!(inspect(&src, &dst, "example.txt", true).unwrap().applied);
        assert_eq!(fs::read_to_string(dst.join("example.txt")).unwrap(), "safe");
        assert!(inspect(&src, &dst, "example.txt", true).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn refuses_traversal_protected_files_and_differences() {
        for path in ["../secret", "/tmp/a", ".git/config", ".dude/memory.json", "dir/../a"] {
            assert!(safe_rel(path).is_err(), "{path}");
        }
        let (root, src, dst) = fixture();
        fs::write(src.join("a"), "old").unwrap();
        fs::write(dst.join("a"), "new").unwrap();
        let report = inspect(&src, &dst, "a", false).unwrap();
        assert_eq!(report.result, "different");
        let diff = report.diff.unwrap();
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);
        assert!(diff.content.contains("-old"));
        assert!(diff.content.contains("+new"));
        assert!(inspect(&src, &dst, "a", true).is_err());
        assert_eq!(fs::read_to_string(dst.join("a")).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }
}