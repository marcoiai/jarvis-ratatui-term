//! M1.2: deterministic validation before showing generated text.
//! An observed line is NOT equivalent to semantic proof.
use crate::{evidence::EvidenceCache, evidence_ledger, metrics::Metrics};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub async fn validate(client: &reqwest::Client, host: &str, model: &str,
    question: &str, original: String, evidence: &EvidenceCache, metrics: &Metrics) -> String {
    let first = evidence_ledger::audit(&original, evidence);
    if first.unchecked_references.is_empty() && !first.coverage_conflict && first.falsely_unread.is_empty() {
        return original;
    }
    // A single, bounded revision without tool calls and without executing actions.
    // Errors fail closed as an explicitly unverified answer.
    let manifest = evidence.export();
    let prompt = format!(
        "PERGUNTA ORIGINAL DO USUÁRIO:\n{}\n\nTAREFA: Responda diretamente à PERGUNTA ORIGINAL, corrigindo o texto preliminar. NÃO escreva um parecer sobre o texto, NÃO diga 'revisão da análise', 'afirmação no relatório', nem liste correções; produza uma resposta final útil ao usuário. Preserve informações sustentadas; marque hipóteses; não invente comandos, módulos ou referências. Não liste arquivos como não lidos se estiverem no manifesto abaixo. Não alegue verificação semântica completa.\n\nMANIFESTO DE LEITURAS E EVIDÊNCIAS:\n{}\n\nRESPOSTA PRELIMINAR A CORRIGIR:\n{}",
        question.chars().take(2000).collect::<String>(),
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
        if body["done_reason"].as_str() == Some("length") { return None; }
        let text = body["message"]["content"].as_str()?.trim().to_owned();
        if text.is_empty() || is_meta_review(&text) { None } else { Some(text) }
    }.await;
    metrics.stage("validation_gate","ollama",started,revised.is_some());
    if let Some(text) = revised {
        let post = evidence_ledger::audit(&text,evidence);
        if post.unchecked_references.is_empty() && !post.coverage_conflict && post.falsely_unread.is_empty() {
            return format!("⚠️ Revisão automática por inconsistência de evidências; suporte semântico não verificado.\n\n{text}");
        }
    }
    format!("⚠️ PARECER PRELIMINAR NÃO VALIDADO: a revisão não preservou a tarefa, foi truncada ou manteve inconsistências. Não use este resultado como análise verificada.\n{}\n\n{}",
        evidence_ledger::notice(&original,evidence), original)
}

fn is_meta_review(text: &str) -> bool {
    let heading: String = text.chars().take(650).collect::<String>().to_lowercase();
    ["revisão conservadora da análise", "revisão técnica do relatório",
     "correção de afirmações sem suporte", "afirmação no relatório",
     "revisão do texto fornecido", "auditoria do relatório anterior"]
        .iter().any(|marker|heading.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_check_is_independent_of_llm() {
        let cache = EvidenceCache::default();
        assert_eq!(evidence_ledger::audit("src/none.rs:42", &cache).unchecked_references.len(), 1);
        assert!(is_meta_review("Abaixo segue a revisão conservadora da análise fornecida."));
        assert!(!is_meta_review("O DUDE é um projeto Rust com interface Ratatui."));
    }
}
