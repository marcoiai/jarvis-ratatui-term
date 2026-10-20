//! Non-sensitive request telemetry for comparing Solo and Symphony.
use std::{fs::{self,OpenOptions}, io::Write, path::Path, sync::{Arc,Mutex},time::Instant};
use serde::Serialize;
use crate::evidence::EvidenceCache;

#[derive(Clone,Default)]
pub struct Metrics(Arc<Mutex<Vec<Stage>>>);
#[derive(Clone,Serialize)]
pub struct Stage { pub name:String, pub provider:String, pub elapsed_ms:u128, pub success:bool }
#[derive(Serialize)]
struct Record<'a> {
    mode: &'a str,
    elapsed_ms:u128,
    success:bool,
    tool_executions:usize,
    evidence_cache_hits:usize,
    evidence_items:usize,
    stages:Vec<Stage>,
    token_counts_available:bool,
}
impl Metrics {
    pub fn stage(&self,name:&str,provider:&str,start:Instant,success:bool){
        if let Ok(mut stages)=self.0.lock(){stages.push(Stage{name:name.into(),provider:provider.into(),elapsed_ms:start.elapsed().as_millis(),success});}
    }
    pub fn finish(&self,workspace:&Path,mode:&str,start:Instant,success:bool,evidence:&EvidenceCache)->Result<String,String>{
        let stages=self.0.lock().map_err(|e|e.to_string())?.clone();
        let elapsed_ms=start.elapsed().as_millis();
        let record=Record{mode,elapsed_ms,success,tool_executions:evidence.calls(),evidence_cache_hits:evidence.hits(),evidence_items:evidence.len(),stages:stages.clone(),token_counts_available:false};
        let path=workspace.join(".dude/metrics.jsonl");
        let dir=path.parent().ok_or("Missing metrics directory")?;
        fs::create_dir_all(dir).map_err(|e|e.to_string())?;
        if fs::symlink_metadata(dir).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink metrics directory".into())}
        if path.exists() && fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink metrics file".into())}
        let line=serde_json::to_string(&record).map_err(|e|e.to_string())?;
        writeln!(OpenOptions::new().create(true).append(true).open(path).map_err(|e|e.to_string())?,"{line}").map_err(|e|e.to_string())?;
        let steps=stages.iter().map(|s|format!("{}({}): {}ms{}",s.name,s.provider,s.elapsed_ms,if s.success{""}else{" ERROR"})).collect::<Vec<_>>().join(", ");
        Ok(format!("DUDE metrics: {mode}, total {elapsed_ms}ms, tool executions {}, cache hits {}, evidence items {}; {}. Token usage: not instrumented.",record.tool_executions,record.evidence_cache_hits,record.evidence_items,steps))
    }
}
pub fn recent(workspace:&Path)->String{
    let path=workspace.join(".dude/metrics.jsonl");
    let Ok(content)=fs::read_to_string(path) else {return "No recorded metrics yet.".into()};
    content.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
}
