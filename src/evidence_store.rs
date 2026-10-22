//! M1.5: explicit, bounded, workspace-scoped persistence of source excerpts.
//! Entries are observations, NOT semantic verification nor model training.
use std::{fs, io::{BufRead, BufReader, Write}, path::{Path, Component}};
use serde::{Deserialize, Serialize};
use crate::evidence::EvidenceCache;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Excerpt {
    pub path: String,
    pub excerpt: String,
    pub recorded_unix: u64,
    pub source_size: u64,
    pub source_modified_ns: u128,
    pub question: String,
}

fn folder(workspace:&Path)->Result<std::path::PathBuf,String> {
    let dir=workspace.join(".dude");
    if dir.exists() && fs::symlink_metadata(&dir).map_err(|e|e.to_string())?.file_type().is_symlink() {
        return Err("Refusing symlink evidence directory".into())
    }
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    Ok(dir)
}
fn destination(workspace:&Path)->Result<std::path::PathBuf,String> {
    let path=folder(workspace)?.join("source_evidence.jsonl");
    if path.exists() && fs::symlink_metadata(&path).map_err(|e|e.to_string())?.file_type().is_symlink() {
        return Err("Refusing symlink evidence file".into())
    }
    Ok(path)
}
fn checked_source(workspace:&Path,path:&str)->Option<std::path::PathBuf> {
    let relative=Path::new(path);
    if relative.is_absolute() || relative.components().any(|c| !matches!(c,Component::Normal(_))) {return None}
    if relative.components().any(|c|c.as_os_str().to_string_lossy().starts_with('.')) {return None}
    // Conservative filter: store source-code excerpts only. A source file can
    // still contain credentials; review the local store before exporting.
    let allowed=["rs","py","java","cpp","cc","c","h","hpp","ts","tsx","js","jsx","php","go","kt","swift","toml","md"];
    if !relative.extension().and_then(|v|v.to_str()).is_some_and(|ext|allowed.contains(&ext.to_ascii_lowercase().as_str())) {return None}
    let forbidden=["secret","credential","password","token","private","keyfile"];
    if relative.components().any(|c|{
        let component=c.as_os_str().to_string_lossy().to_lowercase();
        forbidden.iter().any(|part|component.contains(part))
    }) {return None}
    let source=workspace.join(relative);
    if !source.symlink_metadata().ok()?.is_file() || source.symlink_metadata().ok()?.file_type().is_symlink() {return None}
    let canonical=source.canonicalize().ok()?;
    if !canonical.starts_with(workspace.canonicalize().ok()?) {return None}
    Some(source)
}
fn stamp(path:&Path)->Option<(u64,u128)> {
    let m=fs::metadata(path).ok()?;
    let nanos=m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some((m.len(),nanos))
}
fn read_all(path:&Path)->Result<Vec<Excerpt>,String>{
    if !path.exists(){return Ok(Vec::new())}
    if fs::metadata(path).map_err(|e|e.to_string())?.len()>3_000_000 {return Err("Evidence file exceeds 3MB".into())}
    let file=fs::File::open(path).map_err(|e|e.to_string())?;
    BufReader::new(file).lines().take(4000).map(|line| {
        let line=line.map_err(|e|e.to_string())?;
        serde_json::from_str::<Excerpt>(&line).map_err(|e|e.to_string())
    }).collect()
}
pub fn capture(workspace:&Path,question:&str,evidence:&EvidenceCache)->Result<usize,String> {
    if question.len()>10000 {return Err("Question too large".into())}
    let path=destination(workspace)?;
    let mut existing=read_all(&path)?;
    let mut saved=0usize;
    for item in evidence.source_observations().into_iter().take(32) {
        let Some(name)=item["arguments"]["path"].as_str() else {continue};
        let Some(source)=checked_source(workspace,name) else {continue};
        let Some((source_size,source_modified_ns))=stamp(&source) else {continue};
        let Some(snippet)=item["result"].as_str() else {continue};
        if snippet.is_empty() || snippet.len()>16000 {continue}
        if existing.iter().any(|e|e.path==name && e.excerpt==snippet && e.source_size==source_size && e.source_modified_ns==source_modified_ns) {continue}
        existing.push(Excerpt {
            path:name.to_owned(),excerpt:snippet.to_owned(),
            recorded_unix:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            source_size,source_modified_ns,question:question.chars().take(500).collect(),
        });
        saved+=1;
    }
    if saved==0 {return Ok(0)}
    // Atomic rewrite, capped on disk; old records trimmed first.
    while existing.len()>200 {existing.remove(0);}
    let mut data=Vec::new();
    for entry in &existing {
        serde_json::to_writer(&mut data,entry).map_err(|e|e.to_string())?;
        data.push(b'\n');
    }
    if data.len()>2_500_000 {return Err("Evidence size limit exceeded; no data written".into())}
    let dir=folder(workspace)?;
    let temp=dir.join(format!(".source-evidence-{}.tmp",std::process::id()));
    let write=(||->Result<(),String>{
        let mut f=fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
        f.write_all(&data).map_err(|e|e.to_string())?;
        f.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&temp,&path).map_err(|e|e.to_string())
    })();
    if write.is_err(){let _=fs::remove_file(&temp);}
    write.map(|_|saved)
}
pub fn status(workspace:&Path)->Result<String,String>{
    let entries=read_all(&destination(workspace)?)?;
    let mut fresh=0usize;
    for e in &entries {
        if checked_source(workspace,&e.path).as_deref().and_then(stamp)==Some((e.source_size,e.source_modified_ns)) {
            fresh+=1;
        }
    }
    Ok(format!("Evidence Store: {} trechos persistidos; {} atuais; {} desatualizados/indisponíveis.\nArmazenamento: .dude/source_evidence.jsonl\nTrechos são observações, não fatos verificados.\n",entries.len(),fresh,entries.len()-fresh))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn stores_and_invalidates_observations(){
        let root=std::env::temp_dir().join(format!("dude-store-{}",std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("example.rs"),"first\nsecond\n").unwrap();
        let evidence=EvidenceCache::default();
        evidence.execute(&root,"read_file",&json!({"path":"example.rs"}));
        assert_eq!(capture(&root,"explique",&evidence).unwrap(),1);
        assert_eq!(capture(&root,"explique",&evidence).unwrap(),0);
        assert!(status(&root).unwrap().contains("1 atuais"));
        fs::write(root.join("example.rs"),"changed content\n").unwrap();
        assert!(status(&root).unwrap().contains("1 desatualizados"));
        fs::remove_dir_all(root).unwrap();
    }
}
