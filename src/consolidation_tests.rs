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
