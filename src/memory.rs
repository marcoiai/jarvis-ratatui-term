//! On-disk, project-scoped co-engineering memory. No automatic summarization or silent edits.
use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Exchange { pub user: String, pub assistant: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingRequest {
    pub question: String,
    #[serde(default)]
    pub roles: Option<crate::roles::Roles>,
    #[serde(default)]
    pub ollama_model: Option<String>,
    #[serde(default)]
    pub groq_model: Option<String>,
    #[serde(default)]
    pub gemini_model: Option<String>,
    #[serde(default)]
    pub cloud_was_allowed: bool,
}

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
        let path = self.dir.join("history.jsonl");
        if path.exists() && fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Symlink history is not allowed".into());
        }
        let mut file=OpenOptions::new().create(true).append(true).open(path)
            .map_err(|e|e.to_string())?;
        writeln!(file,"{line}").map_err(|e|e.to_string())?;
        file.sync_data().map_err(|e|e.to_string())
    }
    /// Raw operational journal, independent of approved Knowledge and summaries.
    /// One JSONL file per UTC day; every completed exchange is flushed to disk.
    pub fn journal_exchange(&self, user: &str, assistant: &str) -> Result<(), String> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let dir = self.dir.join("journal");
        fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
        if fs::symlink_metadata(&dir).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink journal directory".into());
        }
        let path = dir.join(format!("{today}.jsonl"));
        if path.exists() && fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink journal file".into());
        }
        let entry = serde_json::json!({
            "timestamp_utc": chrono::Utc::now().to_rfc3339(),
            "kind": "conversation",
            "user": user,
            "assistant": assistant,
        });
        let line = serde_json::to_string(&entry).map_err(|e|e.to_string())?;
        if line.len() > 128_000 { return Err("Journal entry too large".into()); }
        let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|e|e.to_string())?;
        writeln!(file, "{line}").map_err(|e|e.to_string())?;
        file.sync_data().map_err(|e|e.to_string())
    }

    /// Durable recovery marker for the in-flight user request. It is NOT curated learning.
    pub fn save_pending(&self, pending: &PendingRequest) -> Result<(), String> {
        let question = &pending.question;
        if question.len() > 32_000 { return Err("Pending request too large".into()); }
        let destination = self.dir.join("pending_request.json");
        if destination.exists() && fs::symlink_metadata(&destination).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink pending file".into());
        }
        let tmp = self.dir.join(format!(".pending-{}.tmp", std::process::id()));
        let data = serde_json::to_vec(pending).map_err(|e|e.to_string())?;
        let result = (|| -> Result<(), String> {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(|e|e.to_string())?;
            file.write_all(&data).map_err(|e|e.to_string())?;
            file.sync_all().map_err(|e|e.to_string())?;
            fs::rename(&tmp,&destination).map_err(|e|e.to_string())
        })();
        if result.is_err() { let _=fs::remove_file(&tmp); }
        result
    }
    pub fn pending(&self) -> Result<Option<PendingRequest>, String> {
        let path=self.dir.join("pending_request.json");
        if !path.exists() { return Ok(None); }
        if fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
            return Err("Refusing symlink pending file".into());
        }
        let data=fs::read(path).map_err(|e|e.to_string())?;
        if data.len()>40_000 {return Err("Pending request too large".into());}
        let value:PendingRequest=serde_json::from_slice(&data).map_err(|e|e.to_string())?;
        Ok(Some(value))
    }
    pub fn clear_pending(&self) -> Result<(),String> {
        let path=self.dir.join("pending_request.json");
        if path.exists() {
            if fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
                return Err("Refusing symlink pending file".into());
            }
            fs::remove_file(path).map_err(|e|e.to_string())?;
        }
        Ok(())
    }
    pub fn recent(&self) -> Vec<Exchange> {
        let Ok(text)=fs::read_to_string(self.dir.join("history.jsonl")) else {return Vec::new()};
        text.lines().rev().take(8).filter_map(|line|serde_json::from_str(line).ok())
            .collect::<Vec<_>>().into_iter().rev().collect()
    }
    pub fn location(&self) -> &Path { &self.dir }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let id=NEXT.fetch_add(1,Ordering::Relaxed);
            let path=std::env::temp_dir().join(format!("dude-memory-test-{}-{id}",std::process::id()));
            fs::create_dir_all(&path).expect("create fixture");
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) { let _=fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn persists_across_restarts() {
        let fixture=Fixture::new();
        let memory=Memory::open(&fixture.0).unwrap();
        memory.remember("Qt 6 migration").unwrap();
        memory.decision("ADR-1: test first").unwrap();
        memory.checkpoint("checkpoint one").unwrap();
        memory.record("hi","hello").unwrap();
        drop(memory);
        let restored=Memory::open(&fixture.0).unwrap();
        assert!(restored.context().contains("Qt 6 migration"));
        assert!(restored.context().contains("ADR-1"));
        assert_eq!(restored.recent().len(),1);
        assert_eq!(restored.recent()[0].assistant,"hello");
        assert!(restored.location().join("checkpoints.md").exists());
    }

    #[test]
    fn raw_journal_survives_restart_without_knowledge_approval() {
        let fixture = Fixture::new();
        let memory = Memory::open(&fixture.0).unwrap();
        memory.journal_exchange("investigue a rota", "rota não localizada").unwrap();
        drop(memory);
        let restored = Memory::open(&fixture.0).unwrap();
        let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let data = fs::read_to_string(restored.location().join("journal").join(format!("{day}.jsonl"))).unwrap();
        assert!(data.contains("investigue a rota"));
        assert!(!restored.context().contains("rota não localizada"));
    }
    #[test]
    fn restores_unfinished_request_after_reopen() {
        let fixture=Fixture::new();
        let memory=Memory::open(&fixture.0).unwrap();
        memory.save_pending(&PendingRequest {
            question:"analise o projeto".into(), roles: None,
            ollama_model:None, groq_model:None, gemini_model:None, cloud_was_allowed:false,
        }).unwrap();
        drop(memory);
        let restored=Memory::open(&fixture.0).unwrap();
        assert_eq!(restored.pending().unwrap().unwrap().question,"analise o projeto");
        restored.clear_pending().unwrap();
        assert!(restored.pending().unwrap().is_none());
    }
    #[test]
    fn corrupted_history_line_does_not_lose_valid_entries() {
        let fixture=Fixture::new();
        let memory=Memory::open(&fixture.0).unwrap();
        memory.record("valid","answer").unwrap();
        fs::OpenOptions::new().append(true)
            .open(memory.location().join("history.jsonl")).unwrap()
            .write_all(b"{partial json\n").unwrap();
        let restored=Memory::open(&fixture.0).unwrap();
        assert_eq!(restored.recent().len(),1);
        assert_eq!(restored.recent()[0].user,"valid");
    }

    #[test]
    fn rejects_blank_and_overlong_memory() {
        let fixture=Fixture::new();
        let memory=Memory::open(&fixture.0).unwrap();
        assert!(memory.remember("   ").is_err());
        assert!(memory.decision(&"x".repeat(4001)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_journal() {
        use std::os::unix::fs::symlink;
        let fixture=Fixture::new();
        let memory=Memory::open(&fixture.0).unwrap();
        let outside=fixture.0.join("outside");
        fs::write(&outside,"untouched").unwrap();
        symlink(&outside,memory.location().join("history.jsonl")).unwrap();
        assert!(memory.record("secret","response").is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(),"untouched");
    }
}
