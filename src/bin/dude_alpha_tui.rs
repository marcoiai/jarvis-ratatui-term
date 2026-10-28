//! Experimental DUDE Alpha TUI: read-only workspace specialists + MLX analysis.
//! Uses existing Investigator and Inspector; does not execute model-generated commands.
use std::{env, io::{self, Stdout}, path::{Path, PathBuf}, process::Command, sync::mpsc::{self, Receiver}, thread};
use crossterm::{event::{self, Event, KeyCode, KeyEventKind}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::{backend::CrosstermBackend, layout::{Constraint, Direction, Layout}, style::{Color, Style}, widgets::{Block, Borders, Paragraph, Wrap}, Terminal};
use serde_json::Value;
type Term = Terminal<CrosstermBackend<Stdout>>;

fn run_specialist(name: &str, args: &[&str]) -> Result<String, String> {
    let exe = env::current_exe().map_err(|e| e.to_string())?
        .with_file_name(format!("{name}{}", env::consts::EXE_SUFFIX));
    if !exe.is_file() { return Err(format!("Build specialists first: cargo build --bins ({name})")); }
    let out = Command::new(exe).args(args).env("GIT_OPTIONAL_LOCKS", "0")
        .output().map_err(|e| format!("{name}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{name}: {}", String::from_utf8_lossy(&out.stderr)));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}
fn render_investigation(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(v) => {
            let mut summary = format!("Investigator: {} | proposta, NÃO verificada semanticamente\n", v["status"].as_str().unwrap_or("unknown"));
            if let Some(findings) = v["validated_findings"].as_array() {
                if findings.is_empty() { summary.push_str("Nenhuma evidência aceita.\n"); }
                for (i, f) in findings.iter().enumerate() {
                    let lines = f["line_ids"].as_array().map(|xs| xs.iter().take(8).filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                    summary.push_str(&format!("\n{}. {}\n   evidência: {} [{}{}]\n", i+1, f["claim"].as_str().unwrap_or(""), f["evidence_ids"][0].as_str().unwrap_or(""), lines, if f["line_ids"].as_array().is_some_and(|x|x.len()>8) {", ..."} else {""}));
                }
            }
            if let Some(rejected) = v["rejected_findings"].as_array() { if !rejected.is_empty() { summary.push_str(&format!("\n{} afirmações rejeitadas.\n",rejected.len())); } }
            if let Some(uncertain) = v["uncertainty"].as_array() { for u in uncertain { if let Some(t)=u.as_str(){summary.push_str(&format!("\nIncerteza: {t}\n"));} } }
            summary.push_str("\nSem modificações no workspace.");
            summary
        }
        Err(_) => format!("Saída não interpretável do Investigator:\n{}", raw.chars().take(2200).collect::<String>()),
    }
}
fn investigate(source: String, dest: String, question: String) -> Result<String, String> {
    // First vertical slice: deterministic routing, no model-driven tool execution.
    // Only explicit path tokens or the conventional entry point are considered.
    let file = question.split_whitespace().map(|w|w.trim_matches(|c:char| !c.is_alphanumeric() && c!='/' && c!='.' && c!='_'))
        .find(|w|w.starts_with("src/") && (w.ends_with(".rs") || w.ends_with(".toml")))
        .unwrap_or("src/main.rs").to_string();
    let hunk = question.split_whitespace().find_map(|w|w.parse::<usize>().ok()).filter(|n|*n>0).unwrap_or(1);
    let left=Path::new(&source).join(&file);
    let right=Path::new(&dest).join(&file);
    if !left.is_file() || !right.is_file() {
        return Err(format!("O alpha só compara arquivos presentes nos dois workspaces. Arquivo escolhido: {file}. Tente: analisa src/main.rs 7"));
    }
    let inspector=run_specialist("workspace_inspector", &[&dest])?;
    let state:Value=serde_json::from_str(&inspector).map_err(|e|e.to_string())?;
    let git_type=state["git_kind"].as_str().unwrap_or("unknown");
    let raw=run_specialist("workspace_investigator", &[&source,&dest,"--file",&file,"--hunk",&hunk.to_string()])?;
    Ok(format!("Pergunta: {question}\nWorkspace: {} ({git_type})\nArquivo: {file} | hunk {hunk}\n\n{}",dest,render_investigation(&raw)))
}
fn init() -> Result<Term,String> {
    enable_raw_mode().map_err(|e|e.to_string())?;
    let mut stdout=io::stdout();
    if let Err(e)=execute!(stdout, EnterAlternateScreen){let _=disable_raw_mode();return Err(e.to_string());}
    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(t)=>Ok(t),
        Err(e)=>{let _=disable_raw_mode();let _=execute!(io::stdout(),LeaveAlternateScreen);Err(e.to_string())}
    }
}
fn restore(mut t:Term){let _=disable_raw_mode();let _=execute!(t.backend_mut(),LeaveAlternateScreen);let _=t.show_cursor();}
fn loop_ui(t:&mut Term,source:String,dest:String)->Result<(),String>{
    let mut input=String::new();
    let mut output="DUDE Alpha / TUI\n\nDigite: analisa o projeto\nOu: analisa src/main.rs 7\n\nPrimeiro protótipo: compara workspaces e usa o Investigator via MLX.\nSem escrita. Sem comandos livres.".to_string();
    let mut status="Pronto".to_string();
    let (tx,rx):(mpsc::Sender<Result<String,String>>,Receiver<Result<String,String>>)=mpsc::channel();
    let mut busy=false;
    let mut scroll:u16=0;
    loop {
        if let Ok(result)=rx.try_recv() {
            busy=false;status="Pronto".into();
            output=match result {Ok(x)=>x,Err(e)=>format!("Falha observável:\n{e}")};
            scroll=0;
        }
        t.draw(|f|{
            let areas=Layout::default().direction(Direction::Vertical)
                .constraints([Constraint::Length(3),Constraint::Min(5),Constraint::Length(3),Constraint::Length(2)]).split(f.area());
            let title=Paragraph::new(format!(" DUDE ALPHA 0.1 | Qwen / MLX | {status}"))
                .style(Style::default().fg(Color::Cyan)).block(Block::default().borders(Borders::ALL));
            f.render_widget(title,areas[0]);
            let body=Paragraph::new(output.as_str()).wrap(Wrap{trim:false}).scroll((scroll,0))
                .block(Block::default().title(" Investigação / evidências ").borders(Borders::ALL));
            f.render_widget(body,areas[1]);
            let prompt=Paragraph::new(format!("> {input}")).block(Block::default().title(" Pedido ").borders(Borders::ALL));
            f.render_widget(prompt,areas[2]);
            f.render_widget(Paragraph::new("Enter envia | ↑/↓ rola | Esc sai | somente leitura"),areas[3]);
        }).map_err(|e|e.to_string())?;
        if event::poll(std::time::Duration::from_millis(100)).map_err(|e|e.to_string())? {
            if let Event::Key(key)=event::read().map_err(|e|e.to_string())? {
                if key.kind!=KeyEventKind::Press {continue;}
                match key.code {
                    KeyCode::Esc if !busy => break,
                    KeyCode::Up =>scroll=scroll.saturating_sub(2),
                    KeyCode::Down =>scroll=scroll.saturating_add(2),
                    KeyCode::Backspace if !busy =>{input.pop();},
                    KeyCode::Char(c) if !busy =>input.push(c),
                    KeyCode::Enter if !busy && !input.trim().is_empty()=>{
                        let question=std::mem::take(&mut input);
                        let src=source.clone();let dst=dest.clone();let sender=tx.clone();
                        busy=true;status="Investigando...".into();
                        output=format!("Investigando: {question}\n\n1. Workspace Inspector\n2. Diff / Investigator\n3. Conferência de referências\n\nO modelo pode demorar ou falhar.");
                        thread::spawn(move ||{let _=sender.send(investigate(src,dst,question));});
                    }
                    _=>{}
                }
            }
        }
    }
    Ok(())
}
fn main()->std::process::ExitCode{
    let args:Vec<String>=env::args().skip(1).collect();
    if args.len()!=2 {eprintln!("Usage: dude_alpha_tui SOURCE_WORKSPACE DEST_WORKSPACE");return std::process::ExitCode::from(2);}
    let mut paths=Vec::new();
    for p in args {
        match PathBuf::from(&p).canonicalize() {
            Ok(x) if x.is_dir()=>paths.push(x.to_string_lossy().to_string()),
            _=>{eprintln!("Invalid directory: {p}");return std::process::ExitCode::from(2);}
        }
    }
    let mut t=match init(){Ok(t)=>t,Err(e)=>{eprintln!("{e}");return std::process::ExitCode::FAILURE;}};
    let result=loop_ui(&mut t,paths[0].clone(),paths[1].clone());
    restore(t);
    if let Err(e)=result{eprintln!("{e}");std::process::ExitCode::FAILURE}else{std::process::ExitCode::SUCCESS}
}
