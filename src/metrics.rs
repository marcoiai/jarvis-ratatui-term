//! Non-sensitive request telemetry for comparing Solo and Symphony.
use std::{fs::{self,OpenOptions}, io::Write, path::Path, sync::{Arc,Mutex},time::Instant};
use serde::Serialize;
use crate::evidence::EvidenceCache;

#[derive(Clone,Default)]
pub struct Metrics(Arc<Mutex<Vec<Stage>>>, Arc<Mutex<Vec<TokenUsage>>>);
#[derive(Clone,Serialize)]
pub struct Stage { pub name:String, pub provider:String, pub elapsed_ms:u128, pub success:bool }
#[derive(Clone,Serialize)]
pub struct TokenUsage { pub provider:String, pub model:String, pub input:u64, pub output:u64 }
#[derive(Serialize)]
struct Record<'a> {
    mode: &'a str,
    elapsed_ms:u128,
    success:bool,
    tool_executions:usize,
    evidence_cache_hits:usize,
    evidence_items:usize,
    stages:Vec<Stage>,
    token_usage:Vec<TokenUsage>,
}
impl Metrics {
    pub fn tokens(&self,provider:&str,model:&str,input:u64,output:u64){
        if let Ok(mut values)=self.1.lock(){values.push(TokenUsage{provider:provider.into(),model:model.into(),input,output});}
    }
    pub fn stage(&self,name:&str,provider:&str,start:Instant,success:bool){
        if let Ok(mut stages)=self.0.lock(){stages.push(Stage{name:name.into(),provider:provider.into(),elapsed_ms:start.elapsed().as_millis(),success});}
    }
    pub fn finish(&self,workspace:&Path,mode:&str,start:Instant,success:bool,evidence:&EvidenceCache)->Result<String,String>{
        let stages=self.0.lock().map_err(|e|e.to_string())?.clone();
        let elapsed_ms=start.elapsed().as_millis();
        let usage=self.1.lock().map_err(|e|e.to_string())?.clone();
        let record=Record{mode,elapsed_ms,success,tool_executions:evidence.calls(),evidence_cache_hits:evidence.hits(),evidence_items:evidence.len(),stages:stages.clone(),token_usage:usage.clone()};
        let path=workspace.join(".dude/metrics.jsonl");
        let dir=path.parent().ok_or("Missing metrics directory")?;
        fs::create_dir_all(dir).map_err(|e|e.to_string())?;
        if fs::symlink_metadata(dir).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink metrics directory".into())}
        if path.exists() && fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink metrics file".into())}
        let line=serde_json::to_string(&record).map_err(|e|e.to_string())?;
        writeln!(OpenOptions::new().create(true).append(true).open(path).map_err(|e|e.to_string())?,"{line}").map_err(|e|e.to_string())?;
        let steps=stages.iter().map(|s|format!("{}({}): {}ms{}",s.name,s.provider,s.elapsed_ms,if s.success{""}else{" ERROR"})).collect::<Vec<_>>().join(", ");
        let input:u64=usage.iter().map(|u|u.input).sum();
        let output:u64=usage.iter().map(|u|u.output).sum();
        Ok(format!("DUDE metrics: {mode}, total {elapsed_ms}ms, tool executions {}, cache hits {}, evidence items {}; {}. Reported tokens: input {input}, output {output} ({} measured API responses).",record.tool_executions,record.evidence_cache_hits,record.evidence_items,steps,usage.len()))
    }
}
pub fn recent(workspace:&Path)->String{
    let path=workspace.join(".dude/metrics.jsonl");
    let Ok(content)=fs::read_to_string(path) else {return "No recorded metrics yet.".into()};
    content.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
}
