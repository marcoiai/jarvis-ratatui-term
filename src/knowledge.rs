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
    #[serde(default)]
    pub references_checked: bool,
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
fn useful_claim(claim: &str) -> bool {
    let text = claim.trim();
    let lower = text.to_lowercase();
    if text.len() < 25 || lower.starts_with("source declaration in ") {
        return false;
    }
    !["pub fn ", "pub async fn ", "pub struct ", "pub enum ", "pub trait ",
        "public class ", "public interface ", "class ", "def ", "async def "]
        .iter().any(|prefix| lower.starts_with(prefix))
}

pub fn propose(workspace:&Path,question:&str,claims:&[CandidateDraft], observations:&crate::evidence::EvidenceCache)->Result<Vec<u64>,String>{
    let mut entries=load(workspace)?;
    let mut ids=Vec::new();
    for c in claims.iter().take(12){
        if !useful_claim(&c.claim) || c.claim.len()>1600 || !matches!(c.classification.as_str(),"documented"|"verified"|"inferred") {continue}
        if entries.iter().any(|e|e.claim==c.claim && e.status!="rejected"){continue}
        let next=entries.iter().map(|e|e.id).max().unwrap_or(0)+1;
        let references: Vec<String> = c.evidence.iter().take(5)
            .map(|v|v.chars().take(500).collect()).collect();
        let checked = !references.is_empty()
            && references.iter().all(|reference| observations.observed_reference(reference));
        if !checked { continue; }
        let classification = c.classification.clone();
        entries.push(Candidate{id:next,status:"pending".into(),classification,
            claim:c.claim.trim().into(),evidence:references,question:question.chars().take(1000).collect(),
            references_checked:checked});
        ids.push(next);
    }
    if !ids.is_empty(){save(workspace,&entries)?;}
    Ok(ids)
}
#[derive(Clone,Debug,Deserialize)]
pub struct CandidateDraft{
    pub classification:String,
    pub claim:String,
    #[serde(default, deserialize_with = "decode_evidence_refs")]
    pub evidence:Vec<String>,
}
fn decode_evidence_refs<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where D: serde::Deserializer<'de> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Refs { Single(String), Multiple(Vec<String>) }
    match Refs::deserialize(deserializer)? {
        Refs::Single(value) => Ok(vec![value]),
        Refs::Multiple(values) => Ok(values),
    }
}
/// Return the oldest reviewable candidate so the UI can request consent.
pub fn next_pending(workspace:&Path)->Result<Option<Candidate>,String>{
    Ok(load(workspace)?.into_iter().find(|c|c.status=="pending"))
}
pub fn list(workspace:&Path)->Result<String,String>{
    let entries=load(workspace)?;
    let pending:Vec<_>=entries.iter().filter(|e|e.status=="pending").collect();
    let approved=entries.iter().filter(|e|e.status=="approved").count();
    let rejected=entries.iter().filter(|e|e.status=="rejected").count();
    let blocked=pending.iter().filter(|e|!e.references_checked || e.classification=="inferred").count();
    let memory_path=workspace.join(".dude/memory.md");
    let retained=if memory_path.exists() {
        if fs::symlink_metadata(&memory_path).map_err(|e|e.to_string())?.file_type().is_symlink(){
            return Err("Refusing symlink memory file".into());
        }
        let source=fs::read_to_string(&memory_path).map_err(|e|e.to_string())?;
        source.lines().filter(|line|line.starts_with("- ")).count()
    } else {0};
    let mut report=format!(
        "Knowledge: {} pending ({} unverified), {} approved, {} rejected; {} retained memory entries.\n",
        pending.len(), blocked, approved, rejected, retained
    );
    if pending.is_empty(){
        report.push_str("No pending candidates. Automatic extraction is best-effort; check the last analysis warning.\n");
    } else {
        for item in pending {
            report.push_str(&format!(
                "#{} [{}{}] {}\n  Evidence: {}\n  Review: {}\n",
                item.id,item.classification,
                if item.references_checked {""} else {", references unverified"},
                item.claim,
                if item.evidence.is_empty() {"(none)".to_owned()} else {item.evidence.join("; ")},
                if item.references_checked {"line reference observed; claim not automatically validated"}
                else {"no matching source line observed"}
            ));
        }
    }
    report.push_str("Approved memory: .dude/memory.md | Candidate queue: .dude/knowledge_candidates.json\n");
    Ok(report)
}
pub fn approve(workspace:&Path,id:u64,memory:&crate::memory::Memory)->Result<(),String>{
    let mut entries=load(workspace)?;
    let item=entries.iter_mut().find(|e|e.id==id && e.status=="pending").ok_or("Candidate not found or already reviewed")?;
    if item.classification!="verified" && item.classification!="documented" {return Err("Only documented or verified claims can be approved as facts; inferred claims require verification".into())}
    if item.evidence.is_empty() || !item.references_checked {return Err("Cannot approve a fact without source lines verified from tool output".into())}
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
    fn rejects_source_declarations() {
        assert!(!useful_claim("Source declaration in src/router.rs: pub enum Route {"));
        assert!(!useful_claim("pub fn route(input: &str) -> Route {"));
        assert!(useful_claim("The application requires explicit approval before persisting knowledge."));
    }

    #[test]
    fn draft_parsing(){
        let draft:CandidateDraft=serde_json::from_str(r#"{"classification":"documented","claim":"A","evidence":["README.md:1"]}"#).unwrap();
        assert_eq!(draft.evidence[0],"README.md:1");
        let scalar: CandidateDraft = serde_json::from_str(
            r#"{"classification":"verified","claim":"A","evidence":"src/interpreter.rs:6-14"}"#
        ).unwrap();
        assert_eq!(scalar.evidence, vec!["src/interpreter.rs:6-14"]);
        assert!(serde_json::from_str::<CandidateDraft>(
            r#"{"classification":"verified","claim":"A","evidence":17}"#
        ).is_err());
    }
}
