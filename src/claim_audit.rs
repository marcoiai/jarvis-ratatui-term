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
}
