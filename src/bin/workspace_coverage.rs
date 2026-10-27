//! Read-only evidence collector for workspace_diff JSON output.
//! This collects hunks; it does NOT claim an LLM inspected them.
//! Usage: cargo run --bin workspace_coverage -- SOURCE DEST --file REL [--max-hunks N]
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, env, path::Path, process::{Command, ExitCode}};

#[derive(Debug, Deserialize)]
struct DiffPage { result: String, source_blob: Option<String>, destination_blob: Option<String>, diff: Option<DiffData> }
#[derive(Debug, Deserialize)]
struct DiffData {
    total_hunks: usize, next_hunk: Option<usize>, truncated: bool,
    hunks: Vec<Hunk>,
}
#[derive(Debug, Deserialize)]
struct Hunk { id: String, ordinal: usize, header: String, content: String }
#[derive(Debug, Serialize)]
struct Evidence { id: String, ordinal: usize, header: String, bytes: usize, complete: bool }
#[derive(Debug, Serialize)]
struct Coverage {
    status: String, file: String, source_blob: Option<String>, destination_blob: Option<String>,
    total_hunks: usize, collected_hunks: usize, pending_hunks: usize,
    truncated_hunks: usize, model_reviewed_hunks: usize,
    full_collection: bool, eligible_for_resolution: bool,
    evidence: Vec<Evidence>, warnings: Vec<String>,
}
fn run_diff(binary: &Path, src: &str, dst: &str, relative: &str, from: usize, count: usize) -> Result<DiffPage,String> {
    let output=Command::new(binary).args([src,dst,"--file",relative,"--hunk",&from.to_string(),"--count",&count.to_string()])
        .env("GIT_OPTIONAL_LOCKS","0").output().map_err(|e|format!("cannot launch workspace_diff: {e}"))?;
    if !output.status.success(){ return Err(format!("workspace_diff failed: {}",String::from_utf8_lossy(&output.stderr))); }
    serde_json::from_slice(&output.stdout).map_err(|e|format!("invalid diff JSON: {e}"))
}
fn collect(binary:&Path,src:&str,dst:&str,relative:&str,max_hunks:usize)->Result<Coverage,String>{
    let first=run_diff(binary,src,dst,relative,1,1)?;
    let mut result=Coverage {
        status:first.result.clone(),file:relative.into(),
        source_blob:first.source_blob.clone(),destination_blob:first.destination_blob.clone(),
        total_hunks:first.diff.as_ref().map_or(0,|d|d.total_hunks),
        collected_hunks:0,pending_hunks:0,truncated_hunks:0,
        model_reviewed_hunks:0,full_collection:false,eligible_for_resolution:false,
        evidence:vec![],warnings:vec![],
    };
    if first.result!="different" {
        result.full_collection=first.result=="identical";
        result.warnings.push("No divergent text hunks to collect; no automatic resolution".into());
        return Ok(result);
    }
    let mut next=1;
    let mut seen=BTreeSet::new();
    while next<=result.total_hunks && seen.len()<max_hunks {
        let count=(max_hunks-seen.len()).min(8);
        let page=if next==1 && count==1 { run_diff(binary,src,dst,relative,1,1)? } else {run_diff(binary,src,dst,relative,next,count)?};
        if page.result!="different" || page.source_blob!=result.source_blob || page.destination_blob!=result.destination_blob {
            return Err("file changed during collection; stale evidence discarded".into());
        }
        let data=page.diff.ok_or("missing diff data")?;
        if data.total_hunks!=result.total_hunks {return Err("hunk count changed; stale evidence discarded".into());}
        if data.hunks.is_empty(){return Err("empty diff page; stopping".into());}
        if data.truncated {result.truncated_hunks+=1;}
        for h in data.hunks {
            if !seen.insert(h.ordinal) {return Err("duplicate hunk returned".into());}
            if h.id!=format!("hunk-{}",h.ordinal) {return Err("unexpected hunk id".into());}
            result.evidence.push(Evidence{id:h.id,ordinal:h.ordinal,header:h.header,bytes:h.content.len(),complete:!data.truncated});
        }
        next=match data.next_hunk {Some(n) if n>next =>n, None=>result.total_hunks+1,_=>return Err("invalid next_hunk cursor".into())};
    }
    result.collected_hunks=seen.len();
    result.pending_hunks=result.total_hunks.saturating_sub(result.collected_hunks);
    result.full_collection=result.pending_hunks==0 && result.truncated_hunks==0;
    if result.pending_hunks>0 {result.warnings.push("Collection incomplete: more hunks exist".into());}
    if result.truncated_hunks>0 {result.warnings.push("At least one response was truncated; full evidence not available".into());}
    result.warnings.push("Collected means made available to the runtime, NOT read or understood by an LLM".into());
    result.warnings.push("No proposal or file modification is authorized by this report".into());
    Ok(result)
}
fn main()->ExitCode {
    let args:Vec<String>=env::args().skip(1).collect();
    if args.len()<4 || args.get(2).map(String::as_str)!=Some("--file") {
        eprintln!("Usage: workspace_coverage SOURCE DEST --file REL [--max-hunks N]");
        return ExitCode::from(2);
    }
    let max=if args.len()==4 {64} else if args.len()==6 && args[4]=="--max-hunks" {
        match args[5].parse::<usize>(){Ok(n) if (1..=256).contains(&n)=>n,_=>{eprintln!("--max-hunks must be 1..256");return ExitCode::from(2);}}
    } else {eprintln!("invalid arguments");return ExitCode::from(2)};
    let binary=match env::current_exe(){
        Ok(path)=>path.with_file_name(format!("workspace_diff{}",env::consts::EXE_SUFFIX)),
        Err(e)=>{eprintln!("current executable: {e}");return ExitCode::FAILURE;}
    };
    match collect(&binary,&args[0],&args[1],&args[3],max){
        Ok(data)=>match serde_json::to_string_pretty(&data){
            Ok(json)=>{println!("{json}");ExitCode::SUCCESS},
            Err(e)=>{eprintln!("serialization failed: {e}");ExitCode::FAILURE}
        },
        Err(e)=>{eprintln!("Evidence collection failed: {e}");ExitCode::FAILURE}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_does_not_claim_model_review() {
        let report=Coverage {
            status:"different".into(),file:"main.rs".into(),
            source_blob:None,destination_blob:None,total_hunks:48,
            collected_hunks:8,pending_hunks:40,truncated_hunks:0,
            model_reviewed_hunks:0,full_collection:false,
            eligible_for_resolution:false,evidence:vec![],warnings:vec![]
        };
        assert!(!report.full_collection);
        assert_eq!(report.model_reviewed_hunks,0);
        assert!(!report.eligible_for_resolution);
    }
}
