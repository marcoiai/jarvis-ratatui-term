//! Resolve read-only runtime questions locally, without trusting the LLM.
//! Not a command parser: changes to runtime state still require explicit slash commands.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent { Workspace, Model, Mode }

pub fn resolve(query: &str) -> Option<Intent> {
    let mut q = query.trim().to_lowercase();
    for (from,to) in [('á',"a"),('ã',"a"),('â',"a"),('é',"e"),('ê',"e"),('í',"i"),('ó',"o"),('ô',"o"),('õ',"o"),('ú',"u"),('ç',"c")] {
        q=q.replace(from,to);
    }
    if q.starts_with('/') || q.starts_with('!') || q.starts_with("solo:")
        || q.starts_with("symphony:") || q.starts_with("sinfonica:") { return None; }
    let ask = ["qual","quais","onde","mostre","mostrar","me diga","diga","informe","estamos usando","esta usando","atual","current","which","what","where","show"]
        .iter().any(|v|q.contains(v));
    if !ask { return None; }
    if ["analise","analisar","analisa","audite","auditoria","refatore","refatorar","implemente","implementar","codigo","codebase","arquivos","files","estrutura","arquitetura","trocar","mudar","alterar","change","switch","setar","defina","configure"]
        .iter().any(|v|q.contains(v)) { return None; }
    if ["workspace","diretorio de trabalho","diretorio atual","pasta do projeto","working directory"].iter().any(|v|q.contains(v)) {
        return Some(Intent::Workspace);
    }
    if ["modelo atual","qual modelo","modelo estamos usando","modelo voce usa","current model","which model"].iter().any(|v|q.contains(v)) {
        return Some(Intent::Model);
    }
    if ["modo atual","qual modo","modo estamos usando","current mode"].iter().any(|v|q.contains(v)) {
        return Some(Intent::Mode);
    }
    None
}

pub fn answer(intent: Intent, workspace: &Path, model: &str, mode: &str) -> String {
    match intent {
        Intent::Workspace => format!("Workspace atual: {}\n",workspace.display()),
        Intent::Model => format!("Modelo Ollama configurado: {model}\n"),
        Intent::Mode => format!("Modo DUDE atual: {mode}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn workspace_queries() {
        for q in ["qual era o workspace atual?", "Qual é o workspace atual?", "onde estamos trabalhando?", "mostre o diretório de trabalho atual"] {
            if q.contains("onde estamos trabalhando") { assert_eq!(resolve(q),None); }
            else {assert_eq!(resolve(q),Some(Intent::Workspace));}
        }
        let got=answer(Intent::Workspace,Path::new("/home/marco/projeto"),"qwen","solo");
        assert_eq!(got,"Workspace atual: /home/marco/projeto\n");
    }
    #[test] fn avoids_changes_and_analysis() {
        for q in ["analise o código do workspace", "mudar workspace para /tmp", "/workspace", "refatore o workspace", "qual estrutura do workspace?"] {
            assert_eq!(resolve(q),None,"{q}");
        }
        assert_eq!(resolve("qual modelo atual?"),Some(Intent::Model));
        assert_eq!(resolve("qual modo atual?"),Some(Intent::Mode));
    }
}
