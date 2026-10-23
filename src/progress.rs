//! Durable metadata-only investigation progress, independent of learned Knowledge.
//! Source content is deliberately not persisted in this journal.
use std::{fs::{self, OpenOptions}, io::Write, path::Path};
use serde_json::{json, Value};

pub fn append(root: &Path, tool: &str, args: &Value, success: bool, cache_hit: bool) -> Result<(), String> {
    let dir = root.join(".dude");
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    if fs::symlink_metadata(&dir).map_err(|e|e.to_string())?.file_type().is_symlink() {
        return Err("Refusing symlink progress directory".into());
    }
    let path = dir.join("investigation_progress.jsonl");
    if path.exists() && fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
        return Err("Refusing symlink progress file".into());
    }
    // A bounded path prevents oversized lines or accidental inclusion of tool arguments.
    let relative = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let entry = json!({
        "timestamp_utc": chrono::Utc::now().to_rfc3339(),
        "tool": tool,
        "path": relative.chars().take(512).collect::<String>(),
        "success": success,
        "cache_hit": cache_hit,
        "start_line": args.get("start_line").and_then(Value::as_u64),
        "end_line": args.get("end_line").and_then(Value::as_u64),
    });
    let line = serde_json::to_string(&entry).map_err(|e|e.to_string())?;
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|e|e.to_string())?;
    writeln!(file, "{line}").map_err(|e|e.to_string())?;
    file.sync_data().map_err(|e|e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_metadata_without_source_text() {
        let root=std::env::temp_dir().join(format!("dude-progress-{}",std::process::id()));
        fs::create_dir_all(&root).unwrap();
        append(&root,"read_file",&json!({"path":"src/main.rs","start_line":1,"pattern":"secret-pattern"}),true,false).unwrap();
        let data=fs::read_to_string(root.join(".dude/investigation_progress.jsonl")).unwrap();
        assert!(data.contains("src/main.rs"));
        assert!(!data.contains("secret-pattern"));
        fs::remove_dir_all(root).unwrap();
    }
}
