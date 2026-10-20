//! Conservative response filter for models that leak <think> markup into content.
//! Drops only explicitly delimited leading thinking blocks; does not guess at natural language.
const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";

#[derive(Default)]
pub struct ThinkingFilter {
    pending: String,
    phase: Phase,
}

#[derive(Default)]
enum Phase { #[default] Initial, Thinking, Visible }

impl ThinkingFilter {
    pub fn push(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        loop {
            match self.phase {
                Phase::Initial => {
                    // Wait for enough bytes to distinguish a split leading tag.
                    let trimmed = self.pending.trim_start();
                    if OPEN.starts_with(trimmed) && trimmed.len() < OPEN.len() {
                        return String::new();
                    }
                    if trimmed.starts_with(OPEN) {
                        let consumed = self.pending.len() - trimmed.len() + OPEN.len();
                        self.pending.drain(..consumed);
                        self.phase = Phase::Thinking;
                    } else {
                        self.phase = Phase::Visible;
                    }
                }
                Phase::Thinking => {
                    if let Some(end) = self.pending.find(CLOSE) {
                        self.pending.drain(..end + CLOSE.len());
                        self.phase = Phase::Visible;
                    } else {
                        // Cap memory even if the model never terminates its thinking.
                        if self.pending.len() > 1_000_000 {
                            self.pending.clear();
                        }
                        return String::new();
                    }
                }
                Phase::Visible => return std::mem::take(&mut self.pending),
            }
        }
    }

    pub fn finish(self) -> Result<String, String> {
        match self.phase {
            Phase::Thinking => Err("Ollama retornou um bloco <think> sem fechamento; resposta descartada.".into()),
            Phase::Initial | Phase::Visible => Ok(self.pending),
        }
    }
}

pub fn strip_completed(content: &str) -> Result<String, String> {
    // Some Ollama templates omit the opening tag while still returning </think>.
    // Filter only when a closing delimiter is present: do not guess from prose.
    let trimmed=content.trim_start();
    if !trimmed.starts_with(OPEN) {
        if let Some(end)=content.find(CLOSE) {
            return Ok(content[end+CLOSE.len()..].trim_start().to_owned());
        }
    }
    let mut filter = ThinkingFilter::default();
    let visible = filter.push(content);
    Ok(format!("{}{}", visible, filter.finish()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_thinking_tags_do_not_leak() {
        let mut f=ThinkingFilter::default();
        assert_eq!(f.push("<thi"), "");
        assert_eq!(f.push("nk>segredo"), "");
        assert_eq!(f.push(" interno</thi"), "");
        assert_eq!(f.push("nk>Resposta"), "Resposta");
        assert_eq!(f.push(" final"), " final");
        assert_eq!(f.finish().unwrap(), "");
    }
    #[test]
    fn ordinary_text_passes_through() {
        assert_eq!(strip_completed("Olá, buddy!").unwrap(), "Olá, buddy!");
        assert_eq!(strip_completed("<think>interno</think>Resposta").unwrap(), "Resposta");
        assert!(strip_completed("<think>sem fechamento").is_err());
        assert_eq!(strip_completed("Reasoning without opening tag</think>Resposta").unwrap(),"Resposta");
    }
}
