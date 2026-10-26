//! Regression coverage for selectively restored branch work.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = env::temp_dir().join(format!("dude-consolidation-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

async fn server(replies: Vec<(u16, String)>) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = Vec::new();
        for (status, body) in replies {
            let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept()).await.unwrap().unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") { break end + 4; }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
            let length: usize = headers.lines().find_map(|line| line.strip_prefix("content-length:")).unwrap().trim().parse().unwrap();
            while bytes.len() < header_end + length {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            requests.push(serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap());
            let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            socket.write_all(response.as_bytes()).await.unwrap();
        }
        requests
    });
    (host, task)
}

fn reply(text: &str) -> String {
    json!({"message":{"content":text},"prompt_eval_count":7,"eval_count":3}).to_string()
}

async fn ask(host: &str, workspace: &Workspace, tools: bool, cache: evidence::EvidenceCache, metrics: metrics::Metrics) -> Result<String, String> {
    let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
    ask_ollama(&client, host, "test-model", vec![Message {role:"user".into(), content:"review".into()}],
        workspace.0.clone(), tools, cache, metrics, None).await
}

#[tokio::test]
async fn parser_recovery_keeps_observations_filters_reasoning_and_measures_requests() {
    let workspace = Workspace::new();
    std::fs::write(workspace.0.join("sample.rs"), "fn main() {}\n").unwrap();
    let cache = evidence::EvidenceCache::default();
    cache.execute(&workspace.0, "read_file", &json!({"path":"sample.rs"}));
    let metrics = metrics::Metrics::default();
    let (host, task) = server(vec![
        (500, json!({"error":"XML syntax error <function>"}).to_string()),
        (200, reply("<think>private reasoning</think>Observed sample.rs:1")),
    ]).await;
    let result = ask(&host, &workspace, true, cache.clone(), metrics.clone()).await.unwrap();
    assert!(result.contains("Análise parcial"));
    assert!(result.contains("sample.rs:1"));
    assert!(!result.contains("private reasoning"));
    assert_eq!(cache.calls(), 1, "recovery must not repeat completed reads");
    let requests = task.await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0]["tools"].as_array().unwrap().is_empty());
    assert!(requests[1]["tools"].as_array().unwrap().is_empty());
    assert!(requests[1]["messages"].to_string().contains("fn main()"));
    metrics.finish(&workspace.0, "solo", Instant::now(), true, &cache).unwrap();
    let record: Value = serde_json::from_str(&metrics::recent(&workspace.0)).unwrap();
    assert_eq!(record["stages"].as_array().unwrap().len(), 2);
    assert_eq!(record["stages"][0]["success"], false);
    assert!(record["stages"][0]["name"].as_str().unwrap().contains("payload_bytes_"));
    assert_eq!(record["stages"][1]["name"], "ollama_recovery");
    assert_eq!(record["stages"][1]["success"], true);
}

