use std::{collections::{BTreeMap, BTreeSet}, path::Path, sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}}, time::Instant};
use serde_json::{json, Value};
use crate::file_tools;

const MAX_CACHED_RESULTS: usize = 80;
const MAX_CACHED_RESULT_BYTES: usize = 16_000;
const MAX_OBSERVATIONS: usize = 100;

/// Canonical EvidenceCache boundaries, distinct from file-tool operation limits.
pub fn limits_description() -> String {
    format!(
        "EvidenceCache limits: at most {MAX_CACHED_RESULTS} successful results are cached when each is no more than {MAX_CACHED_RESULT_BYTES} bytes; at most {MAX_OBSERVATIONS} observations are retained, each excerpt limited to {MAX_CACHED_RESULT_BYTES} characters. These are not file-tool operation limits."
    )
}

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
        self.execute_with_origin(root, name, args, "model_tool")
    }

    /// Deterministic source reading for broad project requests. This is context
    /// supplied by DUDE, not a tool call selected by the model.
    pub fn execute_bootstrap(&self, root: &Path, name: &str, args: &Value) -> String {
        self.execute_with_origin(root, name, args, "automatic_bootstrap")
    }

    fn execute_with_origin(&self, root: &Path, name: &str, args: &Value, origin: &str) -> String {
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
                self.record(name, args, started, true, true, &result, origin);
                let _ = crate::progress::append_with_origin(&root, name, args, true, true, origin);
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
        if success && result.len() <= MAX_CACHED_RESULT_BYTES
            && let Some(key) = key
            && let Ok(mut cache) = self.entries.lock()
            && cache.len() < MAX_CACHED_RESULTS {
                cache.insert(key, json!({"tool":name,"arguments":args,"result":result}));
        }
        self.record(name, args, started, false, success, &result, origin);
        let _ = crate::progress::append_with_origin(&root, name, args, success, false, origin);
        if success && let Ok(mut records) = self.observations.lock()
            && records.len() < MAX_OBSERVATIONS {
                records.push(json!({"tool":name,"arguments":args,"origin":origin,"result":result.chars().take(MAX_CACHED_RESULT_BYTES).collect::<String>()}));
        }
        result
    }
    fn record(&self, name: &str, args: &Value, started: Instant, cache_hit: bool, success: bool, result: &str, origin: &str) {
        if let Ok(mut entries) = self.events.lock()
            && entries.len() < 256 {
                entries.push(json!({
                    "tool":name, "path":args.get("path").and_then(Value::as_str).unwrap_or("."), "origin":origin,
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
    /// A citation is verified only if every numbered line of its range was
    /// returned by a successful read_file. Supports file:42 and file:42-48.
    /// Reject large, malformed and partially observed ranges rather than
    /// converting them into false claims of verification.
    pub fn observed_reference(&self, reference: &str) -> bool {
        let reference = reference.trim();
        let reference = reference.strip_prefix("automatic_bootstrap:")
            .or_else(|| reference.strip_prefix("model_tool:"))
            .unwrap_or(reference);
        let Some((path, line_spec)) = reference.rsplit_once(':') else { return false; };
        if path.trim().is_empty() { return false; }
        let (start, end) = if let Some((first, last)) = line_spec.split_once('-') {
            let (Ok(first), Ok(last)) = (first.parse::<usize>(), last.parse::<usize>()) else {
                return false;
            };
            (first, last)
        } else {
            let Ok(line) = line_spec.parse::<usize>() else { return false; };
            (line, line)
        };
        if start == 0 || end < start || end - start > 200 { return false; }
        let Ok(observations) = self.observations.lock() else { return false; };
        let mut seen = BTreeSet::new();
        for item in observations.iter() {
            if item["tool"] != "read_file" || item["arguments"]["path"].as_str() != Some(path.trim()) {
                continue;
            }
            if let Some(content) = item["result"].as_str() {
                for row in content.lines() {
                    if let Some((number, _)) = row.split_once(": ")
                        && let Ok(number) = number.parse::<usize>()
                        && (start..=end).contains(&number) {
                            seen.insert(number);
                    }
                }
            }
        }
        (start..=end).all(|line| seen.contains(&line))
    }
    /// Find source files named in inline Markdown code that were not actually
    /// read during this request. A real read of src/main.rs satisfies a
    /// shorthand mention of `main.rs`, but listing files does NOT satisfy it.
    /// This only detects evidence *absence*, never validates claims about files
    /// that were read (a semantic audit may still be needed).
    pub fn uninspected_source_mentions(&self, report: &str) -> Vec<String> {
        let Ok(events) = self.events.lock() else { return Vec::new() };
        let inspected: BTreeSet<String> = events.iter()
            .filter(|event| event["tool"] == "read_file" && event["success"] == true)
            .filter_map(|event| event["path"].as_str())
            .map(|path| path.trim_start_matches("./").replace('\\', "/"))
            .collect();
        let mut unread = BTreeSet::new();
        for (index, snippet) in report.split('`').enumerate() {
            // Only explicit Markdown inline source-file mentions; not code
            // blocks, arbitrary prose, tool calls, or command arguments.
            if index % 2 == 0 { continue; }
            let snippet = snippet.trim();
            if snippet.len() > 180 || snippet.contains(char::is_whitespace) { continue; }
            let without_lines = snippet.rsplit_once(':')
                .filter(|(_, lines)| !lines.is_empty() &&
                    lines.chars().all(|ch| ch.is_ascii_digit() || ch == '-'))
                .map(|(path, _)| path).unwrap_or(snippet);
            let file = without_lines.trim_start_matches("./");
            if file.is_empty() || file.contains("..") || file.contains('*') ||
                file.contains('{') || file.contains('}') || file.ends_with('/') {
                continue;
            }
            let extension = Path::new(file).extension()
                .and_then(|extension| extension.to_str()).unwrap_or("");
            if !matches!(extension,
                "rs" | "java" | "php" | "py" | "js" | "jsx" | "ts" |
                "tsx" | "c" | "h" | "cpp" | "hpp" | "go" | "rb" |
                "sql" | "toml" | "yml" | "yaml" | "md" | "json"
            ) { continue; }
            let checked = inspected.iter().any(|path| {
                path == file || path.ends_with(&format!("/{file}"))
            });
            if !checked {
                unread.insert(file.to_owned());
            }
        }
        unread.into_iter().collect()
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
                let origin = entry["origin"].as_str().unwrap_or("unknown");
                paths.insert(format!("[{origin}] {path}: {range}"));
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
        let mut bootstrap_reads = 0usize;
        let mut model_reads = 0usize;
        let mut other_reads = 0usize;
        let mut failures = 0usize;
        for e in events.iter() {
            if e["success"] == false { failures += 1; }
            if e["tool"] == "read_file" && e["success"] == true
                && let Some(path) = e["path"].as_str() {
                    read.insert(path.to_owned());
                    match e["origin"].as_str() {
                        Some("automatic_bootstrap") => bootstrap_reads += 1,
                        Some("model_tool") => model_reads += 1,
                        _ => other_reads += 1,
                    }
                }
        }
        format!("Source reads: {} (automatic bootstrap: {}; model-requested tool calls: {}; other: {}). Files: {}. Tool errors: {}. This is sampled coverage, not verification of all modules.",
            bootstrap_reads + model_reads + other_reads, bootstrap_reads, model_reads, other_reads,
            if read.is_empty() { "(none)".into() } else { read.into_iter().collect::<Vec<_>>().join(", ") }, failures)
    }
    pub fn export(&self) -> String {
        let Ok(cache) = self.observations.lock() else { return "Evidence records unavailable".into() };
        let mut text = String::new();
        for (index, item) in cache.iter().enumerate() {
            let line = format!("Evidence #{} [{}]: {}({})\n{}\n\n",index+1,item["origin"].as_str().unwrap_or("unknown"),item["tool"].as_str().unwrap_or("unknown"),item["arguments"],item["result"].as_str().unwrap_or(""));
            if text.len()+line.len()>28000 { text.push_str("[Further evidence omitted due to context budget]\n"); break; }
            text.push_str(&line);
        }
        text.push_str(&format!("\n{}\n{}\n",self.coverage(),self.read_manifest()));
        text
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
                let span: String = remainder.chars()
                    .take_while(|ch| ch.is_ascii_digit() || *ch == '-').collect();
                if !span.is_empty() && self.observed_reference(&format!("{path}:{span}")) {
                    return true;
                }
            }
        }
        false
    }
    /// Distinct source file paths read since the supplied event index.
    /// This excludes directory listings and reads from prior stages.
    pub fn source_files_since(&self, event_start: usize) -> usize {
        self.events.lock().map(|events| events.iter().skip(event_start)
            .filter(|e| e["tool"] == "read_file" && e["success"] == true)
            .filter_map(|e| e["path"].as_str())
            .collect::<BTreeSet<_>>().len()).unwrap_or(0)
    }
    pub fn event_count(&self) -> usize {
        self.events.lock().map(|events| events.len()).unwrap_or(0)
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
    fn source_mentions_require_actual_reads_not_just_directory_listing() {
        let root = std::env::temp_dir().join(format!(
            "dude-mention-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("src/file_tools.rs"), "pub fn execute() {}").unwrap();
        std::fs::write(root.join("src/knowledge.rs"), "pub fn approve() {}").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "list_files", &json!({"path":"src"}));
        evidence.execute(&root, "read_file", &json!({"path":"./src/main.rs"}));
        evidence.execute(&root, "read_file", &json!({"path":"src/file_tools.rs"}));
        let response = "Inspecionei `main.rs` e `src/file_tools.rs:1`.             O módulo `knowledge.rs` possui as funções `execute`, `resolve` e `scan`.             Use `cargo check` para compilar.";
        assert_eq!(evidence.uninspected_source_mentions(response), vec!["knowledge.rs"]);
        evidence.execute(&root, "read_file", &json!({"path":"src/knowledge.rs"}));
        assert!(evidence.uninspected_source_mentions(response).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn source_mentions_only_check_explicit_file_names() {
        let evidence = EvidenceCache::default();
        assert!(evidence.uninspected_source_mentions(
            "Faça `cargo run`, depois use `/index status`."
        ).is_empty());
        assert_eq!(evidence.uninspected_source_mentions(
            "O arquivo `src/knowledge.rs` gerencia memória e `Cargo.toml` define dependências."
        ), vec!["Cargo.toml", "src/knowledge.rs"]);
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
    fn citation_ranges_require_every_observed_line() {
        let root = std::env::temp_dir().join(format!("dude-citation-range-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("module.rs"), "a\nb\nc\nd\ne\nf\n").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "read_file", &json!({"path":"module.rs","start_line":2,"end_line":4}));
        assert!(evidence.observed_reference("module.rs:2-4"));
        assert!(!evidence.observed_reference("module.rs:1-4"));
        assert!(!evidence.observed_reference("module.rs:2-5"));
        assert!(!evidence.observed_reference("module.rs:4-2"));
        assert!(!evidence.observed_reference("module.rs:2-9999"));
        assert!(!evidence.observed_reference("module.rs:2-foo"));
        assert!(evidence.has_supported_citation("Read module.rs:2-4"));
        assert!(!evidence.has_supported_citation("Unobserved module.rs:1-4"));
        evidence.execute(&root, "read_file", &json!({"path":"module.rs","start_line":5,"end_line":5}));
        assert!(evidence.observed_reference("module.rs:2-5"));
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
    fn distinct_source_files_after_bootstrap_are_counted_by_runtime() {
        let root = std::env::temp_dir().join(format!(
            "dude-source-delta-{}", std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(root.join("b.rs"), "fn b() {}\n").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "read_file", &json!({"path":"a.rs"}));
        let after_bootstrap = evidence.event_count();
        assert_eq!(evidence.source_files_since(after_bootstrap), 0);
        evidence.execute(&root, "list_files", &json!({"path":"."}));
        evidence.execute(&root, "read_file", &json!({"path":"a.rs"}));
        evidence.execute(&root, "read_file", &json!({"path":"a.rs"}));
        assert_eq!(evidence.source_files_since(after_bootstrap), 1);
        evidence.execute(&root, "read_file", &json!({"path":"b.rs"}));
        assert_eq!(evidence.source_files_since(after_bootstrap), 2);
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
    #[test]
    fn coverage_distinguishes_bootstrap_from_model_tool_reads() {
        let root = std::env::temp_dir().join(format!("dude-origin-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("bootstrap.rs"), "bootstrap").unwrap();
        std::fs::write(root.join("model.rs"), "model").unwrap();
        let cache = EvidenceCache::default();
        cache.execute_bootstrap(&root, "read_file", &json!({"path":"bootstrap.rs"}));
        cache.execute(&root, "read_file", &json!({"path":"model.rs"}));
        let coverage = cache.coverage();
        assert!(coverage.contains("automatic bootstrap: 1"));
        assert!(coverage.contains("model-requested tool calls: 1"));
        assert!(cache.read_manifest().contains("[automatic_bootstrap] bootstrap.rs"));
        assert!(cache.read_manifest().contains("[model_tool] model.rs"));
        assert!(cache.observed_reference("automatic_bootstrap:bootstrap.rs:1"));
        assert!(cache.observed_reference("model_tool:model.rs:1"));
        assert!(!cache.observed_reference("unknown:bootstrap.rs:1"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exposes_cache_limits_without_claiming_file_tool_limits() {
        let description = limits_description();
        assert!(description.contains("80"));
        assert!(description.contains("16000"));
        assert!(description.contains("100"));
        assert!(description.contains("not file-tool operation limits"));
    }
}
