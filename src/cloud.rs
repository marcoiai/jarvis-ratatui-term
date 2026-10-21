use std::{path::PathBuf, time::Duration};
use serde_json::{json, Value};
use crate::{file_tools, credentials, evidence::EvidenceCache, metrics::Metrics, Message};

async fn tool(workspace: PathBuf, name: String, args: Value, evidence: EvidenceCache) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        evidence.execute(&workspace, &name, &args)
    }).await.map_err(|e| e.to_string())
}

// Bound Groq prompt size independently of the local evidence cache.
// Preserve message roles and tool_call IDs; abbreviate bulky tool outputs.
fn groq_compact_messages(messages: &[Value]) -> Vec<Value> {
    const MAX_TOOL_CHARS: usize = 850;
    const MAX_OTHER_CHARS: usize = 4200;
    messages.iter().map(|original| {
        let mut msg = original.clone();
        let role = msg["role"].as_str().unwrap_or("");
        let limit = if role == "tool" { MAX_TOOL_CHARS } else { MAX_OTHER_CHARS };
        if let Some(content) = msg["content"].as_str() {
            if content.chars().count() > limit {
                let shortened: String = content.chars().take(limit).collect();
                msg["content"] = json!(format!("{shortened}\n[truncated for Groq token budget; originals retained in local evidence]"));
            }
        }
        msg
    }).collect()
}

pub async fn groq(client: &reqwest::Client, model: &str, messages: Vec<Message>, workspace: PathBuf, allow_tools: bool, evidence: EvidenceCache, metrics: Metrics) -> Result<String, String> {
    let key = credentials::get("groq").ok_or("Missing Groq API key. Use /apikey groq")?;
    let mut convo: Vec<Value> = messages.into_iter().map(|m| json!({"role":m.role,"content":m.content})).collect();
    let rounds = if allow_tools { crate::investigation::tool_rounds() } else { 0 };
    for round in 0..=rounds {
        let tools_enabled = allow_tools && round < rounds;
        if allow_tools && !tools_enabled {
            convo.push(json!({"role":"user","content":crate::investigation::FINALIZE}));
        }
        // Rate limits are shared across all Groq calls, including other Symphony roles.
        // Retry the exact same turn; never re-execute completed file tools.
        let mut retries = 0;
        let outgoing = groq_compact_messages(&convo);
        let response = loop {
            let response = client.post("https://api.groq.com/openai/v1/chat/completions")
                .bearer_auth(&key)
                .json(&if tools_enabled { json!({"model":model,"messages":outgoing,"stream":false,"tools":file_tools::definitions(),"tool_choice":"auto"}) } else { json!({"model":model,"messages":convo,"stream":false}) })
                .send().await.map_err(|e|format!("Groq connection: {e}"))?;
            if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {break response;}
            if retries >= 3 {break response;}
            let retry_header = response.headers().get("retry-after")
                .and_then(|h|h.to_str().ok())
                .and_then(|h|h.parse::<f64>().ok());
            let body = response.text().await.unwrap_or_default();
            let suggested = parse_retry_seconds(&body).or(retry_header).unwrap_or(15.0);
            let wait = suggested.ceil().clamp(1.0, 75.0) as u64;
            retries += 1;
            tokio::time::sleep(Duration::from_secs(wait)).await;
        };
        let status = response.status();
        if !status.is_success() {return Err(format!("Groq HTTP {status}: {}", response.text().await.unwrap_or_default().chars().take(600).collect::<String>()));}
        let value: Value = response.json().await.map_err(|e|e.to_string())?;
        if let (Some(i),Some(o))=(value["usage"]["prompt_tokens"].as_u64(),value["usage"]["completion_tokens"].as_u64()){ metrics.tokens("groq",model,i,o); }
        let msg = &value["choices"][0]["message"];
        let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {return msg["content"].as_str().map(str::to_owned).ok_or_else(||"Groq returned no message".into());}
        if !tools_enabled { return Err("Unexpected Groq tool call during final synthesis".into()); }
        convo.push(msg.clone());
        for call in calls {
            let name = call["function"]["name"].as_str().ok_or("Groq tool missing name")?.to_owned();
            let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
            let args: Value = serde_json::from_str(args_str).map_err(|e|format!("Groq tool arguments: {e}"))?;
            let output = tool(workspace.clone(), name, args, evidence.clone()).await?;
            convo.push(json!({"role":"tool","tool_call_id":call["id"],"content":output}));
        }
    }
    Err("Groq ended without a final response".into())
}

