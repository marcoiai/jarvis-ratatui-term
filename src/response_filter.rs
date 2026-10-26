//! Filters reasoning delimiters before anything reaches the terminal.
//! Ollama models sometimes emit a closing </think> without its opening tag.
//! Until a delimiter is resolved, buffer rather than stream potentially private reasoning.
const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";
const MAX_BUFFER: usize = 1_000_000;

#[derive(Default)]
pub struct ThinkingFilter {
    pending: String,
    phase: Phase,
    stream_plain: bool,
}

#[derive(Default)]
enum Phase { #[default] Initial, Thinking, Visible }

impl ThinkingFilter {
    /// Ollama's structured `message.thinking` is handled separately by the UI.
    /// Stream ordinary `message.content` as soon as it is distinguishable
    /// from an inline opening <think> delimiter.
    pub fn streaming() -> Self {
        Self { stream_plain: true, ..Self::default() }
    }

    pub fn push(&mut self, chunk: &str) -> Result<String, String> {
        self.pending.push_str(chunk);
        if self.pending.len() > MAX_BUFFER && !matches!(self.phase, Phase::Visible) {
            return Err("Ollama retornou texto não delimitado acima do limite seguro; resposta descartada.".into());
        }
        loop {
            match self.phase {
                Phase::Initial => {
                    let trimmed = self.pending.trim_start();
                    if trimmed.starts_with(OPEN) {
                        let consumed = self.pending.len() - trimmed.len() + OPEN.len();
                        self.pending.drain(..consumed);
                        self.phase = Phase::Thinking;
                    } else if let Some(end) = self.pending.find(CLOSE) {
                        // Legacy models may omit <think>; retain filtering for a
                        // complete delimiter already present in the buffered text.
                        self.pending.drain(..end + CLOSE.len());
                        self.phase = Phase::Visible;
                    } else if self.stream_plain && !trimmed.is_empty() && !OPEN.starts_with(trimmed) {
                        // Ordinary Ollama content has no think tags. The old
                        // filter buffered ALL of it until done=true, defeating
                        // streaming even for a simple greeting.
                        self.phase = Phase::Visible;
                    } else {
                        // Wait for a possibly split opening delimiter.
                        return Ok(String::new());
                    }
                }
                Phase::Thinking => {
                    if let Some(end) = self.pending.find(CLOSE) {
                        self.pending.drain(..end + CLOSE.len());
                        self.phase = Phase::Visible;
                    } else {
                        return Ok(String::new());
                    }
                }
                Phase::Visible => return Ok(std::mem::take(&mut self.pending)),
            }
        }
    }

    pub fn finish(self) -> Result<String, String> {
        match self.phase {
            Phase::Thinking => Err("Ollama retornou um bloco <think> sem fechamento; resposta descartada.".into()),
            Phase::Initial if self.pending.trim_start().starts_with(OPEN) => Err("Ollama retornou uma tag <think> incompleta.".into()),
            Phase::Initial | Phase::Visible => Ok(self.pending),
        }
    }
}

pub fn strip_completed(content: &str) -> Result<String, String> {
    let mut filter = ThinkingFilter::default();
    let visible = filter.push(content)?;
    Ok(format!("{}{}", visible, filter.finish()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_thinking_tags_do_not_leak() {
        let mut f=ThinkingFilter::default();
        assert_eq!(f.push("<thi").unwrap(), "");
        assert_eq!(f.push("nk>segredo").unwrap(), "");
        assert_eq!(f.push(" interno</thi").unwrap(), "");
        assert_eq!(f.push("nk>Resposta").unwrap(), "Resposta");
        assert_eq!(f.push(" final").unwrap(), " final");
        assert_eq!(f.finish().unwrap(), "");
    }
    #[test]
    fn missing_opening_tag_is_not_streamed() {
        let mut f=ThinkingFilter::default();
        assert_eq!(f.push("Okay, let me think in English.").unwrap(), "");
        assert_eq!(f.push(" More internal planning.").unwrap(), "");
        assert_eq!(f.push("</thi").unwrap(), "");
        assert_eq!(f.push("nk>Resposta").unwrap(), "Resposta");
        assert_eq!(f.finish().unwrap(), "");
    }
    #[test]
    fn plain_response_is_returned_on_finish() {
        let mut f=ThinkingFilter::default();
        assert_eq!(f.push("Olá, buddy!").unwrap(), "");
        assert_eq!(f.finish().unwrap(), "Olá, buddy!");
    }
    #[test]
    fn ordinary_streaming_content_is_visible_before_finish() {
        let mut filter = ThinkingFilter::streaming();
        assert_eq!(filter.push("Olá, ").unwrap(), "Olá, ");
        assert_eq!(filter.push("buddy!").unwrap(), "buddy!");
        assert_eq!(filter.finish().unwrap(), "");
    }
    #[test]
    fn streaming_still_hides_split_inline_think_tags() {
        let mut filter = ThinkingFilter::streaming();
        assert_eq!(filter.push("<thi").unwrap(), "");
        assert_eq!(filter.push("nk>internal").unwrap(), "");
        assert_eq!(filter.push("</think>Oi").unwrap(), "Oi");
        assert_eq!(filter.finish().unwrap(), "");
    }
    #[test]
    fn completed_response_filters_both_templates() {
        assert_eq!(strip_completed("<think>interno</think>Resposta").unwrap(),"Resposta");
        assert_eq!(strip_completed("interno</think>Resposta").unwrap(),"Resposta");
        assert_eq!(strip_completed("normal").unwrap(),"normal");
        assert!(strip_completed("<think>sem fechamento").is_err());
    }
    #[test]
    fn final_response_can_be_empty_after_reasoning_is_removed() {
        assert_eq!(strip_completed("<think>reasoning only</think>").unwrap(), "");
        assert_eq!(strip_completed("hidden reasoning</think>  ").unwrap().trim(), "");
    }
    #[test]
    fn final_response_remains_visible_without_thinking_tags() {
        let answer="Project review: src/main.ts has an entrypoint.";
        assert_eq!(strip_completed(answer).unwrap(),answer);
    }
    #[test]
    fn oversized_undelimited_text_is_rejected() {
        let mut f=ThinkingFilter::default();
        assert!(f.push(&"x".repeat(MAX_BUFFER+1)).is_err());
    }
}
