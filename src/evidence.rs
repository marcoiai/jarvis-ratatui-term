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
                let _ = crate::progress::append(&root, name, args, true, true);
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
        let _ = crate::progress::append(&root, name, args, success, false);
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
        let Some((path, spec)) = reference.trim().rsplit_once(':') else { return false; };
        if path.trim().is_empty() { return false; }
        let (start, end) = match spec.split_once('-') {
            Some((start, end)) => {
                let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) else {
                    return false;
                };
                (start, end)
            }
            None => {
                let Ok(line) = spec.parse::<usize>() else { return false; };
                (line, line)
            }
        };
        // Require EVERY source line in the cited range to have appeared in
        // successful read_file output. A requested range alone is not evidence.
        if start == 0 || end < start || end - start > 79 { return false; }
        let Ok(observations) = self.observations.lock() else { return false; };
        let mut seen = BTreeSet::new();
        for item in observations.iter().filter(|item| {
            item["tool"] == "read_file" && item["arguments"]["path"].as_str() == Some(path.trim())
        }) {
            if let Some(content) = item["result"].as_str() {
                for row in content.lines() {
                    let Some((number, _)) = row.split_once(": ") else { continue; };
                    if let Ok(number) = number.parse::<usize>()
                        && (start..=end).contains(&number)
                    {
                        seen.insert(number);
                    }
                }
            }
        }
        seen.len() == end - start + 1
    }
    /// Collect declaration candidates exclusively from numbered read_file source lines.
    /// This is a syntax observation, not proof of runtime behavior.
    pub fn declaration_drafts(&self, limit: usize) -> Vec<crate::knowledge::CandidateDraft> {
        let Ok(observations) = self.observations.lock() else { return Vec::new(); };
        let mut drafts = Vec::new();
        let mut seen = BTreeSet::new();
        for item in observations.iter() {
            if item["tool"] != "read_file" { continue; }
            let Some(path) = item["arguments"]["path"].as_str() else { continue; };
            if !(path.ends_with(".rs") || path.ends_with(".java")
                || path.ends_with(".php") || path.ends_with(".py")) { continue; }
            let Some(content) = item["result"].as_str() else { continue; };
            for row in content.lines() {
                let Some((number, line)) = row.split_once(": ") else { continue; };
                let Ok(number) = number.parse::<usize>() else { continue; };
                if number == 0 { continue; }
                let declaration = line.trim();
                let is_declaration = ["pub fn ", "pub async fn ", "pub struct ", "pub enum ",
                    "pub trait ", "class ", "def ", "async def ", "public class ",
                    "public interface "].iter().any(|prefix| declaration.starts_with(prefix));
                if !is_declaration || declaration.len() > 180 { continue; }
                let reference = format!("{path}:{number}");
                if !seen.insert(reference.clone()) { continue; }
                drafts.push(crate::knowledge::CandidateDraft {
                    classification: "documented".into(),
                    claim: format!("Source declaration in {path}: {}", declaration),
                    evidence: vec![reference],
                });
                if drafts.len() >= limit { return drafts; }
            }
        }
        drafts
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
    /// Bounded evidence excerpts distributed across all successfully read files.
    /// Unlike taking the FIRST bytes of export(), later files remain visible.
    /// This is a MODEL INPUT sample, never a substitute for validating citations
    /// against the full cache of observed numbered source lines.
    pub fn semantic_audit_sample(&self, budget: usize) -> String {
        let Ok(observations) = self.observations.lock() else {
            return "Observações indisponíveis.".into();
        };
        let reads: Vec<&Value> = observations.iter()
            .filter(|item| item["tool"] == "read_file")
            .collect();
        if reads.is_empty() {
            return "Nenhum trecho de arquivo foi lido com sucesso.".into();
        }
        let per_read = budget.saturating_sub(700) / reads.len();
        let mut output = format!(
            "Amostra distribuída de {} leituras (trechos podem estar truncados):\n",
            reads.len()
        );
        for item in reads {
            let path = item["arguments"]["path"].as_str().unwrap_or("(arquivo desconhecido)");
            let heading = format!("\nFONTE: {path}\n");
            if output.len() + heading.len() > budget { break; }
            output.push_str(&heading);
            let available = per_read.min(budget.saturating_sub(output.len()));
            let mut used = 0usize;
            let mut omitted = false;
            if let Some(excerpt) = item["result"].as_str() {
                for line in excerpt.lines() {
                    let needed = line.chars().count() + 1;
                    if used + needed > available {
                        omitted = true;
                        break;
                    }
                    output.push_str(line);
                    output.push('\n');
                    used += needed;
                }
            }
            if omitted { output.push_str("[Outras linhas omitidas desta fonte]\n"); }
        }
        output
    }

    /// Whether the report contains at least one file:line pointing to a line
    /// actually returned by read_file. This is citation presence, NOT claim validation.
    pub fn has_supported_citation(&self, report: &str) -> bool {
        let Ok(events) = self.events.lock() else { return false; };
        let paths: BTreeSet<String> = events.iter()
            .filter(|event| event["tool"] == "read_file" && event["success"] == true)
            .filter_map(|event| event["path"].as_str().map(str::to_owned))
            .collect();
        drop(events);
        for path in paths {
            let needle = format!("{path}:");
            for (offset, _) in report.match_indices(&needle) {
                let remainder = &report[offset + needle.len()..];
                let digits: String = remainder.chars().take_while(|ch| ch.is_ascii_digit()).collect();
                if digits.is_empty() { continue; }
                let rest = &remainder[digits.len()..];
                let range_end: String = rest.strip_prefix('-').unwrap_or("")
                    .chars().take_while(|ch| ch.is_ascii_digit()).collect();
                let spec = if range_end.is_empty() {
                    digits
                } else {
                    format!("{digits}-{range_end}")
                };
                if self.observed_reference(&format!("{path}:{spec}")) {
                    return true;
                }
            }
        }
        false
    }
    pub fn source_read_count(&self) -> usize {
        self.events.lock().map(|events| events.iter()
            .filter(|event| event["tool"] == "read_file" && event["success"] == true).count())
            .unwrap_or(0)
    }
    pub fn len(&self) -> usize { self.observations.lock().map(|c|c.len()).unwrap_or(0) }
    pub fn calls(&self) -> usize { self.calls.load(Ordering::Relaxed) }
    pub fn hits(&self) -> usize { self.hits.load(Ordering::Relaxed) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_ranges_require_every_numbered_line() {
        let root = std::env::temp_dir().join(format!("dude-evidence-range-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("range.rs"), "a\nb\nc\nd\ne\n").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "read_file", &json!({"path":"range.rs","start_line":2,"end_line":3}));
        assert!(evidence.observed_reference("range.rs:2-3"));
        assert!(evidence.has_supported_citation("Trecho range.rs:2-3 foi lido."));
        assert!(!evidence.has_supported_citation("Trecho range.rs:2-5 foi lido."));
        assert!(!evidence.observed_reference("range.rs:1-3"));
        assert!(!evidence.observed_reference("range.rs:2-4"));
        assert!(!evidence.observed_reference("range.rs:0-1"));
        assert!(!evidence.observed_reference("range.rs:3-2"));
        assert!(!evidence.observed_reference("range.rs:2-500"));
        evidence.execute(&root, "read_file", &json!({"path":"range.rs","start_line":4,"end_line":5}));
        assert!(evidence.observed_reference("range.rs:2-5"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_sample_includes_later_sources_in_bounded_context() {
        let root = std::env::temp_dir().join(format!("dude-audit-sample-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let evidence = EvidenceCache::default();
        for i in 0..12 {
            let path = format!("file{i}.rs");
            std::fs::write(root.join(&path), format!("pub fn example_{i}() {{}}\n").repeat(100)).unwrap();
            evidence.execute(&root, "read_file", &json!({"path":path}));
        }
        let sample = evidence.semantic_audit_sample(12000);
        assert!(sample.contains("FONTE: file0.rs"));
        assert!(sample.contains("FONTE: file11.rs"));
        assert!(sample.len() < 14000);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_observed_source_lines_can_be_cited() {
        let root = std::env::temp_dir().join(format!("dude-reference-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("example.rs"), "first\nsecond\nthird\n").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "read_file", &json!({"path":"example.rs","start_line":2,"end_line":2}));
        assert!(evidence.observed_reference("example.rs:2"));
        assert!(evidence.declaration_drafts(6).is_empty());
        assert!(evidence.has_supported_citation("Implementação em example.rs:2"));
        assert!(!evidence.has_supported_citation("Implementação em example.rs:1"));
        assert!(!evidence.has_supported_citation("Sem referências"));
        assert!(!evidence.observed_reference("example.rs:1"));
        assert!(!evidence.observed_reference("other.rs:2"));
        assert!(!evidence.observed_reference("example.rs"));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn declaration_drafts_require_actual_read_file_lines() {
        let root=std::env::temp_dir().join(format!("dude-decl-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("sample.rs"),"pub struct Agent;\nfn private() {}\npub fn run() {}\n").unwrap();
        let evidence=EvidenceCache::default();
        evidence.execute(&root,"read_file",&json!({"path":"sample.rs"}));
        let drafts=evidence.declaration_drafts(6);
        assert_eq!(drafts.len(),2);
        assert_eq!(drafts[0].evidence,vec!["sample.rs:1"]);
        assert_eq!(drafts[1].evidence,vec!["sample.rs:3"]);
        assert!(drafts.iter().all(|d|evidence.observed_reference(&d.evidence[0])));
        std::fs::remove_dir_all(root).unwrap();
    }
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
