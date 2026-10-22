//! Conservative, deterministic post-response evidence checks.
//! A cited line being observed does NOT establish semantic support for a claim.
use crate::evidence::EvidenceCache;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Audit {
    pub unchecked_references: Vec<String>,
    pub coverage_conflict: bool,
    pub falsely_unread: Vec<String>,
}

fn references(answer: &str) -> Vec<String> {
    let mut refs = std::collections::BTreeSet::new();
    // Recognizes literal relative file:line mentions; ignores links, line ranges
    // and unnumbered references (those remain unverified).
    for word in answer.split_whitespace() {
        let word = word.trim_matches(|c: char| matches!(c, '`' | '*' | '(' | ')' | '[' | ']' | ',' | '.' | ';' | '"' | '\'' ));
        if let Some((path, rest)) = word.rsplit_once(':') {
            let line = rest.trim_end_matches(|c: char| !c.is_ascii_digit());
            if !path.is_empty() && !path.contains("://") && !path.starts_with('/')
                && path.contains('.') && !line.is_empty() && line.chars().all(|c| c.is_ascii_digit())
                && line.parse::<usize>().is_ok_and(|n| n > 0)
            {
                refs.insert(format!("{path}:{line}"));
            }
        }
    }
    refs.into_iter().collect()
}

pub fn audit(answer: &str, evidence: &EvidenceCache) -> Audit {
    let unchecked_references = references(answer).into_iter()
        .filter(|reference| !evidence.observed_reference(reference)).collect();
    let lower = answer.to_lowercase();
    // Only match explicit assertions of no access; avoid treating partial
    // coverage disclaimers as contradictions.
    let denial = [
        "nenhum arquivo foi lido", "nenhum arquivo foi acessado",
        "nenhum código foi lido", "workspace vazio",
        "no source files were read", "no files were read",
        "workspace is empty",
        "apenas snippets e readme foram acessíveis",
        "apenas snippets e readme foram acessiveis",
        "apenas snippets e readme foram acessados",
        "código base completo precisa ser carregado",
    ].iter().any(|phrase| lower.contains(phrase));
    let coverage_conflict = denial && evidence.read_manifest().starts_with("READ SOURCE EXCERPTS");
    let manifest = evidence.read_manifest();
    let read_paths: Vec<String> = manifest.lines().skip(1)
        .filter_map(|line|line.split_once(": ").map(|(path,_)|path.trim().to_lowercase()))
        .collect();
    let mut falsely_unread = std::collections::BTreeSet::new();
    for line in answer.lines() {
        let normalized = line.to_lowercase();
        let unread = ["não lido","não leido","não foi lido","nao lido","nao leido",
                      "not read","not accessed"].iter().any(|phrase|normalized.contains(phrase));
        if !unread {continue;}
        for path in &read_paths {
            // Check only declarations of unread files, not paragraphs discussing
            // why another report's unread declarations were mistaken.
            if normalized.trim_start().starts_with(['-', '*', '|'])
                && normalized.contains(path)
                && !normalized.contains("incorreto")
                && !normalized.contains("falso") {
                falsely_unread.insert(path.clone());
            }
        }
    }
    Audit { unchecked_references, coverage_conflict, falsely_unread: falsely_unread.into_iter().collect() }
}

pub fn notice(answer: &str, evidence: &EvidenceCache) -> String {
    let report = audit(answer, evidence);
    if report.unchecked_references.is_empty() && !report.coverage_conflict && report.falsely_unread.is_empty() {
        return String::new();
    }
    let mut lines = vec!["\n\n⚠️ EvidenceLedger (checagem automática, sem validação semântica):".to_owned()];
    if report.coverage_conflict {
        lines.push("- CONTRADIÇÃO: a resposta declara ausência de arquivos, mas o runtime registrou leituras de código.".into());
    }
    if !report.unchecked_references.is_empty() {
        lines.push(format!("- Referências arquivo:linha NÃO observadas nesta execução: {}",
            report.unchecked_references.join(", ")));
    }
    if !report.falsely_unread.is_empty() {
        lines.push(format!("- Arquivos incorretamente declarados como não lidos: {}",report.falsely_unread.join(", ")));
    }
    lines.push(format!("- Manifesto do runtime: {}", evidence.coverage()));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reports_only_unobserved_references() {
        let root = std::env::temp_dir().join(format!("dude-ledger-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("sample.rs"), "first\nsecond\n").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "read_file", &json!({"path":"sample.rs","start_line":2,"end_line":2}));
        let report = audit("sample.rs:2 válido; sample.rs:1 não visto", &evidence);
        assert_eq!(report.unchecked_references, vec!["sample.rs:1"]);
        assert!(!report.coverage_conflict);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn catches_incompatible_coverage_statement() {
        let root = std::env::temp_dir().join(format!("dude-ledger-denial-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("sample.rs"), "hello\n").unwrap();
        let evidence = EvidenceCache::default();
        evidence.execute(&root, "read_file", &json!({"path":"sample.rs"}));
        assert!(audit("Workspace vazio. Nenhum arquivo foi lido.", &evidence).coverage_conflict);
        assert!(!audit("Apenas parte dos arquivos foi lida.", &evidence).coverage_conflict);
        assert!(audit("Até agora, apenas snippets e README foram acessíveis.", &evidence).coverage_conflict);
        assert_eq!(audit("- \`sample.rs\`: NÃO LIDO", &evidence).falsely_unread, vec!["sample.rs"]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
