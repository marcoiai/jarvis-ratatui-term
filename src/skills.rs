//! Read-only SKILL.md compatibility layer. Skill text is advisory, never authority.
use std::{env, fs, path::{Path,PathBuf}};
#[derive(Clone,Debug)]
pub struct Skill {pub name:String,pub description:String,pub file:PathBuf}
fn roots(workspace:&Path)->Vec<PathBuf>{
    let mut roots=vec![workspace.join(".dude/skills")];
    if let Ok(home)=env::var("HOME"){
        let home=PathBuf::from(home);
        roots.push(home.join(".config/dude/skills"));
        roots.push(home.join(".codex/skills"));
    }
    roots
}
fn safe_name(name:&str)->bool{
    !name.is_empty() && name.len()<=80 && name.chars().all(|c|c.is_ascii_alphanumeric()||matches!(c,'-'|'_'))
}
pub fn discover(workspace:&Path)->Vec<Skill>{
    let mut skills=std::collections::BTreeMap::new();
    for root in roots(workspace){
        let Ok(dir)=fs::read_dir(root) else {continue};
        for entry in dir.flatten().take(128){
            let name=entry.file_name().to_string_lossy().to_string();
            if !safe_name(&name) || !entry.file_type().is_ok_and(|t|t.is_dir()&&!t.is_symlink()){continue}
            let file=entry.path().join("SKILL.md");
            let Ok(meta)=fs::symlink_metadata(&file) else {continue};
            if !meta.is_file()||meta.file_type().is_symlink()||meta.len()>32_768{continue}
            let Ok(content)=fs::read_to_string(&file) else {continue};
            let description=content.lines().take(30).find_map(|line|
                line.trim().strip_prefix("description:").map(|s|s.trim().trim_matches('"').to_string()))
                .unwrap_or_else(||"No description".into());
            skills.entry(name.clone()).or_insert(Skill{name,description,file});
        }
    }
    skills.into_values().collect()
}
pub fn list(workspace:&Path)->String{
    let skills=discover(workspace);
    if skills.is_empty(){return "Nenhuma skill encontrada em .dude/skills, ~/.config/dude/skills ou ~/.codex/skills.\n".into()}
    let mut result=String::from("Skills disponíveis (use /skill NOME):\n");
    for skill in skills {result.push_str(&format!("- {} — {}\n",skill.name,skill.description));}
    result
}
pub fn content(workspace:&Path,name:&str)->Result<String,String>{
    if !safe_name(name){return Err("Nome de skill inválido".into())}
    let skill=discover(workspace).into_iter().find(|s|s.name==name).ok_or("Skill não encontrada")?;
    fs::read_to_string(skill.file).map_err(|e|e.to_string())
}
pub fn automatic(workspace:&Path,query:&str)->Option<String>{
    let lower=query.to_lowercase();
    let selected=if ["brainstorm","ideias","explorar opções","explorar opcoes"].iter().any(|w|lower.contains(w)){
        "marco-brainstorming"
    } else {return None};
    discover(workspace).iter().any(|s|s.name==selected).then(||selected.to_owned())
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn names_are_confined(){
 assert!(safe_name("marco-brainstorming"));
 assert!(!safe_name("../secret"));
 assert!(!safe_name("/etc/passwd"));
 }
}