#[tokio::test]
async fn parser_recovery_without_sources_fails_closed() {
    let workspace = Workspace::new();
    let (host, task) = server(vec![(500, "XML syntax error <function>".into())]).await;
    let result = ask(&host, &workspace, true, evidence::EvidenceCache::default(), metrics::Metrics::default()).await.unwrap_err();
    assert!(result.contains("No source lines"));
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn parser_recovery_rejects_hidden_only_and_tool_responses() {
    for body in [reply("<think>hidden only</think>"), json!({"message":{"content":"untrusted", "tool_calls":[{"function":{"name":"read_file","arguments":{"path":"sample.rs"}}}]}}).to_string()] {
        let workspace = Workspace::new();
        std::fs::write(workspace.0.join("sample.rs"), "source\n").unwrap();
        let cache = evidence::EvidenceCache::default();
        cache.execute(&workspace.0, "read_file", &json!({"path":"sample.rs"}));
        let (host, task) = server(vec![(500, "XML syntax error <function>".into()), (200, body)]).await;
        assert!(ask(&host, &workspace, true, cache.clone(), metrics::Metrics::default()).await.is_err());
        assert_eq!(cache.calls(), 1);
        task.await.unwrap();
    }
}

#[tokio::test]
async fn empty_response_retries_once_and_counts_both_token_usages() {
    let workspace = Workspace::new();
    let cache = evidence::EvidenceCache::default();
    let metrics = metrics::Metrics::default();
    let (host, task) = server(vec![(200, reply("")), (200, reply("answer"))]).await;
    assert_eq!(ask(&host, &workspace, false, cache.clone(), metrics.clone()).await.unwrap(), "answer");
    let requests = task.await.unwrap();
    assert!(requests.iter().all(|r| r["tools"].as_array().unwrap().is_empty()));
    metrics.finish(&workspace.0, "solo", Instant::now(), true, &cache).unwrap();
    let record: Value = serde_json::from_str(&metrics::recent(&workspace.0)).unwrap();
    assert_eq!(record["token_usage"].as_array().unwrap().len(), 2);
    assert_eq!(record["stages"].as_array().unwrap().len(), 2);
    assert!(record["stages"][1]["name"].as_str().unwrap().starts_with("ollama_retry_empty_response_"));
}

#[tokio::test]
async fn invalid_json_is_recorded_as_a_failed_request() {
    let workspace = Workspace::new();
    let cache = evidence::EvidenceCache::default();
    let metrics = metrics::Metrics::default();
    let (host, task) = server(vec![(200, "not JSON".into())]).await;
    assert!(ask(&host, &workspace, false, cache.clone(), metrics.clone()).await.is_err());
    task.await.unwrap();
    metrics.finish(&workspace.0, "solo", Instant::now(), false, &cache).unwrap();
    let record: Value = serde_json::from_str(&metrics::recent(&workspace.0)).unwrap();
    assert_eq!(record["stages"][0]["success"], false);
}

#[test]
fn excerpt_metadata_keeps_evidence_line_numbers_and_exposes_truncation() {
    let workspace = Workspace::new();
    std::fs::write(workspace.0.join("sample.rs"), "first\nsecond\nthird\n").unwrap();
    let cache = evidence::EvidenceCache::default();
    cache.execute(&workspace.0, "read_file", &json!({"path":"sample.rs","start_line":2,"end_line":2}));
    assert!(cache.observed_reference("sample.rs:2"));
    assert!(!cache.observed_reference("sample.rs:1"));
    let missing = file_tools::execute(&workspace.0, "read_file", &json!({"path":"sample.rs","start_line":9})).unwrap();
    assert!(missing.contains("lines none of 3 | complete_file=false"));
    std::fs::write(workspace.0.join("long.rs"), "á".repeat(40_000)).unwrap();
    let long = file_tools::execute(&workspace.0, "read_file", &json!({"path":"long.rs"})).unwrap();
    assert!(long.contains("complete_file=false | output_truncated=true"));
}

#[test]
fn listing_does_not_expose_protected_names() {
    let workspace = Workspace::new();
    for name in [".env", ".env.production", "id_ed25519", "visible.rs"] {
        std::fs::write(workspace.0.join(name), "sample").unwrap();
    }
    for name in [".dude", ".git", ".ssh", ".aws"] { std::fs::create_dir(workspace.0.join(name)).unwrap(); }
    let listed = file_tools::execute(&workspace.0, "list_files", &json!({"path":"."})).unwrap();
    assert_eq!(listed, "visible.rs");
}

#[cfg(unix)]
#[tokio::test]
async fn explicit_commands_run_in_selected_workspace() {
    let workspace = Workspace::new();
    let expected = workspace.0.canonicalize().unwrap();
    let output = run_command("pwd".into(), expected.clone()).await.unwrap();
    assert_eq!(output.trim(), expected.to_string_lossy());
}

#[test]
fn role_models_survive_config_and_pending_request_recovery() {
    let workspace = Workspace::new();
    let legacy = r#"{"investigator":"ollama","architect":"gemini","reviewer":"groq"}"#;
    let mut roles: roles::Roles = serde_json::from_str(legacy).unwrap();
    assert_eq!(roles.model_for("investigator", "default"), "default");
    roles.set_model("investigator", "local:4b").unwrap();
    roles.set_model("architect", "remote-model").unwrap();
    roles.save(&workspace.0).unwrap();
    assert_eq!(roles::Roles::load(&workspace.0).unwrap(), roles);
    let memory = memory::Memory::open(&workspace.0).unwrap();
    memory.save_pending(&memory::PendingRequest {
        question: "review".into(), roles: Some(roles.clone()),
        ollama_model: Some("default".into()), groq_model: None,
        gemini_model: None, cloud_was_allowed: false,
    }).unwrap();
    assert_eq!(memory.pending().unwrap().unwrap().roles.unwrap(), roles);
    assert!(roles.set_model("unknown", "valid-model").is_err());
    assert!(roles.set_model("reviewer", "invalid\nmodel").is_err());
}

#[tokio::test]
async fn symphony_uses_role_models_preserves_evidence_and_checkpoints_before_failure() {
    let workspace = Workspace::new();
    std::fs::write(workspace.0.join("sample.rs"), "observed source\n").unwrap();
    let cache = evidence::EvidenceCache::default();
    let (host, task) = server(vec![
        (200, json!({"message":{"tool_calls":[{"function":{"name":"read_file","arguments":{"path":"sample.rs"}}}]}}).to_string()),
        (200, reply("Investigator sample.rs:1")),
        (200, reply("Architect sample.rs:1")),
        (503, "unavailable".into()),
    ]).await;
    let mut roles = roles::Roles::default();
    for (role, model) in [("investigator", "investigator:4b"), ("architect", "architect:8b"), ("reviewer", "reviewer:4b")] {
        roles.set(role, "ollama").unwrap();
        roles.set_model(role, model).unwrap();
    }
    let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
    let (updates, _receiver) = mpsc::unbounded_channel();
    let result = run_symphony(&client, &roles,
        vec![Message {role:"user".into(), content:"review".into()}],
        workspace.0.clone(), &host, "default", "unused", "unused", cache.clone(), metrics::Metrics::default(), updates).await;
    assert!(result.unwrap_err().contains("Reviewer (ollama) failed"));
    let requests = task.await.unwrap();
    assert_eq!(requests[0]["model"], "investigator:4b");
    assert_eq!(requests[1]["model"], "investigator:4b");
    assert_eq!(requests[2]["model"], "architect:8b");
    assert_eq!(requests[3]["model"], "reviewer:4b");
    assert!(requests[2]["tools"].as_array().unwrap().is_empty());
    assert!(requests[3]["tools"].as_array().unwrap().is_empty());
    assert!(requests[3]["messages"].to_string().contains("observed source"));
    assert_eq!(cache.calls(), 1);
    let checkpoint: Value = serde_json::from_slice(&std::fs::read(workspace.0.join(".dude/symphony_checkpoint.json")).unwrap()).unwrap();
    assert_eq!(checkpoint["stage"], "Architect");
    assert_eq!(checkpoint["model"], "architect:8b");
    assert_eq!(checkpoint["question"], "review");
    assert!(checkpoint["note"].as_str().unwrap().contains("not validated for automatic resume"));
}

#[tokio::test]
async fn focused_source_inspection_without_native_reads_fails_before_architect() {
    let workspace = Workspace::new();
    std::fs::create_dir_all(workspace.0.join("src")).unwrap();
    std::fs::write(workspace.0.join("Cargo.toml"),
        "[package]\\nname = \\"dude-fixture\\"\\nversion = \\"0.1.0\\"\\nedition = \\"2024\\"\\n").unwrap();
    std::fs::write(workspace.0.join("src/main.rs"),
        "fn main() { println!(\\"hello\\"); }\\n").unwrap();
    indexer::refresh(&workspace.0).unwrap();
    let (host, task) = server(vec![
        (200, reply("I cannot find any source lines for this request.")),
        (200, reply("No source files could be examined.")),
    ]).await;
    let mut roles = roles::Roles::default();
    for name in ["investigator", "architect", "reviewer"] {
        roles.set(name, "ollama").unwrap();
        roles.set_model(name, "test-model").unwrap();
    }
    let metrics = metrics::Metrics::default();
    let evidence = evidence::EvidenceCache::default();
    let client = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
    let (updates, _receiver) = mpsc::unbounded_channel();
    let question = "Investigue como o DUDE executa chamadas de ferramentas no Ollama. Localize as funções relevantes pelo índice, leia os arquivos e explique o fluxo com referências de linha.";
    let result = run_symphony(&client, &roles,
        vec![Message {role:"user".into(), content:question.into()}],
        workspace.0.clone(), &host, "test-model", "unused", "unused",
        evidence.clone(), metrics.clone(), updates).await;
    let error = result.unwrap_err();
    assert!(error.contains("Investigator (ollama) failed"), "{error}");
    assert!(error.contains("Revisão incompleta"), "{error}");
    let requests = task.await.unwrap();
    assert_eq!(requests.len(), 2, "must not dispatch Architect or Reviewer");
    assert!(requests[0]["tools"].as_array().unwrap().iter().any(
        |tool| tool["function"]["name"] == "search_index"));
    assert!(requests[0]["messages"].to_string().contains("STRUCTURAL INDEX"));
    assert!(requests[1]["messages"].to_string().contains("Call native read_file tools now"));
    assert_eq!(evidence.source_read_count(), 0);
}

#[cfg(unix)]
#[test]
fn symphony_checkpoint_refuses_symlink_destination() {
    let workspace = Workspace::new();
    std::fs::create_dir(workspace.0.join(".dude")).unwrap();
    let protected = workspace.0.join("untouched");
    std::fs::write(&protected, "keep").unwrap();
    std::os::unix::fs::symlink(&protected, workspace.0.join(".dude/symphony_checkpoint.json")).unwrap();
    assert!(symphony_checkpoint::save(&workspace.0, "question", "Architect", "ollama", "model", "answer", "partial").is_err());
    assert_eq!(std::fs::read_to_string(protected).unwrap(), "keep");
}
