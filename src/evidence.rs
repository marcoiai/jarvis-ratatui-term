use std::{collections::BTreeMap, path::Path, sync::{Arc, Mutex}};
use serde_json::{json, Value};
use crate::file_tools;

#[derive(Clone, Default)]
pub struct EvidenceCache {
    cache: Arc<Mutex<BTreeMap<String, Value>>>,
    events: Arc<Mutex<Vec<Value>>>,
}

impl EvidenceCache {
    pub fn execute(&self, root: &Path, name: &str, args: &Value) -> String {
        let key = format!("{name}:{}", args);
        if let Ok(cache) = self.cache.lock() {
            if let Some(item) = cache.get(&key) {
                let result = item["result"].as_str().unwrap_or_default().to_owned();
                self.record_event(name, args, "cache_hit");
                return result;
            }
        }
        match file_tools::execute(root, name, args) {
            Ok(result) => {
                self.record_event(name, args, "ok");
                if let Ok(mut cache) = self.cache.lock() {
                    if cache.len() < 80 {
                        cache.insert(key, json!({"tool":name,"arguments":args,"result":result.chars().take(16000).collect::<String>()}));
                    }
                }
                result
            }
            Err(error) => {
                self.record_event(name, args, "error");
                format!("Tool error: {error}")
            }
        }
    }

    fn record_event(&self, name: &str, args: &Value, status: &str) {
        if let Ok(mut events) = self.events.lock() {
            if events.len() < 300 {
                events.push(json!({"tool":name,"arguments":args,"status":status}));
            }
        }
    }

    /// Count actual, successful source-file reads. Maps and searches only discover paths.
    pub fn successful_file_reads(&self) -> usize {
        self.events.lock().map(|events| events.iter().filter(|event|
            event["tool"] == "read_file"
                && (event["status"] == "ok" || event["status"] == "cache_hit")
        ).count()).unwrap_or(0)
    }

    /// Events describe actual tool executions (or cache hits), never model-generated text.
    pub fn execution_summary(&self) -> String {
        let Ok(events) = self.events.lock() else { return String::new() };
        if events.is_empty() { return "Tools executed this turn: none".into(); }
        let successful = events.iter().filter(|e|e["status"] == "ok").count();
        let failed = events.iter().filter(|e|e["status"] == "error").count();
        let hits = events.iter().filter(|e|e["status"] == "cache_hit").count();
        let mut lines = vec![format!("Tools executed this turn: {} successful, {} errors, {} cache hits",successful,failed,hits)];
        for event in events.iter().take(25) {
            let path = event["arguments"]["path"].as_str()
                .or_else(||event["arguments"]["query"].as_str())
                .unwrap_or(".");
            lines.push(format!("- {}({}) [{}]",event["tool"].as_str().unwrap_or("?"),path,event["status"].as_str().unwrap_or("?")));
        }
        if events.len() > 25 { lines.push(format!("... {} additional actions omitted",events.len()-25)); }
        lines.join("\n")
    }

    pub fn export(&self) -> String {
        let Ok(cache) = self.cache.lock() else { return "Evidence cache unavailable".into() };
        let mut text = String::new();
        for (index, item) in cache.values().enumerate() {
            let line = format!("Evidence #{}: {}({})\n{}\n\n",index+1,item["tool"].as_str().unwrap_or("unknown"),item["arguments"],item["result"].as_str().unwrap_or(""));
            if text.len()+line.len()>28000 { text.push_str("[Further evidence omitted due to context budget]\n"); break; }
            text.push_str(&line);
        }
        text
    }
    pub fn len(&self) -> usize { self.cache.lock().map(|c|c.len()).unwrap_or(0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn records_real_reads_cache_hits_and_errors() {
        let root = std::env::temp_dir().join(format!("dude-evidence-{}-alpha", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("hello.rs"), "fn hello() {}").unwrap();
        let evidence = EvidenceCache::default();
        let args = json!({"path":"hello.rs"});
        assert!(evidence.execute(&root,"read_file",&args).contains("hello"));
        assert!(evidence.execute(&root,"read_file",&args).contains("hello"));
        assert!(evidence.execute(&root,"read_file",&json!({"path":"missing.rs"})).starts_with("Tool error:"));
        let summary = evidence.execution_summary();
        assert!(summary.contains("1 successful, 1 errors, 1 cache hits"));
        assert!(summary.contains("read_file(hello.rs) [ok]"));
        assert!(summary.contains("read_file(hello.rs) [cache_hit]"));
        assert!(summary.contains("read_file(missing.rs) [error]"));
        assert_eq!(evidence.successful_file_reads(), 2);
        assert_eq!(evidence.len(), 1);
        assert!(evidence.export().contains("Evidence #1: read_file"));
        assert!(!evidence.export().contains("missing.rs"));
        let _ = fs::remove_dir_all(&root);
    }
}