pub async fn gemini(client: &reqwest::Client, model: &str, messages: Vec<Message>, workspace: PathBuf, allow_tools: bool, evidence: EvidenceCache, metrics: Metrics) -> Result<String, String> {
    let key = credentials::get("gemini").ok_or("Missing Gemini API key. Use /apikey gemini")?;
    let mut context = String::new();
    let mut contents = Vec::<Value>::new();
    for message in messages {
        match message.role.as_str() {
            "system" => { context.push_str(&message.content); context.push('\n'); },
            "assistant" => contents.push(json!({"role":"model","parts":[{"text":message.content}]})),
            _ => contents.push(json!({"role":"user","parts":[{"text":message.content}]})),
        }
    }
    let defs:Vec<Value> = file_tools::definitions().iter().map(|t| {
        let f=&t["function"];
        json!({"name":f["name"],"description":f["description"],"parameters":f["parameters"]})
    }).collect();
    let rounds = if allow_tools { crate::investigation::tool_rounds() } else { 0 };
    for round in 0..=rounds {
        let tools_enabled = allow_tools && round < rounds;
        if allow_tools && !tools_enabled {
            contents.push(json!({"role":"user","parts":[{"text":crate::investigation::FINALIZE}]}));
        }
        let response = client.post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
            .header("x-goog-api-key",&key)
            .json(&if tools_enabled { json!({"systemInstruction":{"parts":[{"text":context}]},"contents":contents,"tools":[{"functionDeclarations":defs}]}) } else { json!({"systemInstruction":{"parts":[{"text":context}]},"contents":contents}) })
            .send().await.map_err(|e|format!("Gemini connection: {e}"))?;
        let status=response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(format!(
                    "Gemini model '{model}' is unavailable to this API key (404). Set GEMINI_MODEL=gemini-3.8-flash and restart DUDE. Details: {}",
                    body.chars().take(300).collect::<String>()
                ));
            }
            return Err(format!("Gemini HTTP {status}: {}",body.chars().take(600).collect::<String>()));
        }
        let value:Value=response.json().await.map_err(|e|e.to_string())?;
        if let (Some(i),Some(o))=(value["usageMetadata"]["promptTokenCount"].as_u64(),value["usageMetadata"]["candidatesTokenCount"].as_u64()){ metrics.tokens("gemini",model,i,o); }
        let candidate=&value["candidates"][0]["content"];
        let parts=candidate["parts"].as_array().ok_or_else(|| format!("Gemini no content: {}",value.to_string().chars().take(250).collect::<String>()))?;
        let calls:Vec<Value>=parts.iter().filter_map(|p|p.get("functionCall").cloned()).collect();
        if calls.is_empty() {
            let text=parts.iter().filter_map(|p|p["text"].as_str()).collect::<Vec<_>>().join("\n");
            return if text.is_empty(){Err("Gemini response has no text".into())}else{Ok(text)};
        }
        if !tools_enabled { return Err("Unexpected Gemini tool call during final synthesis".into()); }
        contents.push(candidate.clone());
        let mut responses=Vec::new();
        for call in calls {
            let name=call["name"].as_str().ok_or("Gemini tool missing name")?.to_owned();
            let args=call.get("args").cloned().unwrap_or(json!({}));
            let output=tool(workspace.clone(),name.clone(),args, evidence.clone()).await?;
            let mut reply=json!({"name":name,"response":{"output":output}});
            if let Some(id)=call.get("id"){reply["id"]=id.clone();}
            responses.push(json!({"functionResponse":reply}));
        }
        contents.push(json!({"role":"user","parts":responses}));
    }
    Err("Gemini ended without a final response".into())
}


fn parse_retry_seconds(body: &str) -> Option<f64> {
    let start = body.find("Please try again in ")? + "Please try again in ".len();
    let suffix = &body[start..];
    let end = suffix.find('s')?;
    suffix[..end].trim().parse::<f64>().ok().filter(|v|v.is_finite() && *v > 0.0)
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn groq_compaction_retains_tool_identity() {
        let messages = vec![json!({"role":"tool","tool_call_id":"call-1","content":"x".repeat(10000)})];
        let compact = groq_compact_messages(&messages);
        assert_eq!(compact[0]["tool_call_id"], "call-1");
        assert!(compact[0]["content"].as_str().unwrap().len() < 1100);
        assert_eq!(messages[0]["content"].as_str().unwrap().len(), 10000);
    }
    #[test]
    fn detects_retry_seconds() {
        assert_eq!(parse_retry_seconds("Please try again in 12.1425s."), Some(12.1425));
        assert_eq!(parse_retry_seconds("unrelated"), None);
    }
}
