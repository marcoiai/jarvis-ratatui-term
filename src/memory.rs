//! On-disk, project-scoped co-engineering memory. No automatic summarization or silent edits.
use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Exchange { pub user: String, pub assistant: String }

pub struct Memory { dir: PathBuf }

impl Memory {
    pub fn open(workspace: &Path) -> Result<Self, String> {
        let dir = workspace.join(".dude");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        if fs::symlink_metadata(&dir).map_err(|e| e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink memory directory".into());
        }
        let memory = Self { dir };
        for (name, text) in [
            ("context.md", "# Project context\n\nDescribe project goals, constraints, architecture and important references here.\n"),
            ("decisions.md", "# Engineering decisions\n\nAppend-only decision journal. Verify decisions against evidence.\n"),
            ("memory.md", "# Curated project memory\n\nOnly verified, explicitly retained facts go here.\n"),
        ] {
            let file = memory.dir.join(name);
            if !file.exists() {
                OpenOptions::new().write(true).create_new(true).open(&file)
                    .and_then(|mut f| f.write_all(text.as_bytes())).map_err(|e| e.to_string())?;
            }
        }
        Ok(memory)
    }
    fn read(&self, name: &str, limit: usize) -> String {
        fs::read_to_string(self.dir.join(name)).unwrap_or_default().chars().take(limit).collect()
    }
    pub fn context(&self) -> String {
        format!("Project context (user-maintained, may be incomplete):\n{}\nVerified memory:\n{}\nRecent decisions:\n{}",
            self.read("context.md", 5000),
            self.read("memory.md", 5000),
            self.read("decisions.md", 3500))
    }
    fn append(&self, file: &str, text: &str) -> Result<(), String> {
        if text.trim().is_empty() || text.len() > 4000 { return Err("Entry must be 1-4000 bytes".into()); }
        let path = self.dir.join(file);
        if fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Symlink memory files are not allowed".into());
        }
        writeln!(OpenOptions::new().append(true).open(path).map_err(|e|e.to_string())?,
            "\n- {}", text.trim().replace('\n', "\n  ")).map_err(|e| e.to_string())
    }
    pub fn remember(&self, text: &str) -> Result<(), String> { self.append("memory.md", text) }
    pub fn decision(&self, text: &str) -> Result<(), String> { self.append("decisions.md", text) }
    pub fn checkpoint(&self, summary: &str) -> Result<(), String> {
        let path = self.dir.join("checkpoints.md");
        if !path.exists() {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => f.write_all(b"# Checkpoints\n").map_err(|e|e.to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.to_string()),
            }
        }
        self.append("checkpoints.md", summary)
    }
    pub fn record(&self, user: &str, assistant: &str) -> Result<(), String> {
        let exchange = Exchange {user:user.to_owned(),assistant:assistant.to_owned()};
        let line = serde_json::to_string(&exchange).map_err(|e|e.to_string())?;
        if line.len() > 64_000 { return Err("Exchange too large to persist".into()); }
        let mut file=OpenOptions::new().create(true).append(true).open(self.dir.join("history.jsonl"))
            .map_err(|e|e.to_string())?;
        writeln!(file,"{line}").map_err(|e|e.to_string())
    }
    pub fn recent(&self) -> Vec<Exchange> {
        let Ok(text)=fs::read_to_string(self.dir.join("history.jsonl")) else {return Vec::new()};
        text.lines().rev().take(8).filter_map(|line|serde_json::from_str(line).ok())
            .collect::<Vec<_>>().into_iter().rev().collect()
    }
    pub fn location(&self) -> &Path { &self.dir }
}
