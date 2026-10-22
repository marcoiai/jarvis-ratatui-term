//! Small-model intent interpretation and conservative prompt enrichment.
use serde_json::{json, Value};
use crate::router::Route;
use std::time::Duration;

pub struct Interpretation {
    pub route: Route,
    pub enriched: Option<String>,
}

fn parse(body: &str, mode: &str, fallback: Route, original: &str) -> Interpretation {
    let Some(obj) = serde_json::from_str::<Value>(body).ok() else {return Interpretation{route:fallback,enriched:None}};
    let label = obj["route"].as_str().unwrap_or("").to_ascii_uppercase();
    let route = match label.as_str() {
        "LOCAL" if matches!(&fallback, Route::Local(_)) => fallback,
        "SOLO" => Route::Solo,
        "SYMPHONY" if mode == "symphony" => Route::Symphony,
        _ => fallback,
    };
    let rewritten = obj["improved_prompt"].as_str().unwrap_or("").trim();
    // Never erase the original request: a short bounded enhancement may only
    // supplement it. Don't insert instructions about tools, permissions or actions.
    let safe = !rewritten.is_empty()
        && rewritten.chars().count() <= 1200
        && !rewritten.contains("IGNORE")
        && !rewritten.contains("ignore previous")
        && !rewritten.contains("sudo ")
        && !rewritten.contains("/cloud allow");
    let enriched = if safe && !matches!(&route, Route::Local(_)) {
        Some(format!("{original}\n\nContexto interpretado (hipótese auxiliar; prevalece o pedido original): {rewritten}"))
    } else { None };
    Interpretation { route, enriched }
}

pub async fn interpret(client: &reqwest::Client, host: &str, model: &str, input: &str, mode: &str, fallback: Route) -> Interpretation {
    let response = client.post(format!("{host}/api/chat")).timeout(Duration::from_secs(15))
        .json(&json!({"model":model,"stream":false,"think":false,"messages":[
            {"role":"system","content":"Você é o Interpreter do DUDE. Analise a intenção e reescreva de forma mais clara e útil sem adicionar requisitos, fatos ou permissões não solicitados. Responda apenas JSON no formato {\"route\":\"LOCAL|SOLO|SYMPHONY\",\"improved_prompt\":\"...\"}. LOCAL apenas para cumprimentos simples; SOLO para perguntas normais; SYMPHONY para auditorias e análises complexas. Preserve nomes de arquivos, restrições e contexto. Não execute nenhuma ação."},
            {"role":"user","content":input.chars().take(1400).collect::<String>()}
        ],"format":"json","options":{"num_predict":400,"temperature":0}}));
    let Ok(response) = response.send().await else {return Interpretation {route:fallback,enriched:None}};
    if !response.status().is_success() {return Interpretation {route:fallback,enriched:None}};
    let Ok(body) = response.json::<Value>().await else {return Interpretation {route:fallback,enriched:None}};
    parse(body["message"]["content"].as_str().unwrap_or(""),mode,fallback,input)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_original_and_adds_context() {
        let x=parse(r#"{"route":"SOLO","improved_prompt":"Explique os componentes do projeto e suas responsabilidades."}"#,"solo",Route::Solo,"analisa projeto");
        assert!(x.enriched.unwrap().starts_with("analisa projeto\n"));
    }
    #[test]
    fn malformed_response_falls_back() {
        let x=parse("invalid","solo",Route::Solo,"pedido");
        assert!(matches!(x.route,Route::Solo));
        assert!(x.enriched.is_none());
    }
}
