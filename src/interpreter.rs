//! Optional small-model intent interpreter. Explicit CLI commands are never sent to an LLM.
use serde_json::{json, Value};
use crate::router::Route;
use std::time::Duration;

pub async fn classify(client: &reqwest::Client, host: &str, model: &str, input: &str, mode: &str, fallback: Route) -> Route {
    let prompt = format!("Classifique a intenção da mensagem em uma ÚNICA palavra: LOCAL, SOLO ou SYMPHONY. LOCAL somente para cumprimentos simples, sem perguntas técnicas. SYMPHONY para revisão profunda, auditoria, arquitetura ou planejamento complexo. SOLO para perguntas comuns. Mensagem: {}", input.chars().take(1400).collect::<String>());
    let request = client.post(format!("{host}/api/chat")).timeout(Duration::from_secs(15))
        .json(&json!({"model":model,"stream":false,"think":false,"messages":[
            {"role":"system","content":"Você é o roteador do DUDE. Responda exclusivamente LOCAL, SOLO ou SYMPHONY, sem explicações."},
            {"role":"user","content":prompt}
        ],"options":{"num_predict":12,"temperature":0}}));
    let Ok(response) = request.send().await else { return fallback };
    if !response.status().is_success() { return fallback; }
    let Ok(body) = response.json::<Value>().await else { return fallback };
    let raw = body["message"]["content"].as_str().unwrap_or("").trim().to_uppercase();
    match raw.as_str() {
        "LOCAL" => Route::Local("E aí, buddy! O que vamos construir hoje? 🦀".into()),
        "SOLO" => Route::Solo,
        "SYMPHONY" if mode=="symphony" => Route::Symphony,
        "SYMPHONY" => Route::Solo,
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_types_are_available() {
        assert!(matches!(Route::Solo, Route::Solo));
    }
}
