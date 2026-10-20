//! Deterministic, low-cost request router; no model call for local help/greetings.
use crate::roles::Roles;

#[derive(Debug, PartialEq, Eq)]
pub enum Route { Local(String), Solo, Symphony }

impl Route {
    pub fn needs_ollama(&self, roles:&Roles)->bool {
        match self {
            Self::Local(_)=>false,
            Self::Solo=>roles.selected_provider()=="ollama",
            Self::Symphony=>[&roles.investigator,&roles.architect,&roles.reviewer].iter().any(|p|p.as_str()=="ollama"),
        }
    }
}
pub fn route(input:&str, mode:&str, language:&str)->Route {
    let query=input.trim();
    let lower=query.to_lowercase();
    if matches!(lower.as_str(), "?" | "help" | "/help") {
        if language == "pt-BR" { return Route::Local("Comandos do DUDE: /help, /roles, /mode solo|symphony, /solo ROLE, /role ROLE PROVIDER, /workspace PATH, /memory, /knowledge, /cloud allow|deny, /clear, /provider PROVIDER, /model NAME, /apikey, /apikey gemini|groq, /apikey clear PROVIDER, /env, /align, /metrics.\nOllama: OLLAMA_NUM_CTX (padrão 16384). Investigação: DUDE_TOOL_ROUNDS (padrão 8, máximo 32).\nFaça uma pergunta para começar. Para usar todos os agentes, digite: symphony: SUA PERGUNTA.".into()); }
        return Route::Local("DUDE commands: /help, /roles, /mode solo|symphony, /solo ROLE, /role ROLE PROVIDER, /workspace PATH, /memory, /knowledge, /cloud allow|deny, /clear, /provider PROVIDER, /model NAME, /apikey, /apikey gemini|groq (masked entry), /apikey clear PROVIDER, /env, /align, /metrics.\nOllama: OLLAMA_NUM_CTX (default 16384). Investigation: DUDE_TOOL_ROUNDS (default 8, max 32).\nAsk a specific question to begin. Force full orchestration with 'symphony: YOUR QUESTION'.".into());
    }
    if matches!(lower.as_str(),"oi"|"olá"|"ola"|"hey"|"hi"|"hello"|"bom dia"|"boa tarde"|"boa noite"|"eae"|"e aí") {
        return Route::Local(if language == "pt-BR" { "E aí, buddy! O que vamos construir hoje? Use /help para ver os comandos. 🦀" } else { "Hey dude! Ask me something about your project. Use /help for commands. 🦀" }.into());
    }
    if lower.starts_with("symphony:") || lower.starts_with("sinfônica:") {
        return Route::Symphony;
    }
    if lower.starts_with("solo:") { return Route::Solo; }
    if mode=="solo" { return Route::Solo; }
    // In Symphony mode, only explicit engineering-depth requests trigger all roles.
    // Conservative default: short or straightforward questions use the selected Solo provider.
    let markers=["arquitetura","architecture","analise","análise","analisar","audit","auditoria",
        "refator","redesign","planej","estratég","strategy","trade-off","riscos",
        "investig","proponha melhorias","review","revis","performance",
        "migração","migration","segurança","security","implementar","implement",
        "design","investigate","optimiz"];
    if query.chars().count()>=55 && markers.iter().any(|word|lower.contains(word)) {
        Route::Symphony
    } else {
        Route::Solo
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn local_responses_follow_language() {
        assert!(matches!(route("oi","solo","pt-BR"),Route::Local(s) if s.contains("E aí")));
        assert!(matches!(route("/help","solo","pt-BR"),Route::Local(s) if s.contains("Comandos do DUDE")));
        assert!(matches!(route("oi","solo","en"),Route::Local(s) if s.contains("Hey dude")));
    }
    #[test] fn trivial_is_local() {
        for input in ["?","/help","oi","Bom dia"] {assert!(matches!(route(input,"symphony","pt-BR"),Route::Local(_))); }
    }
    #[test] fn simple_in_symphony_uses_solo() {
        assert_eq!(route("O que é Tokio?","symphony","pt-BR"),Route::Solo);
        assert_eq!(route("Como faço para listar arquivos?","symphony","pt-BR"),Route::Solo);
    }
    #[test] fn complex_in_symphony_uses_all_roles() {
        assert_eq!(route("Explique a arquitetura do projeto atual, identifique riscos e proponha melhorias.","symphony","pt-BR"),Route::Symphony);
    }
    #[test] fn override_respected() {
        assert_eq!(route("symphony: oi","solo","pt-BR"),Route::Symphony);
        assert_eq!(route("solo: analise detalhada de arquitetura e proponha melhorias completas","symphony","pt-BR"),Route::Solo);
    }
}
