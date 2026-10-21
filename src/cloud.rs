use std::{path::PathBuf, time::Duration};
use serde_json::{json, Value};
use crate::{file_tools, credentials, evidence::EvidenceCache, Message};

async fn tool(workspace: PathBuf, name: String, args: Value, evidence: EvidenceCache) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        evidence.execute(&workspace, &name, &args)
    }).await.map_err(|e| e.to_string())
}


const GROQ_SLICE_CHARS: usize = 6500;

// Preserve every character of the conversation, including tool output, in ordered slices.
// Each slice gets its own bounded Groq request; the original question is never clipped.
fn groq_slices(convo: &[Value]) -> Vec<String> {
    let mut source = String::new();
    for entry in convo {
        let role = entry["role"].as_str().unwrap_or("unknown");
        source.push_str("\n--- ");
        source.push_str(role);
        source.push_str(" ---\n");
        if let Some(content) = entry["content"].as_str() {
            source.push_str(content);
        }
        if let Some(calls) = entry.get("tool_calls") {
            source.push_str("\nTool calls: ");
            source.push_str(&calls.to_string());
        }
        source.push('\n');
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        current.push(ch);
        if current.len() >= GROQ_SLICE_CHARS {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() { chunks.push(current); }
    chunks
}

async fn groq_request_with_quota(
    client: &reqwest::Client, key: &str, model: &str, messages: &[Value],
    allow_tools: bool,
) -> Result<Value, String> {
    let mut retries = 0;
    loop {
        let mut body = json!({"model":model,"messages":messages,"stream":false});
        if allow_tools {
            body["tools"] = json!(file_tools::definitions());
            body["tool_choice"] = json!("auto");
        } else {
            body["tool_choice"] = json!("none");
        }
        let response = client.post("https://api.groq.com/openai/v1/chat/completions")
            .bearer_auth(key).json(&body).send().await
            .map_err(|e|format!("Groq connection: {e}"))?;
        let status = response.status();
        if status.is_success() {
            return response.json().await.map_err(|e|format!("Invalid Groq response: {e}"));
        }
        let retry_header = response.headers().get("retry-after")
            .and_then(|h|h.to_str().ok()).and_then(|h|h.parse::<f64>().ok());
        let body = response.text().await.unwrap_or_default();
        // HTTP 413 with "Requested > Limit" is not solved by waiting:
        // the next call must be sliced. HTTP 429 may clear with time.
        // Groq sometimes reports a depleted TPM window as HTTP 413.
        // Only wait when the request itself fits the limit.
        let fits_tpm = match (groq_error_number(&body, "Limit "), groq_error_number(&body, "Requested ")) {
            (Some(limit), Some(requested)) => requested <= limit,
            _ => false,
        };
        if (status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || (status == reqwest::StatusCode::PAYLOAD_TOO_LARGE && fits_tpm))
            && retries < 5 {
            let wait = parse_retry_seconds(&body).or(retry_header).unwrap_or(60.0)
                .ceil().clamp(1.0, 75.0) as u64;
            retries += 1;
            tokio::time::sleep(Duration::from_secs(wait)).await;
            continue;
        }
        if !allow_tools && status == reqwest::StatusCode::BAD_REQUEST
            && body.contains("tool_use_failed") {
            // GPT-OSS occasionally attempts a tool call while summarizing read_file
            // transcripts. Re-ask for text; preserve the original evidence slice.
            let mut retry_messages = messages.to_vec();
            retry_messages.push(json!({"role":"user","content":
                "Return only a textual analysis of the supplied evidence. Do not invoke tools or output function calls."}));
            let retry = client.post("https://api.groq.com/openai/v1/chat/completions")
                .bearer_auth(key)
                .json(&json!({"model":model,"messages":retry_messages,"stream":false,"tool_choice":"none"}))
                .send().await.map_err(|e|format!("Groq text-only retry: {e}"))?;
            let retry_status = retry.status();
            if retry_status.is_success() {
                return retry.json().await.map_err(|e|e.to_string());
            }
            return Err(format!("Groq text-only retry HTTP {retry_status}: {}",
                retry.text().await.unwrap_or_default().chars().take(600).collect::<String>()));
        }
        return Err(format!("Groq HTTP {status}: {}", body.chars().take(600).collect::<String>()));
    }
}

// When a tool loop exceeds a single request's capacity, walk ALL previous
// context in slices, collect findings, then resume the investigation with tools.
// The chunks are processed sequentially, respecting TPM pressure.
async fn groq_resume_sliced(
    client: &reqwest::Client, key: &str, model: &str,
    convo: &[Value],
) -> Result<Vec<Value>, String> {
    let chunks = groq_slices(convo);
    let mut findings = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let context = vec![
            json!({"role":"system","content":"You are DUDE's context reader. Extract relevant technical facts, file paths, uncertainties and results from this ordered slice. Do not invent facts. Preserve important identifiers. This is one piece of a larger investigation."}),
            json!({"role":"user","content":format!("Evidence slice {} of {}:\n{}",i+1,chunks.len(),chunk)}),
        ];
        let value = groq_request_with_quota(client,key,model,&context,false).await?;
        let extracted = value["choices"][0]["message"]["content"].as_str()
            .ok_or("Groq returned no findings for evidence slice")?;
        findings.push_str(&format!("\nSlice {} findings:\n{}\n",i+1,extracted));
    }
    // Distill the accumulated findings hierarchically if a huge workspace
    // would otherwise reconstruct another over-limit request.
    while findings.len() > GROQ_SLICE_CHARS {
        let mut next = String::new();
        let chunks = groq_slices(&[json!({"role":"user","content":findings})]);
        for (i, chunk) in chunks.iter().enumerate() {
            let context = vec![
                json!({"role":"system","content":"Consolidate these engineering findings faithfully into concise, evidence-grounded notes. Retain important identifiers, paths, caveats and the user's goal. Do not invent details."}),
                json!({"role":"user","content":format!("Findings batch {} of {}:\n{}",i+1,chunks.len(),chunk)}),
            ];
            let value = groq_request_with_quota(client,key,model,&context,false).await?;
            let part = value["choices"][0]["message"]["content"].as_str()
                .ok_or("Groq returned no consolidated findings")?;
            next.push_str(part);
            next.push('\n');
        }
        // Guarantee termination even when the model fails to compress.
        if next.len() >= findings.len() {
            return Err("Groq could not consolidate sliced evidence within its context budget; evidence preserved in the current investigation".into());
        }
        findings = next;
    }
    let original = convo.iter().rev().find(|entry| entry["role"] == "user"
        && entry.get("tool_call_id").is_none())
        .and_then(|entry|entry["content"].as_str()).unwrap_or("");
    Ok(vec![
        json!({"role":"system","content":"You are DUDE, an engineering investigator. The following notes were gathered by examining ALL previous context slices, including file-tool results. Continue the original investigation, use file tools only when additional evidence is needed, and distinguish verified facts from hypotheses."}),
        json!({"role":"user","content":format!("Original request:\n{}\n\nGathered technical findings:\n{}\n\nContinue investigating and respond to the request.",original,findings)}),
    ])
}

pub async fn groq(client: &reqwest::Client, model: &str, messages: Vec<Message>, workspace: PathBuf, allow_tools: bool, evidence: EvidenceCache) -> Result<String, String> {
    let key = credentials::get("groq").ok_or("Missing Groq API key. Use /apikey groq")?;
    let mut convo: Vec<Value> = messages.into_iter().map(|m| json!({"role":m.role,"content":m.content})).collect();
    for _ in 0..(if allow_tools { 8 } else { 1 }) {
        // A 413 can mean the accumulated tool conversation no longer fits.
        // Recover by reading all evidence in sequential slices, not truncating it.
        let value = match groq_request_with_quota(client,&key,model,&convo,allow_tools).await {
            Ok(value) => value,
            Err(error) if error.contains("413") || error.contains("Payload Too Large") => {
                convo = groq_resume_sliced(client,&key,model,&convo).await?;
                groq_request_with_quota(client,&key,model,&convo,allow_tools).await?
            }
            Err(error) => return Err(error),
        };
        let msg = &value["choices"][0]["message"];
        let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {return msg["content"].as_str().map(str::to_owned).ok_or_else(||"Groq returned no message".into());}
        if !allow_tools { return Err("Unexpected Groq tool call in review-only stage".into()); }
        convo.push(msg.clone());
        for call in calls {
            let name = call["function"]["name"].as_str().ok_or("Groq tool missing name")?.to_owned();
            let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
            let args: Value = serde_json::from_str(args_str).map_err(|e|format!("Groq tool arguments: {e}"))?;
            let output = tool(workspace.clone(), name, args, evidence.clone()).await?;
            convo.push(json!({"role":"tool","tool_call_id":call["id"],"content":output}));
        }
    }
    Err("Groq exceeded eight tool rounds".into())
}

pub async fn gemini(client: &reqwest::Client, model: &str, messages: Vec<Message>, workspace: PathBuf, allow_tools: bool, evidence: EvidenceCache) -> Result<String, String> {
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
    for _ in 0..(if allow_tools { 8 } else { 1 }) {
        let response = client.post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
            .header("x-goog-api-key",&key)
            .json(&if allow_tools { json!({"systemInstruction":{"parts":[{"text":context}]},"contents":contents,"tools":[{"functionDeclarations":defs}]}) } else { json!({"systemInstruction":{"parts":[{"text":context}]},"contents":contents}) })
            .send().await.map_err(|e|format!("Gemini connection: {e}"))?;
        let status=response.status();
        if !status.is_success() {return Err(format!("Gemini HTTP {status}: {}",response.text().await.unwrap_or_default().chars().take(600).collect::<String>()));}
        let value:Value=response.json().await.map_err(|e|e.to_string())?;
        let candidate=&value["candidates"][0]["content"];
        let parts=candidate["parts"].as_array().ok_or_else(|| format!("Gemini no content: {}",value.to_string().chars().take(250).collect::<String>()))?;
        let calls:Vec<Value>=parts.iter().filter_map(|p|p.get("functionCall").cloned()).collect();
        if calls.is_empty() {
            let text=parts.iter().filter_map(|p|p["text"].as_str()).collect::<Vec<_>>().join("\n");
            return if text.is_empty(){Err("Gemini response has no text".into())}else{Ok(text)};
        }
        if !allow_tools { return Err("Unexpected Gemini tool call in review-only stage".into()); }
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
    Err("Gemini exceeded eight tool rounds".into())
}


fn groq_error_number(body: &str, marker: &str) -> Option<u64> {
    let value = body.split(marker).nth(1)?;
    value.chars().take_while(|c| c.is_ascii_digit())
        .collect::<String>().parse::<u64>().ok()
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
    fn slices_keep_all_content_in_order() {
        let long = "abcdef 🦀".repeat(3000);
        let convo = vec![json!({"role":"user","content":long})];
        let slices = groq_slices(&convo);
        assert!(slices.len() > 1);
        assert!(slices.iter().all(|s| s.len() <= GROQ_SLICE_CHARS + 4));
        assert_eq!(slices.concat(), format!("\n--- user ---\n{}\n", long));
    }
    #[test]
    fn reads_tpm_budget() {
        let body = "Limit 8000, Requested 9821, please reduce your message size";
        assert_eq!(groq_error_number(body,"Limit "),Some(8000));
        assert_eq!(groq_error_number(body,"Requested "),Some(9821));
    }
    #[test]
    fn detects_retry_seconds() {
        assert_eq!(parse_retry_seconds("Please try again in 12.1425s."), Some(12.1425));
        assert_eq!(parse_retry_seconds("unrelated"), None);
    }
}
