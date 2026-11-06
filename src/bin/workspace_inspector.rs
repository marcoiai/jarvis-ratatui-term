//! DUDE Workspace Inspector MVP — read-only, no LLM and no filesystem mutations.
//! Usage: cargo run --bin workspace_inspector -- /path/to/workspace
//! Output: structured JSON on stdout. Diagnostic errors go to stderr.
use serde::Serialize;
use std::{env, path::{Path, PathBuf}, process::{Command, ExitCode}};

#[derive(Debug, Serialize)]
struct Observation {
    path: String,
    exists: bool,
    canonical_path: Option<String>,
    directory: bool,
    git_kind: String,
    git_root: Option<String>,
    branch: Option<String>,
    head: Option<String>,
    remotes: Vec<String>,
    worktrees: Vec<Worktree>,
    changed: Vec<String>,
    untracked: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Worktree {
    path: String,
    head: Option<String>,
    branch: Option<String>,
    bare: bool,
    detached: bool,
    locked: bool,
    prunable: bool,
}

fn git(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git").arg("-C").arg(path).args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output().map_err(|e| format!("git unavailable: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().to_string())
}

fn read_worktrees(text: &str) -> Vec<Worktree> {
    let mut result = Vec::new();
    for block in text.split("\n\n").filter(|b| !b.trim().is_empty()) {
        let mut worktree = Worktree {
            path: String::new(), head: None, branch: None, bare: false,
            detached: false, locked: false, prunable: false,
        };
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("worktree ") { worktree.path = v.into(); }
            else if let Some(v) = line.strip_prefix("HEAD ") { worktree.head = Some(v.into()); }
            else if let Some(v) = line.strip_prefix("branch ") { worktree.branch = Some(v.into()); }
            else if line == "bare" { worktree.bare = true; }
            else if line == "detached" { worktree.detached = true; }
            else if line.starts_with("locked") { worktree.locked = true; }
            else if line.starts_with("prunable") { worktree.prunable = true; }
        }
        if !worktree.path.is_empty() { result.push(worktree); }
    }
    result
}

fn inspect(path: &Path) -> Observation {
    let mut report = Observation {
        path: path.display().to_string(), exists: path.exists(),
        canonical_path: None, directory: path.is_dir(),
        git_kind: "unknown".into(), git_root: None,
        branch: None, head: None, remotes: vec![], worktrees: vec![],
        changed: vec![], untracked: vec![], warnings: vec![],
    };
    if !report.exists {
        report.warnings.push("path does not exist; no changes made".into());
        return report;
    }
    if !report.directory {
        report.warnings.push("path is not a directory; no changes made".into());
        return report;
    }
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            report.warnings.push(format!("cannot resolve path: {e}"));
            return report;
        }
    };
    report.canonical_path = Some(canonical.display().to_string());
    // A directory containing loose Git control files is suspicious, even if
    // they have been accidentally copied from a bare Git directory.
    if ["HEAD", "objects", "refs", "config"].iter().all(|name| canonical.join(name).exists()) {
        report.warnings.push("Git control files found at directory root; verify before cleanup".into());
    }
    let bare = match git(&canonical, &["rev-parse", "--is-bare-repository"]) {
        Ok(s) => s == "true",
        Err(e) => {
            report.git_kind = "not_git_or_invalid".into();
            report.warnings.push(format!("Git inspection failed: {e}"));
            return report;
        }
    };
    report.git_kind = if bare { "bare" } else { "worktree" }.into();
    if bare {
        report.warnings.push("bare repository: git status is not available; do not assume directory contents are disposable".into());
    } else {
        report.git_root = git(&canonical, &["rev-parse", "--show-toplevel"]).ok();
        // --porcelain=v1 supplies stable, machine-oriented output. We retain
        // each complete line; an untrusted filename never becomes a shell command.
        match git(&canonical, &["status", "--porcelain=v1", "--untracked-files=all"]) {
            Ok(output) => {
                for line in output.lines() {
                    if let Some(name) = line.strip_prefix("?? ") {
                        report.untracked.push(name.to_owned());
                    } else if !line.is_empty() {
                        report.changed.push(line.to_owned());
                    }
                }
            }
            Err(e) => report.warnings.push(format!("status unavailable: {e}")),
        }
        report.branch = git(&canonical, &["branch", "--show-current"]).ok().filter(|v| !v.is_empty());
    }
    report.head = git(&canonical, &["rev-parse", "--verify", "HEAD"]).ok();
    report.remotes = git(&canonical, &["remote", "-v"])
        .map(|v| v.lines().map(str::to_owned).collect()).unwrap_or_default();
    match git(&canonical, &["worktree", "list", "--porcelain"]) {
        Ok(output) => report.worktrees = read_worktrees(&output),
        Err(e) => report.warnings.push(format!("worktree listing unavailable: {e}")),
    }
    report
}

fn main() -> ExitCode {
    let mut args = env::args_os();
    let _program = args.next();
    let Some(path) = args.next() else {
        eprintln!("Usage: workspace_inspector PATH");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("Expected exactly one directory PATH");
        return ExitCode::from(2);
    }
    let report = inspect(&PathBuf::from(path));
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(e) => { eprintln!("JSON serialization failed: {e}"); return ExitCode::FAILURE; }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_worktrees_and_flags() {
        let input = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /tmp/test\nHEAD def\ndetached\nlocked reason\n\n";
        let trees = read_worktrees(input);
        assert_eq!(trees.len(), 2);
        assert_eq!(trees[0].branch.as_deref(), Some("refs/heads/main"));
        assert!(trees[1].detached && trees[1].locked);
    }

    #[test]
    fn missing_directory_does_not_mutate() {
        let missing = std::env::temp_dir().join(format!("dude-inspector-nonexistent-{}", std::process::id()));
        let report = inspect(&missing);
        assert!(!report.exists);
        assert!(!missing.exists());
    }

    #[test]
    fn non_git_directory_reports_uncertainty() {
        let report = inspect(&std::env::temp_dir());
        assert!(matches!(report.git_kind.as_str(), "not_git_or_invalid" | "worktree"));
    }
}
