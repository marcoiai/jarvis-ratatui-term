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
pub fn route(input:&str, mode:&str)->Route {
    let query=input.trim();
    let lower=query.to_lowercase();
    if matches!(lower.as_str(), "?" | "help" | "/help") {
        return Route::Local("DUDE commands: /help, /roles, /mode solo|symphony, /solo ROLE, /role ROLE PROVIDER, /workspace PATH, /memory, /knowledge, /cloud allow|deny, /clear, /provider PROVIDER, /model NAME, /apikey, /apikey gemini|groq (masked entry), /apikey clear PROVIDER, /env, /align, /metrics.\nOllama: OLLAMA_NUM_CTX (default 16384). Investigation: DUDE_TOOL_ROUNDS (default 8, max 32).\nAsk a specific question to begin. Force full orchestration with 'symphony: YOUR QUESTION'.".into());
    }
    if matches!(lower.as_str(),"oi"|"olá"|"ola"|"hey"|"hi"|"hello"|"bom dia"|"boa tarde"|"boa noite"|"eae"|"e aí") {
        return Route::Local("Hey dude! Ask me something about your project. Use /help for commands. 🦀".into());
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
    #[test] fn trivial_is_local() {
        for input in ["?","/help","oi","Bom dia"] {assert!(matches!(route(input,"symphony"),Route::Local(_))); }
    }
    #[test] fn simple_in_symphony_uses_solo() {
        assert_eq!(route("O que é Tokio?","symphony"),Route::Solo);
        assert_eq!(route("Como faço para listar arquivos?","symphony"),Route::Solo);
    }
    #[test] fn complex_in_symphony_uses_all_roles() {
        assert_eq!(route("Explique a arquitetura do projeto atual, identifique riscos e proponha melhorias.","symphony"),Route::Symphony);
    }
    #[test] fn override_respected() {
        assert_eq!(route("symphony: oi","solo"),Route::Symphony);
        assert_eq!(route("solo: analise detalhada de arquitetura e proponha melhorias completas","symphony"),Route::Solo);
    }
}
