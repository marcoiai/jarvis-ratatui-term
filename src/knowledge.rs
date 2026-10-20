//! Reviewable, workspace-scoped knowledge candidates. No automatic promotion.
use std::{fs, io::Write, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u64,
    pub status: String,
    pub classification: String,
    pub claim: String,
    pub evidence: Vec<String>,
    pub question: String,
}
fn file(workspace:&Path)->PathBuf {workspace.join(".dude/knowledge_candidates.json")}
fn load(workspace:&Path)->Result<Vec<Candidate>,String>{
    let path=file(workspace);
    if !path.exists(){return Ok(Vec::new())}
    if fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink candidate file".into())}
    let data=fs::read(path).map_err(|e|e.to_string())?;
    if data.len()>1024*1024 {return Err("Candidate file too large".into())}
    serde_json::from_slice(&data).map_err(|e|e.to_string())
}
fn save(workspace:&Path, entries:&[Candidate])->Result<(),String>{
    let dir=workspace.join(".dude");
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    if fs::symlink_metadata(&dir).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink memory directory".into())}
    let target=file(workspace);
    if target.exists() && fs::symlink_metadata(&target).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Refusing symlink candidate file".into())}
    let tmp=dir.join(format!(".candidates-{}.tmp",std::process::id()));
    let data=serde_json::to_vec_pretty(entries).map_err(|e|e.to_string())?;
    let result=(||->Result<(),String>{
        let mut f=fs::OpenOptions::new().create_new(true).write(true).open(&tmp).map_err(|e|e.to_string())?;
        f.write_all(&data).map_err(|e|e.to_string())?;
        f.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&tmp,&target).map_err(|e|e.to_string())
    })();
    if result.is_err(){let _=fs::remove_file(tmp);}
    result
}
pub fn propose(workspace:&Path,question:&str,claims:&[CandidateDraft])->Result<Vec<u64>,String>{
    let mut entries=load(workspace)?;
    let mut ids=Vec::new();
    for c in claims.iter().take(12){
        if c.claim.trim().is_empty() || c.claim.len()>1600 || !matches!(c.classification.as_str(),"documented"|"verified"|"inferred") {continue}
        if entries.iter().any(|e|e.claim==c.claim && e.status!="rejected"){continue}
        let next=entries.iter().map(|e|e.id).max().unwrap_or(0)+1;
        entries.push(Candidate{id:next,status:"pending".into(),classification:c.classification.clone(),claim:c.claim.trim().into(),evidence:c.evidence.iter().take(5).map(|v|v.chars().take(500).collect()).collect(),question:question.chars().take(1000).collect()});
        ids.push(next);
    }
    if !ids.is_empty(){save(workspace,&entries)?;}
    Ok(ids)
}
#[derive(Clone,Debug,Deserialize)]
pub struct CandidateDraft{
    pub classification:String,
    pub claim:String,
    #[serde(default)]
    pub evidence:Vec<String>,
}
pub fn list(workspace:&Path)->Result<String,String>{
    let entries=load(workspace)?;
    let pending:Vec<_>=entries.iter().filter(|e|e.status=="pending").collect();
    if pending.is_empty(){return Ok("No pending knowledge candidates.\n".into())}
    Ok(pending.iter().map(|e|format!("#{} [{}] {}\n  Evidence: {}\n",e.id,e.classification,e.claim,e.evidence.join("; "))).collect::<Vec<_>>().join("\n"))
}
pub fn approve(workspace:&Path,id:u64,memory:&crate::memory::Memory)->Result<(),String>{
    let mut entries=load(workspace)?;
    let item=entries.iter_mut().find(|e|e.id==id && e.status=="pending").ok_or("Candidate not found or already reviewed")?;
    if item.classification!="verified" && item.classification!="documented" {return Err("Only documented or verified claims can be approved as facts; inferred claims require verification".into())}
    if item.evidence.is_empty(){return Err("Cannot approve a fact without evidence references".into())}
    memory.remember(&format!("[{}] {} | Sources: {}",item.classification,item.claim,item.evidence.join("; ")))?;
    item.status="approved".into();
    save(workspace,&entries)
}
pub fn reject(workspace:&Path,id:u64)->Result<(),String>{
    let mut entries=load(workspace)?;
    let item=entries.iter_mut().find(|e|e.id==id && e.status=="pending").ok_or("Candidate not found or already reviewed")?;
    item.status="rejected".into();
    save(workspace,&entries)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn draft_parsing(){
        let draft:CandidateDraft=serde_json::from_str(r#"{"classification":"documented","claim":"A","evidence":["README.md:1"]}"#).unwrap();
        assert_eq!(draft.evidence[0],"README.md:1");
    }
}
