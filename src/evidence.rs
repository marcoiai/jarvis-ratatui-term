use std::{collections::BTreeMap, path::Path, sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}}};
use serde_json::{json, Value};
use crate::file_tools;

#[derive(Clone, Default)]
pub struct EvidenceCache(Arc<Mutex<BTreeMap<String, Value>>>, Arc<AtomicUsize>, Arc<AtomicUsize>);

impl EvidenceCache {
    pub fn execute(&self, root: &Path, name: &str, args: &Value) -> String {
        let key = format!("{name}:{}", args);
        if let Ok(cache) = self.0.lock() {
            if let Some(item) = cache.get(&key) {
                self.2.fetch_add(1, Ordering::Relaxed);
                return item["result"].as_str().unwrap_or_default().to_owned();
            }
        }
        self.1.fetch_add(1, Ordering::Relaxed);
        let result = match file_tools::execute(root, name, args) {
            Ok(result) => result,
            Err(error) => return format!("Tool error: {error}"),
        };
        if let Ok(mut cache) = self.0.lock() {
            if cache.len() < 80 {
                cache.insert(key, json!({"tool":name,"arguments":args,"result":result.chars().take(16000).collect::<String>()}));
            }
        }
        result
    }

    pub fn export(&self) -> String {
        let Ok(cache) = self.0.lock() else { return "Evidence cache unavailable".into() };
        let mut text = String::new();
        for (index, item) in cache.values().enumerate() {
            let line = format!("Evidence #{}: {}({})\n{}\n\n",index+1,item["tool"].as_str().unwrap_or("unknown"),item["arguments"],item["result"].as_str().unwrap_or(""));
            if text.len()+line.len()>28000 { text.push_str("[Further evidence omitted due to context budget]\n"); break; }
            text.push_str(&line);
        }
        text
    }
    pub fn len(&self) -> usize { self.0.lock().map(|c|c.len()).unwrap_or(0) }
    pub fn calls(&self) -> usize { self.1.load(Ordering::Relaxed) }
    pub fn hits(&self) -> usize { self.2.load(Ordering::Relaxed) }
}
