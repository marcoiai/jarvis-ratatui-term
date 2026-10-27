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
    total_hunks: usize,
    selected_hunk: Option<usize>,
    unseen_hunks: usize,
    hunks: Vec<HunkEvidence>,
}
#[derive(Serialize)]
struct HunkEvidence { id: String, ordinal: usize, header: String, content: String }


fn unified_diff(src: &Path, dst: &Path, selected: Option<usize>) -> Result<Option<DiffEvidence>, String> {
    let output = Command::new("git")
        .args(["-c", "diff.external=", "diff", "--no-index", "--no-ext-diff", "--no-textconv",
               "--no-color", "--unified=3", "--"])
        .arg(src).arg(dst).env("GIT_OPTIONAL_LOCKS", "0")
        .output().map_err(|e| format!("git diff: {e}"))?;
    match output.status.code() {
        Some(0) => return Ok(None),
        Some(1) => {},
        _ => return Err(format!("git diff failed: {}", String::from_utf8_lossy(&output.stderr))),
    }
    let all = String::from_utf8_lossy(&output.stdout);
    let added_lines = all.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count();
    let removed_lines = all.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count();
    let mut blocks: Vec<(String, String)> = Vec::new();
    let (mut header, mut body) = (String::new(), String::new());
    for line in all.lines() {
        if line.starts_with("@@ ") {
            if !header.is_empty() { blocks.push((header, body)); body = String::new(); }
            header = line.to_owned();
        }
        if !header.is_empty() { body.push_str(line); body.push('\n'); }
    }
    if !header.is_empty() { blocks.push((header, body)); }
    let total_hunks = blocks.len();
    if let Some(n) = selected {
        if n == 0 || n > total_hunks { return Err(format!("hunk {n} out of range; total {total_hunks}")); }
    }
    let n = selected.unwrap_or(1);
    let mut hunks = Vec::new();
    let mut truncated = false;
    if let Some((header, body)) = blocks.get(n.saturating_sub(1)) {
        let limit = 32 * 1024;
        let end = if body.len() <= limit { body.len() } else {
            body.char_indices().map(|(i,_)| i).take_while(|i| *i <= limit).last().unwrap_or(0)
        };
        truncated = end < body.len();
        hunks.push(HunkEvidence {
            id: format!("hunk-{n}"), ordinal: n, header: header.clone(),
            content: body[..end].to_owned(),
        });
    }
    let content = hunks.first().map(|h| h.content.clone()).unwrap_or_default();
    Ok(Some(DiffEvidence {
        format: "unified-hunks".into(), content, truncated,
        added_lines, removed_lines, total_hunks,
        selected_hunk: if hunks.is_empty() { None } else { Some(n) },
        unseen_hunks: total_hunks.saturating_sub(hunks.len()), hunks,
    }))
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
fn inspect(source: &Path, target: &Path, relative: &str, apply: bool, hunk: Option<usize>) -> Result<DiffReport, String> {
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
            result.diff = unified_diff(&src, &dst, hunk)?;
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
    if args.len() < 4 || args.get(2).map(String::as_str) != Some("--file") {
        eprintln!("Usage: workspace_diff SOURCE DESTINATION --file RELATIVE_PATH [--hunk N] [--apply]");
        return ExitCode::from(2);
    }
    let mut apply = false;
    let mut hunk = None;
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "--apply" if !apply => { apply = true; i += 1; }
            "--hunk" if hunk.is_none() && i + 1 < args.len() => {
                hunk = match args[i + 1].parse::<usize>() {
                    Ok(n) if n > 0 => Some(n),
                    _ => { eprintln!("--hunk must be positive"); return ExitCode::from(2); }
                };
                i += 2;
            }
            _ => { eprintln!("unknown or duplicate option"); return ExitCode::from(2); }
        }
    }
    if apply && hunk.is_some() {
        eprintln!("--hunk is read-only; cannot combine with --apply");
        return ExitCode::from(2);
    }
    match inspect(Path::new(&args[0]), Path::new(&args[1]), &args[3], apply, hunk) {
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
        let dry = inspect(&src, &dst, "example.txt", false, None).unwrap();
        assert_eq!(dry.result, "source_only");
        assert!(!dst.join("example.txt").exists());
        assert!(inspect(&src, &dst, "example.txt", true, None).unwrap().applied);
        assert_eq!(fs::read_to_string(dst.join("example.txt")).unwrap(), "safe");
        assert!(inspect(&src, &dst, "example.txt", true, None).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paginates_hunks_and_preserves_full_counts() {
        let (root, src, dst) = fixture();
        let original = (0..40).map(|i| format!("line {i}\n")).collect::<String>();
        let changed = original.replace("line 2\n", "CHANGED 2\n").replace("line 35\n", "CHANGED 35\n");
        fs::write(src.join("a"), original).unwrap();
        fs::write(dst.join("a"), changed).unwrap();
        let first = inspect(&src, &dst, "a", false, Some(1)).unwrap().diff.unwrap();
        let second = inspect(&src, &dst, "a", false, Some(2)).unwrap().diff.unwrap();
        assert_eq!(first.total_hunks, 2);
        assert_eq!(first.unseen_hunks, 1);
        assert_eq!(first.added_lines, 2);
        assert_eq!(first.removed_lines, 2);
        assert_ne!(first.content, second.content);
        assert!(inspect(&src, &dst, "a", false, Some(3)).is_err());
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
        let report = inspect(&src, &dst, "a", false, None).unwrap();
        assert_eq!(report.result, "different");
        let diff = report.diff.unwrap();
        assert_eq!(diff.total_hunks, 1);
        assert_eq!(diff.unseen_hunks, 0);
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);
        assert!(diff.content.contains("-old"));
        assert!(diff.content.contains("+new"));
        assert!(inspect(&src, &dst, "a", true, None).is_err());
        assert_eq!(fs::read_to_string(dst.join("a")).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }
}