//! Lightweight deterministic checks for unsupported numerical coverage claims.
//! Not a semantic proof checker; never promotes assertions to verified facts.
pub fn unsupported_coverage_percentages(answer: &str) -> bool {
    answer.lines().any(|line| {
        let lower = line.to_lowercase();
        let mentions_coverage = ["cobertura", "coverage", "fluxos", "components", "componentes"]
            .iter().any(|term| lower.contains(term));
        if !mentions_coverage { return false; }
        let bytes = line.as_bytes();
        bytes.iter().enumerate().any(|(i, ch)| {
            if *ch != b'%' { return false; }
            let mut j=i;
            while j>0 && bytes[j-1].is_ascii_whitespace(){ j-=1; }
            let end=j;
            while j>0 && bytes[j-1].is_ascii_digit(){ j-=1; }
            end>j
        })
    })
}

/// Adds a runtime-generated evidence status to broad project reports.
/// A confirmed citation proves only that a source line was observed, not that a claim is true.
pub fn project_report_preamble(answer: &str, coverage: &str, has_supported_citation: bool) -> String {
    let absolute_claims = unsupported_completeness_claims(answer);
    let citation_status = if has_supported_citation {
        "Pelo menos uma referência arquivo:linha foi confirmada nas leituras; isso não valida automaticamente todas as afirmações."
    } else {
        "Nenhuma referência arquivo:linha foi confirmada nas leituras; as conclusões abaixo não devem ser tratadas como verificadas."
    };
    let claim_status = if absolute_claims {
        "Foram detectadas afirmações absolutas sobre completude, funcionamento ou ausência de falhas. Trate-as como NÃO VERIFICADAS."
    } else {
        "A análise continua parcial: ausência de alerta textual não prova que as conclusões estejam corretas."
    };
    format!(
        "⚠ DUDE — STATUS OBJETIVO DA EVIDÊNCIA\n- Escopo: análise parcial; leitura de arquivos não comprova o comportamento global.\n- Citações: {citation_status}\n- Auditoria de afirmações: {claim_status}\n- {coverage}\n\n--- Relatório do modelo (não é validação automática) ---\n\n{answer}"
    )
}

/// Flags broad completeness/verification claims that cannot be established by a
/// sampled set of source reads. This is a heuristic warning, not a semantic proof.
pub fn unsupported_completeness_claims(answer: &str) -> bool {
    answer.lines().any(|line| {
        let lower = line.to_lowercase();
        let absolute_scope = [
            "100%", "fully", "completely", "entire architecture", "all modules",
            "all components", "everything is", "todos os módulos", "todos os componentes",
            "toda a arquitetura", "arquitetura inteira", "totalmente", "completamente",
            "integralmente", "100 por cento", "sem necessidade de mudanças",
            "without need for changes", "não há evidências de bugs",
            "não há evidência de bugs", "não há evidências de falhas",
            "não há evidência de falhas", "no evidence of bugs", "no evidence of failures",
            "no critical bugs", "nenhum bug crítico", "nenhuma falha crítica",
        ].iter().any(|term| lower.contains(term));
        let certainty = [
            "verified", "functional", "implemented", "audited", "tested", "works",
            "verificado", "verificada", "funcional", "implementado", "implementada",
            "auditado", "auditada", "testado", "testada", "funciona", "funcionando",
            "necessidade de mudanças", "no changes needed", "no changes required",
        ].iter().any(|term| lower.contains(term));
        absolute_scope && certainty
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flags_unmeasured_coverage_claims() {
        assert!(unsupported_coverage_percentages("Cobertura: 100% dos componentes críticos"));
        assert!(unsupported_coverage_percentages("85 % dos fluxos analisados"));
        assert!(!unsupported_coverage_percentages("13 arquivos lidos; cobertura parcial"));
        assert!(!unsupported_coverage_percentages("latência 85% menor"));
    }

    #[test]
    fn flags_unproven_absolute_completeness_claims() {
        assert!(unsupported_completeness_claims("Arquitetura 100% verificada e funcional."));
        assert!(unsupported_completeness_claims("Todos os componentes foram testados."));
        assert!(unsupported_completeness_claims("The entire architecture is fully implemented."));
        assert!(unsupported_completeness_claims("Não há evidências de bugs críticos ou falhas na funcionalidade atual."));
        assert!(unsupported_completeness_claims("Sem necessidade de mudanças."));
        assert!(!unsupported_completeness_claims("Somente src/main.rs foi verificado."));
        assert!(!unsupported_completeness_claims("Há funcionalidades ainda não verificadas."));
    }

    #[test]
    fn project_report_always_labels_partial_evidence() {
        let report = project_report_preamble(
            "O sistema funciona 100% conforme verificado.",
            "Source reads: 13; sampled only.",
            false,
        );
        assert!(report.contains("análise parcial"));
        assert!(report.contains("Nenhuma referência arquivo:linha foi confirmada"));
        assert!(report.contains("NÃO VERIFICADAS"));
        assert!(report.contains("Source reads: 13"));
        assert!(report.contains("O sistema funciona 100% conforme verificado."));
    }
}
