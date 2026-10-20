use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}}, time::Instant};
use serde_json::{json, Value};
use crate::file_tools;

#[derive(Clone, Default)]
pub struct EvidenceCache {
    entries: Arc<Mutex<BTreeMap<String, Value>>>,
    calls: Arc<AtomicUsize>,
    hits: Arc<AtomicUsize>,
    events: Arc<Mutex<Vec<Value>>>,
    observations: Arc<Mutex<Vec<Value>>>,
}

impl EvidenceCache {
    pub fn execute(&self, root: &Path, name: &str, args: &Value) -> String {
        let started = Instant::now();
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        // Only cache individual file reads; recursive searches and listings may change
        // between calls even when their directory metadata remains unchanged.
        let fingerprint = if name == "read_file" {
            args.get("path").and_then(Value::as_str).and_then(|p| {
                let relative = Path::new(p);
                if relative.is_absolute() || relative.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
                    return None;
                }
                let path = root.join(relative);
                let meta = path.symlink_metadata().ok()?;
                if !meta.is_file() || meta.file_type().is_symlink() { return None; }
                Some(format!("{:?}:{}", meta.modified().ok()?, meta.len()))
            })
        } else { None };
        let key = fingerprint.as_ref().map(|stamp| format!("{name}:{args}:{stamp}"));
        if let Some(key) = &key
            && let Ok(cache) = self.entries.lock()
            && let Some(item) = cache.get(key) {
                self.hits.fetch_add(1, Ordering::Relaxed);
                let result = item["result"].as_str().unwrap_or_default().to_owned();
                self.record(name, args, started, true, true, &result);
                return result;
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        let outcome = file_tools::execute(&root, name, args);
        let success = outcome.is_ok();
        let result = match outcome {
            Ok(text) => text,
            Err(error) => format!("Tool error: {error}"),
        };
        // Never cache errors or a truncated version of a result as if it were complete.
        if success && result.len() <= 16000
            && let Some(key) = key
            && let Ok(mut cache) = self.entries.lock()
            && cache.len() < 80 {
                cache.insert(key, json!({"tool":name,"arguments":args,"result":result}));
        }
        self.record(name, args, started, false, success, &result);
        if success && let Ok(mut records) = self.observations.lock()
            && records.len() < 100 {
                records.push(json!({"tool":name,"arguments":args,"result":result.chars().take(16000).collect::<String>()}));
        }
        result
    }
    fn record(&self, name: &str, args: &Value, started: Instant, cache_hit: bool, success: bool, result: &str) {
        if let Ok(mut entries) = self.events.lock()
            && entries.len() < 256 {
                entries.push(json!({
                    "tool":name, "path":args.get("path").and_then(Value::as_str).unwrap_or("."),
                    "cache_hit":cache_hit, "success":success, "elapsed_ms":started.elapsed().as_millis(),
                    "start_line":args.get("start_line").and_then(Value::as_u64).unwrap_or(1),
                    "end_line":args.get("end_line").and_then(Value::as_u64),
                    "result_bytes":result.len(),
                }));
        }
    }
    pub fn events(&self) -> Vec<Value> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }
    /// Machine-generated facts about completed source reads. A successful read may be
    /// bounded or truncated; it does not imply full semantic review of a module.
    /// A reference is valid only when that exact numbered source line was returned
    /// by a successful read_file tool call in the current investigation.
    pub fn observed_reference(&self, reference: &str) -> bool {
        let Some((path, line)) = reference.trim().rsplit_once(':') else { return false; };
        let Ok(line) = line.trim().parse::<usize>() else { return false; };
        if line == 0 || path.trim().is_empty() { return false; }
        let Ok(observations) = self.observations.lock() else { return false; };
        observations.iter().any(|item| {
            item["tool"] == "read_file"
                && item["arguments"]["path"].as_str() == Some(path.trim())
                && item["result"].as_str().is_some_and(|content| {
                    let prefix = format!("{line}: ");
                    content.lines().any(|row| row.starts_with(&prefix))
                })
        })
    }
    pub fn read_manifest(&self) -> String {
        let Ok(events) = self.events.lock() else { return "Source-read manifest unavailable".into(); };
        let mut paths = BTreeSet::new();
        for entry in events.iter() {
            if entry["tool"] == "read_file" && entry["success"] == true {
                let path = entry["path"].as_str().unwrap_or("(unknown)");
                let start = entry["start_line"].as_u64().unwrap_or(1);
                let end = entry["end_line"].as_u64();
                let range = match end {
                    Some(end) => format!("{start}-{end} (requested range; actual result may be shorter)"),
                    None => format!("{start}+ (bounded tool output)"),
                };
                paths.insert(format!("{path}: {range}"));
            }
        }
        if paths.is_empty() {
            "No source files read. No source-code behavior has been verified.".into()
        } else {
            format!("READ SOURCE EXCERPTS (not verified modules; no percentage is available):\n{}",
                paths.into_iter().collect::<Vec<_>>().join("\n"))
        }
    }
    pub fn coverage(&self) -> String {
        let Ok(events) = self.events.lock() else { return "Coverage unavailable".into() };
        let mut read = BTreeSet::new();
        let mut failures = 0usize;
        for e in events.iter() {
            if e["success"] == false { failures += 1; }
            if e["tool"] == "read_file" && e["success"] == true
                && let Some(path) = e["path"].as_str() { read.insert(path.to_owned()); }
        }
        format!("Source reads: {}. Files: {}. Tool errors: {}. This is sampled coverage, not verification of all modules.",
            read.len(), if read.is_empty() { "(none)".into() } else { read.into_iter().collect::<Vec<_>>().join(", ") }, failures)
    }
    pub fn export(&self) -> String {
        let Ok(cache) = self.observations.lock() else { return "Evidence records unavailable".into() };
        let mut text = String::new();
        for (index, item) in cache.iter().enumerate() {
            let line = format!("Evidence #{}: {}({})\n{}\n\n",index+1,item["tool"].as_str().unwrap_or("unknown"),item["arguments"],item["result"].as_str().unwrap_or(""));
            if text.len()+line.len()>28000 { text.push_str("[Further evidence omitted due to context budget]\n"); break; }
            text.push_str(&line);
        }
        text.push_str(&format!("\n{}\n{}\n",self.coverage(),self.read_manifest()));
        text
    }
    pub fn len(&self) -> usize { self.observations.lock().map(|c|c.len()).unwrap_or(0) }
    pub fn calls(&self) -> usize { self.calls.load(Ordering::Relaxed) }
    pub fn hits(&self) -> usize { self.hits.load(Ordering::Relaxed) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_reads_cache_and_invalidate() {
        let root = std::env::temp_dir().join(format!("dude-evidence-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("file.rs"),"one").unwrap();
        let cache = EvidenceCache::default();
        let args = json!({"path":"file.rs"});
        assert!(cache.execute(&root,"read_file",&args).contains("one"));
        assert!(cache.execute(&root,"read_file",&args).contains("one"));
        assert_eq!(cache.hits(),1);
        std::fs::write(root.join("file.rs"),"two two").unwrap();
        assert!(cache.execute(&root,"read_file",&args).contains("two two"));
        assert_eq!(cache.calls(),2);
        assert!(cache.coverage().contains("file.rs"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
