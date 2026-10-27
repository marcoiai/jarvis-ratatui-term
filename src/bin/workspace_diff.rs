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
    source_blob: Option<String>,
    destination_blob: Option<String>,
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
    returned_hunks: usize,
    next_hunk: Option<usize>,
    hunks: Vec<HunkEvidence>,
}
#[derive(Serialize)]
struct HunkEvidence { id: String, ordinal: usize, header: String, content: String }


fn unified_diff(src: &Path, dst: &Path, selected: Option<usize>, count: usize) -> Result<Option<DiffEvidence>, String> {
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
    for ordinal in n..n.saturating_add(count).min(total_hunks.saturating_add(1)) {
        let Some((header, body)) = blocks.get(ordinal - 1) else { break };
        let limit = 32 * 1024;
        let end = if body.len() <= limit { body.len() } else {
            body.char_indices().map(|(i,_)| i).take_while(|i| *i <= limit).last().unwrap_or(0)
        };
        truncated |= end < body.len();
        hunks.push(HunkEvidence {
            id: format!("hunk-{ordinal}"), ordinal, header: header.clone(),
            content: body[..end].to_owned(),
        });
    }
    let content = hunks.first().map(|h| h.content.clone()).unwrap_or_default();
    Ok(Some(DiffEvidence {
        format: "unified-hunks".into(), content, truncated,
        added_lines, removed_lines, total_hunks,
        selected_hunk: if hunks.is_empty() { None } else { Some(n) },
        unseen_hunks: total_hunks.saturating_sub(hunks.len()),
        returned_hunks: hunks.len(),
        next_hunk: if n.saturating_add(hunks.len()) <= total_hunks { Some(n + hunks.len()) } else { None },
        hunks,
    }))
}

fn content_oid(path: &Path) -> Result<String, String> {
    let output = Command::new("git").args(["hash-object", "--no-filters", "--"])
        .arg(path).env("GIT_OPTIONAL_LOCKS", "0")
        .output().map_err(|e| format!("hash-object: {e}"))?;
    if !output.status.success() { return Err(format!("hash-object failed: {}", String::from_utf8_lossy(&output.stderr))); }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
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
fn inspect(source: &Path, target: &Path, relative: &str, apply: bool, hunk: Option<usize>, count: usize) -> Result<DiffReport, String> {
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
        source_blob: None, destination_blob: None,
    };
    if !src.exists() {
        result.result = if dst.exists() { "destination_only" } else { "missing_both" }.into();
        return Ok(result);
    }
    let metadata = fs::symlink_metadata(&src).map_err(|e| e.to_string())?;
    if !metadata.is_file() { return Err("source must be a regular file (no directories or links)".into()); }
    if metadata.len() > 8 * 1024 * 1024 { return Err("source larger than 8 MiB; manual review required".into()); }
    let bytes = fs::read(&src).map_err(|e| format!("reading source: {e}"))?;
    if !apply { result.source_blob = Some(content_oid(&src)?); }
    if dst.exists() {
        let metadata = fs::symlink_metadata(&dst).map_err(|e| e.to_string())?;
        if !metadata.is_file() { return Err("destination is not a regular file".into()); }
        result.result = if fs::read(&dst).map_err(|e| e.to_string())? == bytes { "identical" } else { "different" }.into();
        if !apply { result.destination_blob = Some(content_oid(&dst)?); }
        if result.result == "different" {
            result.diff = unified_diff(&src, &dst, hunk, count)?;
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
        eprintln!("Usage: workspace_diff SOURCE DESTINATION --file RELATIVE_PATH [--hunk N] [--count 1..8] [--apply]");
        return ExitCode::from(2);
    }
    let mut apply = false;
    let mut hunk = None;
    let mut count = 1usize;
    let mut seen_count = false;
    let mut i = 4;
    while i < args.len() {
        match args[i].as_str() {
            "--apply" if !apply => { apply = true; i += 1; }
            "--count" if !seen_count && i + 1 < args.len() => {
                count = match args[i + 1].parse::<usize>() {
                    Ok(n) if (1..=8).contains(&n) => n,
                    _ => { eprintln!("--count must be between 1 and 8"); return ExitCode::from(2); }
                };
                seen_count = true;
                i += 2;
            }
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
    if apply && (hunk.is_some() || seen_count) {
        eprintln!("--hunk is read-only; cannot combine with --apply");
        return ExitCode::from(2);
    }
    match inspect(Path::new(&args[0]), Path::new(&args[1]), &args[3], apply, hunk, count) {
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
        let dry = inspect(&src, &dst, "example.txt", false, None, 1).unwrap();
        assert_eq!(dry.result, "source_only");
        assert!(!dst.join("example.txt").exists());
        assert!(inspect(&src, &dst, "example.txt", true, None, 1).unwrap().applied);
        assert_eq!(fs::read_to_string(dst.join("example.txt")).unwrap(), "safe");
        assert!(inspect(&src, &dst, "example.txt", true, None, 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paginates_hunks_and_preserves_full_counts() {
        let (root, src, dst) = fixture();
        let original = (0..40).map(|i| format!("line {i}\n")).collect::<String>();
        let changed = original.replace("line 2\n", "CHANGED 2\n").replace("line 35\n", "CHANGED 35\n");
        fs::write(src.join("a"), original).unwrap();
        fs::write(dst.join("a"), changed).unwrap();
        let first = inspect(&src, &dst, "a", false, Some(1), 1).unwrap().diff.unwrap();
        let second = inspect(&src, &dst, "a", false, Some(2), 1).unwrap().diff.unwrap();
        assert_eq!(first.total_hunks, 2);
        assert_eq!(first.unseen_hunks, 1);
        assert_eq!(first.next_hunk, Some(2));
        let both = inspect(&src, &dst, "a", false, Some(1), 2).unwrap().diff.unwrap();
        assert_eq!(both.returned_hunks, 2);
        assert_eq!(both.unseen_hunks, 0);
        assert_eq!(both.next_hunk, None);
        assert_eq!(first.added_lines, 2);
        assert_eq!(first.removed_lines, 2);
        assert_ne!(first.content, second.content);
        assert!(inspect(&src, &dst, "a", false, Some(3), 1).is_err());
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
        let report = inspect(&src, &dst, "a", false, None, 1).unwrap();
        assert_eq!(report.result, "different");
        let diff = report.diff.unwrap();
        assert_eq!(diff.total_hunks, 1);
        assert_eq!(diff.unseen_hunks, 0);
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);
        assert!(diff.content.contains("-old"));
        assert!(diff.content.contains("+new"));
        assert!(inspect(&src, &dst, "a", true, None, 1).is_err());
        assert_eq!(fs::read_to_string(dst.join("a")).unwrap(), "new");
        fs::remove_dir_all(root).unwrap();
    }
}