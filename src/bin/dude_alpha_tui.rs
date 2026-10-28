//! Experimental DUDE Alpha TUI: read-only workspace specialists + MLX analysis.
//! Uses existing Investigator and Inspector; does not execute model-generated commands.
use std::{env, io::{self, Stdout}, path::{Path, PathBuf}, process::Command, sync::mpsc::{self, Receiver}, thread};
use crossterm::{event::{self, Event, KeyCode, KeyEventKind}, execute, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::{backend::CrosstermBackend, layout::{Constraint, Direction, Layout}, style::{Color, Style}, widgets::{Block, Borders, Paragraph, Wrap}, Terminal};
use serde_json::Value;
type Term = Terminal<CrosstermBackend<Stdout>>;
#[derive(Clone)]
struct ReviewResult { summary: String, evidence: Vec<String> }
fn evidence_views(report: &Value, diff: &Value) -> Result<Vec<String>, String> {
    if report["source_blob"] != diff["source_blob"] || report["destination_blob"] != diff["destination_blob"] {
        return Err("Diff alterado durante a investigação; evidências não exibidas".into());
    }
    let raw = diff["diff"]["hunks"][0]["content"].as_str().ok_or("Hunk ausente")?;
    let rows: Vec<&str> = raw.lines().collect();
    let mut views=Vec::new();
    for (idx,finding) in report["validated_findings"].as_array().into_iter().flatten().enumerate() {
        let mut lines=String::new(); let mut adds=0;let mut removals=0;let mut context=0;
        for id in finding["line_ids"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let line=id.strip_prefix('D').and_then(|n|n.parse::<usize>().ok()).and_then(|n|n.checked_sub(1)).and_then(|i|rows.get(i));
            if let Some(line)=line {
                if line.starts_with('+'){adds+=1;} else if line.starts_with('-'){removals+=1;} else {context+=1;}
                lines.push_str(&format!("{id} {line}\n"));
            }
        }
        let scope=match (adds>0,removals>0) {
            (true,false)=>"adição", (false,true)=>"remoção", (true,true)=>"modificação mista", _=>"somente contexto",
        };
        views.push(format!("Afirmação {} / {}\n{}\n\nTipo das linhas citadas: {scope}\n+{adds} adicionadas | -{removals} removidas | {context} contexto\nReferências do diff, não números de linhas do arquivo.\n\n{lines}\n\nCitações existentes NÃO comprovam a interpretação semântica.",
            idx+1, report["validated_findings"].as_array().map_or(0,Vec::len),
            finding["claim"].as_str().unwrap_or("Afirmação sem texto")));
    }
    Ok(views)
}


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
fn investigate(source: String, dest: String, question: String) -> Result<ReviewResult, String> {
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
    let report:Value=serde_json::from_str(&raw).map_err(|e|e.to_string())?;
    let diff_raw=run_specialist("workspace_diff",&[&source,&dest,"--file",&file,"--hunk",&hunk.to_string(),"--count","1"])?;
    let diff:Value=serde_json::from_str(&diff_raw).map_err(|e|e.to_string())?;
    let evidence=evidence_views(&report,&diff)?;
    Ok(ReviewResult{summary:format!("Pergunta: {question}\nWorkspace: {} ({git_type})\nArquivo: {file} | hunk {hunk}\n\n{}",dest,render_investigation(&raw)), evidence})
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
    let (tx,rx):(mpsc::Sender<Result<ReviewResult,String>>,Receiver<Result<ReviewResult,String>>)=mpsc::channel();
    let mut busy=false;
    let mut evidence: Vec<String>=Vec::new();
    let mut selected:usize=0;
    let mut showing_evidence=false;
    let mut scroll:u16=0;
    loop {
        if let Ok(result)=rx.try_recv() {
            busy=false;status="Pronto".into();
            match result {
                Ok(x)=>{output=x.summary;evidence=x.evidence;selected=0;showing_evidence=false;},
                Err(e)=>{output=format!("Falha observável:\n{e}");evidence.clear();showing_evidence=false;}
            };
            scroll=0;
        }
        t.draw(|f|{
            let areas=Layout::default().direction(Direction::Vertical)
                .constraints([Constraint::Length(3),Constraint::Min(5),Constraint::Length(3),Constraint::Length(2)]).split(f.area());
            let title=Paragraph::new(format!(" DUDE ALPHA 0.1 | Qwen / MLX | {status}"))
                .style(Style::default().fg(Color::Cyan)).block(Block::default().borders(Borders::ALL));
            f.render_widget(title,areas[0]);
            let visible=if showing_evidence {evidence.get(selected).map(String::as_str).unwrap_or(output.as_str())} else {output.as_str()};
            let body=Paragraph::new(visible).wrap(Wrap{trim:false}).scroll((scroll,0))
                .block(Block::default().title(if showing_evidence {" Código citado / evidências " } else {" Investigação / conclusões "}).borders(Borders::ALL));
            f.render_widget(body,areas[1]);
            let prompt=Paragraph::new(format!("> {input}")).block(Block::default().title(" Pedido ").borders(Borders::ALL));
            f.render_widget(prompt,areas[2]);
            f.render_widget(Paragraph::new("Enter envia | V alterna código | Tab próxima conclusão | ↑/↓ rola | Esc sai"),areas[3]);
        }).map_err(|e|e.to_string())?;
        if event::poll(std::time::Duration::from_millis(100)).map_err(|e|e.to_string())? {
            if let Event::Key(key)=event::read().map_err(|e|e.to_string())? {
                if key.kind!=KeyEventKind::Press {continue;}
                match key.code {
                    KeyCode::Esc if !busy => break,
                    KeyCode::Char('v') if !busy && !evidence.is_empty()=>{showing_evidence=!showing_evidence;scroll=0;},
                    KeyCode::Tab if !busy && !evidence.is_empty()=>{selected=(selected+1)%evidence.len();showing_evidence=true;scroll=0;},
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_view_requires_matching_blobs() {
        let report=serde_json::json!({"source_blob":"a","destination_blob":"b",
            "validated_findings":[{"claim":"new function","line_ids":["D2","D3"]}]});
        let diff=serde_json::json!({"source_blob":"a","destination_blob":"b",
            "diff":{"hunks":[{"content":"@@ -0,0 +1,2 @@\\n+fn new() {}\\n+let x = 1;\\n"}]}});
        let views=evidence_views(&report,&diff).unwrap();
        assert_eq!(views.len(),1);
        assert!(views[0].contains("D2 +fn new()"));
        assert!(views[0].contains("2 adicionadas"));
        let changed=serde_json::json!({"source_blob":"changed","destination_blob":"b",
            "diff":{"hunks":[{"content":"@@ -0 +1 @@\\n+changed\\n"}]}});
        assert!(evidence_views(&report,&changed).is_err());
    }
}
