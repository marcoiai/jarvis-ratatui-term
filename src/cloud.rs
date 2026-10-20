use std::{env, path::PathBuf};
use serde_json::{json, Value};
use crate::{file_tools, Message};

async fn tool(workspace: PathBuf, name: String, args: Value) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        file_tools::execute(&workspace, &name, &args)
            .unwrap_or_else(|e| format!("Tool error: {e}"))
    }).await.map_err(|e| e.to_string())
}

pub async fn groq(client: &reqwest::Client, model: &str, messages: Vec<Message>, workspace: PathBuf) -> Result<String, String> {
    let key = env::var("GROQ_API_KEY").map_err(|_| "Missing GROQ_API_KEY".to_string())?;
    let mut convo: Vec<Value> = messages.into_iter().map(|m| json!({"role":m.role,"content":m.content})).collect();
    for _ in 0..8 {
        let response = client.post("https://api.groq.com/openai/v1/chat/completions")
            .bearer_auth(&key)
            .json(&json!({"model":model,"messages":convo,"stream":false,"tools":file_tools::definitions(),"tool_choice":"auto"}))
            .send().await.map_err(|e|format!("Groq connection: {e}"))?;
        let status = response.status();
        if !status.is_success() {return Err(format!("Groq HTTP {status}: {}", response.text().await.unwrap_or_default().chars().take(600).collect::<String>()));}
        let value: Value = response.json().await.map_err(|e|e.to_string())?;
        let msg = &value["choices"][0]["message"];
        let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
        if calls.is_empty() {return msg["content"].as_str().map(str::to_owned).ok_or_else(||"Groq returned no message".into());}
        convo.push(msg.clone());
        for call in calls {
            let name = call["function"]["name"].as_str().ok_or("Groq tool missing name")?.to_owned();
            let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
            let args: Value = serde_json::from_str(args_str).map_err(|e|format!("Groq tool arguments: {e}"))?;
            let output = tool(workspace.clone(), name, args).await?;
            convo.push(json!({"role":"tool","tool_call_id":call["id"],"content":output}));
        }
    }
    Err("Groq exceeded eight tool rounds".into())
}

pub async fn gemini(client: &reqwest::Client, model: &str, messages: Vec<Message>, workspace: PathBuf) -> Result<String, String> {
    let key = env::var("GEMINI_API_KEY").or_else(|_|env::var("GOOGLE_API_KEY"))
        .map_err(|_|"Set GEMINI_API_KEY (or GOOGLE_API_KEY) for Gemini API access".to_owned())?;
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
    for _ in 0..8 {
        let response = client.post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"))
            .header("x-goog-api-key",&key)
            .json(&json!({"systemInstruction":{"parts":[{"text":context}]},"contents":contents,
                "tools":[{"functionDeclarations":defs}]}))
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
        contents.push(candidate.clone());
        let mut responses=Vec::new();
        for call in calls {
            let name=call["name"].as_str().ok_or("Gemini tool missing name")?.to_owned();
            let args=call.get("args").cloned().unwrap_or(json!({}));
            let output=tool(workspace.clone(),name.clone(),args).await?;
            let mut reply=json!({"name":name,"response":{"output":output}});
            if let Some(id)=call.get("id"){reply["id"]=id.clone();}
            responses.push(json!({"functionResponse":reply}));
        }
        contents.push(json!({"role":"user","parts":responses}));
    }
    Err("Gemini exceeded eight tool rounds".into())
}
