#[allow(unused_imports)]
use std::process::Command;

use serde_derive::Deserialize;
use serde_json;

#[derive(Debug, Deserialize)]
struct MyText {
    text: String,
}

#[derive(Debug, Deserialize)]
struct Part {
    parts: Vec<MyText>,
}

#[derive(Debug, Deserialize)]
struct Content {
    content: Part,
}

#[derive(Debug, Deserialize)]
struct Candidates {
    content: Content,
}

#[derive(Debug, Deserialize)]
struct Cand {
    candidates: Candidates,
}

use std::{
    error::Error,
    io,
    sync::{Arc, Mutex},
    time::Duration,
    collections::HashMap,
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Text,
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};

fn get_vars() -> HashMap<std::string::String, std::string::String> {
    let mut vars: HashMap<String, String> = HashMap::<String, String>::new();
    
    for (key, value) in std::env::vars() {
        vars.insert(key, value);
    }

    return vars;
}

async fn get_answer(question: String) -> String {
    let vars = get_vars();
    let api_key = &vars["API_KEY"];

    let output = Command::new("./post.sh")
        .args([&question, api_key])
        .output()
        .expect("Error loading CURL result.");

    if output.status.success() {
        let out: std::borrow::Cow<'_, str> = String::from_utf8_lossy(&output.stdout);
        let v: Cand = serde_json::from_str(&out).expect("Error creating object using serde!");

        return v.candidates.content.content.parts[0].text
            .replace(r"**", "")
            .replace("* ", "\n📝");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Error executing curl: {}", stderr);
        return "Erro ao obter resposta da API.".to_string();
    }
}

#[derive(Clone, Debug)]
struct App {
    input: String,
    answer: Arc<Mutex<String>>,
    scroll_offset: usize,
    is_loading: bool,
}

impl App {
    fn new() -> App {
        App {
            input: String::new(),
            answer: Arc::new(Mutex::new(String::new())),
            scroll_offset: 0,
            is_loading: false,
        }
    }

    fn on_tick(&mut self) {
        
        // Loading concept
        // self.input2 += ".";
    }

    fn on_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.input.push(c);
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::Enter => {
                let question: String = self.input.clone();
                self.input.clear();
                self.is_loading = true;

                let answer_clone = self.answer.clone();
                tokio::spawn(async move {
                    let output_result: String = if question.starts_with("!") {
                        let cmd: &str = question.trim_start_matches("!").trim();
                        let parts: Vec<&str> = cmd.split_whitespace().collect();
                        if parts.is_empty() {
                            "Empty Command.".to_string()
                        } else {
                            let output = Command::new(parts[0])
                                .args(&parts[1..])
                                .output();

                            match output {
                                Ok(out) if out.status.success() => {
                                    String::from_utf8_lossy(&out.stdout).to_string()
                                }
                                Ok(out) => {
                                    format!("Erro: {}", String::from_utf8_lossy(&out.stderr))
                                }
                                Err(e) => format!("Falha ao executar: {}", e),
                            }
                        }
                    } else {
                        get_answer(question).await
                    };

                    let mut answer = answer_clone.lock().unwrap();
                    *answer = "\n\n".to_owned() + &output_result;
                });

                self.scroll_offset = 0;
                self.is_loading = false;
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout: io::Stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend: CrosstermBackend<io::Stdout> = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<io::Stdout>> = Terminal::new(backend)?;

    let default_panic: Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static> = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info: &std::panic::PanicHookInfo<'_>| {
        disable_raw_mode().unwrap();
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture).unwrap();
        default_panic(info);
    }));

    let app: Arc<Mutex<App>> = Arc::new(Mutex::new(App::new()));

    let res: Result<(), Box<dyn Error + 'static>> = run_app(&mut terminal, app.clone()).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app_mutex: Arc<Mutex<App>>,
) -> Result<(), Box<dyn Error>> {
    loop {
        terminal.draw(|f| {
            let app: std::sync::MutexGuard<'_, App> = app_mutex.lock().unwrap();
            let chunks: std::rc::Rc<[ratatui::prelude::Rect]> = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(4),
                        Constraint::Length(5),
                    ]
                    .as_ref(),
                )
                .split(f.area());

            let input: Paragraph<'_> = Paragraph::new(Text::from(app.input.as_str()))
                .block(Block::default().title("💁🏻  Welcome sir, I'm JARVIS your terminal assistant.Use ▲ ▼ to scroll.").borders(Borders::ALL))
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(input, chunks[0]);

            let answer_text = app.answer.lock().unwrap().clone();
            let answer_paragraph = Paragraph::new(Text::from(answer_text))
            .block(Block::default().title("Answer").borders(Borders::ALL))
                .style(Style::default().bg(Color::White))
                .scroll((app.scroll_offset as u16, 0))
                .wrap(Wrap { trim: false });

            f.render_widget(answer_paragraph, chunks[1]);

            let input3 = Paragraph::new(Text::from("Press ESC to exit.\nFor reasoning, just type what you want to know, and I'll do my best.\nFor terminal commands, type !command (!ls for example)."))
                .block(Block::default().title("⚠️").borders(Borders::ALL))
                .style(Style::default().fg(Color::White));

            f.render_widget(input3, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => return Ok(()),
                    _ => {
                        let mut app: std::sync::MutexGuard<'_, App> = app_mutex.lock().unwrap();
                        app.on_key(key);
                    }
                }
            }
        }

        let mut app: std::sync::MutexGuard<'_, App> = app_mutex.lock().unwrap();

        app.on_tick();
    }
}
