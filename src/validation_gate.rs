//! M1.2: deterministic validation before showing generated text.
//! An observed line is NOT equivalent to semantic proof.
use crate::{evidence::EvidenceCache, evidence_ledger, metrics::Metrics};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub async fn validate(client: &reqwest::Client, host: &str, model: &str,
    original: String, evidence: &EvidenceCache, metrics: &Metrics) -> String {
    let first = evidence_ledger::audit(&original, evidence);
    if first.unchecked_references.is_empty() && !first.coverage_conflict {
        return original;
    }
    // A single, bounded revision without tool calls and without executing actions.
    // Errors fail closed as an explicitly unverified answer.
    let manifest = evidence.export();
    let prompt = format!(
        "Corrija SOMENTE afirmações sem suporte e contradições da análise abaixo. Preserve fatos que tenham suporte, identifique hipóteses, não invente comandos nem arquivos e não afirme que o workspace está vazio se houve leituras. Não invente referências. Diga explicitamente quais arquivos NÃO foram lidos. Esta revisão não é verificação semântica.\n\nEvidências:\n{}\n\nTexto a revisar:\n{}",
        manifest.chars().take(22000).collect::<String>(),
        original.chars().take(14000).collect::<String>()
    );
    let started = Instant::now();
    let revised = async {
        let response = client.post(format!("{host}/api/chat"))
            .timeout(Duration::from_secs(60))
            .json(&json!({"model":model,"messages":[
                {"role":"system","content":"Você é revisor conservador de evidências. Responda apenas texto em português. Não utilize ferramentas. Não trate existência de arquivo como prova do seu comportamento."},
                {"role":"user","content":prompt}
            ],"stream":false,"think":false,"options":{"num_predict":1500}}))
            .send().await.ok()?;
        if !response.status().is_success() { return None; }
        let body: Value = response.json().await.ok()?;
        let text = body["message"]["content"].as_str()?.trim().to_owned();
        if text.is_empty() { None } else { Some(text) }
    }.await;
    metrics.stage("validation_gate","ollama",started,revised.is_some());
    if let Some(text) = revised {
        let post = evidence_ledger::audit(&text,evidence);
        if post.unchecked_references.is_empty() && !post.coverage_conflict {
            return format!("⚠️ Revisão automática por inconsistência de evidências; suporte semântico não verificado.\n\n{text}");
        }
    }
    format!("⚠️ PARECER NÃO VALIDADO: foram detectadas inconsistências e a revisão automática não as resolveu. Não use este resultado como análise verificada.\n{}\n\n{}",
        evidence_ledger::notice(&original,evidence), original)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_check_is_independent_of_llm() {
        let cache = EvidenceCache::default();
        assert_eq!(evidence_ledger::audit("src/none.rs:42", &cache).unchecked_references.len(), 1);
    }
}
