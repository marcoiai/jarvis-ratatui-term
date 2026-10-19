use std::{
    env,
    error::Error,
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use serde::{Deserialize, Serialize};
use tokio::{process::Command, sync::mpsc};

#[derive(Clone, Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaReply {
    message: MessageReply,
}

#[derive(Deserialize)]
struct MessageReply {
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

enum ResultMessage {
    Chat(Result<String, String>),
    Shell(Result<String, String>),
}

struct App {
    input: String,
    output: String,
    history: Vec<Message>,
    scroll: u16,
    busy: bool,
    model: String,
    ollama_url: String,
}

impl App {
    fn new() -> Self {
        Self {
            input: String::new(),
            output: String::from(concat!(
                "D.U.D.E. — Demented Unified Development Entity\n",
                "Enova Engineering Assistant | Local AI Mode\n\n",
                "DUDE initialized successfully.\n",
                "Sanity checks: FAILED.\n",
                "NOVVS diagnostics: STANDBY.\n",
                "Coffee dependency: CRITICAL.\n",
                "Ready to deploy questionable decisions (with your approval).\n\n",
                "Hey dude! What are we building today?\n",
                "Type a question, !command, /model NAME or /clear.\n"
            )),
            history: vec![Message {
                role: "system".into(),
                content: concat!(
                    "You are DUDE (Demented Unified Development Entity), a local AI-powered ",
                    "development companion. You assist with coding, Linux, Rust, diagnostics, ",
                    "and (when provided context) the Enova/NOVVS ecosystem. ",
                    "Your personality is friendly, witty, slightly demented, and engineering-focused. ",
                    "Use playful references to coffee, sanity checks, and deployments sparingly. ",
                    "Always prioritize technically accurate advice, clear uncertainty, security, ",
                    "and practical steps. Never claim to have executed a command, inspected a file, ",
                    "or changed a system unless that actually happened. ",
                    "Do not invent NOVVS project details that are not in the conversation."
                ).into(),
            }],
            scroll: 0,
            busy: false,
            model: env::var("OLLAMA_MODEL").unwrap_or_else(|_| "granite-code:latest".into()),
            ollama_url: env::var("OLLAMA_HOST")
                .unwrap_or_else(|_| "http://127.0.0.1:11434".into())
                .trim_end_matches('/')
                .to_string(),
        }
    }

    fn submit(&mut self, tx: mpsc::UnboundedSender<ResultMessage>, client: reqwest::Client) {
        if self.busy {
            return;
        }
        let input = self.input.trim().to_string();
        if input.is_empty() {
            return;
        }
        self.input.clear();
        self.output.push_str(&format!("\n> {input}\n"));
        self.scroll = u16::MAX;

        if let Some(model) = input.strip_prefix("/model ") {
            if !model.trim().is_empty() {
                self.model = model.trim().to_string();
                self.output.push_str(&format!("Model set to {}\n", self.model));
            }
            return;
        }
        if input == "/clear" {
            self.history.truncate(1);
            self.output.clear();
            self.scroll = 0;
            return;
        }

        self.busy = true;
        if let Some(command) = input.strip_prefix('!') {
            let command = command.trim().to_owned();
            tokio::spawn(async move {
                let result = run_command(command).await;
                let _ = tx.send(ResultMessage::Shell(result));
            });
        } else {
            self.history.push(Message { role: "user".into(), content: input });
            let messages = self.history.clone();
            let model = self.model.clone();
            let url = self.ollama_url.clone();
            tokio::spawn(async move {
                let result = ask_ollama(&client, &url, &model, messages).await;
                let _ = tx.send(ResultMessage::Chat(result));
            });
        }
    }

    fn complete(&mut self, result: ResultMessage) {
        self.busy = false;
        match result {
            ResultMessage::Chat(Ok(text)) => {
                self.history.push(Message { role: "assistant".into(), content: text.clone() });
                self.output.push_str(&format!("{text}\n"));
            }
            ResultMessage::Shell(Ok(text)) => self.output.push_str(&format!("{text}\n")),
            ResultMessage::Chat(Err(error)) | ResultMessage::Shell(Err(error)) => {
                self.output.push_str(&format!("Error: {error}\n"));
                // Retry should not include an unanswered question in context.
                if self.history.last().is_some_and(|m| m.role == "user") {
                    self.history.pop();
                }
            }
        }
        self.scroll = u16::MAX;
    }
}

async fn ask_ollama(
    client: &reqwest::Client,
    host: &str,
    model: &str,
    messages: Vec<Message>,
) -> Result<String, String> {
    let response = client
        .post(format!("{host}/api/chat"))
        .json(&ChatRequest { model: model.into(), messages, stream: false })
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ollama HTTP {status}: {body}"));
    }
    let reply: OllamaReply = response.json().await.map_err(|e| format!("Invalid response: {e}"))?;
    Ok(reply.message.content)
}

async fn run_command(command: String) -> Result<String, String> {
    // Intentional: execute a program and arguments, never through a shell.
    let mut words = command.split_whitespace();
    let program = words.next().ok_or("Empty command")?;
    let output = Command::new(program)
        .args(words)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = format!("{stdout}{stderr}");
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!("Exit status {}\n{text}", output.status))
    }
}

fn draw(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &App) -> io::Result<()> {
    terminal.draw(|frame| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(3)])
            .split(frame.area());

        let input = Paragraph::new(app.input.as_str())
            .block(Block::default().title("DUDE - input").borders(Borders::ALL))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(input, chunks[0]);

        let output = Paragraph::new(app.output.as_str())
            .block(Block::default().title(if app.busy { "Answer (working...)" } else { "Answer" }).borders(Borders::ALL))
            .wrap(Wrap { trim: false });
        // u16::MAX means 'bottom'; Paragraph clamps it automatically.
        frame.render_widget(output.scroll((app.scroll, 0)), chunks[1]);

        let footer = format!(
            "Ollama: {} | Model: {} | /model NAME | /clear | !ls | ESC exits",
            app.ollama_url, app.model
        );
        frame.render_widget(
            Paragraph::new(footer)
                .block(Block::default().title("Help").borders(Borders::ALL)),
            chunks[2],
        );
    })?;
    Ok(())
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    loop {
        while let Ok(result) = rx.try_recv() {
            app.complete(result);
        }
        draw(terminal, &app)?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(KeyEvent { code, modifiers, kind, .. }) = event::read()? {
                if kind != KeyEventKind::Press {
                    continue;
                }
                match code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Enter => app.submit(tx.clone(), client.clone()),
                    KeyCode::Backspace => { app.input.pop(); }
                    KeyCode::Up => app.scroll = app.scroll.saturating_sub(1),
                    KeyCode::Down => app.scroll = app.scroll.saturating_add(1),
                    KeyCode::Char(c) => app.input.push(c),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal).await;
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}
