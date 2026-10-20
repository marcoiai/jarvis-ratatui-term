//! General communication alignment: user profile, per-turn override and explicit updates.
//! This is a reusable pattern, not one developer's hard-coded personality.
use std::{env,fs,io::Write,path::PathBuf};
use serde::{Serialize,Deserialize};

#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct Profile {
    pub language:String,
    pub detail:String,
    pub pacing:String,
}
impl Default for Profile {
    fn default()->Self{Self{language:"auto".into(),detail:"concise".into(),pacing:"adaptive".into()}}
}
fn path()->Result<PathBuf,String>{
    let home=env::var_os("HOME").ok_or("HOME missing")?;
    Ok(PathBuf::from(home).join(".config/dude/communication.json"))
}
impl Profile {
    pub fn load()->Self{
        let Ok(p)=path() else{return Self::default()};
        if p.symlink_metadata().is_ok_and(|m|m.file_type().is_symlink()){return Self::default()}
        fs::read(&p).ok().filter(|d|d.len()<=8192).and_then(|d|serde_json::from_slice(&d).ok()).unwrap_or_default()
    }
    pub fn save(&self)->Result<(),String>{
        let p=path()?;
        let dir=p.parent().ok_or("Invalid profile path")?;
        fs::create_dir_all(dir).map_err(|e|e.to_string())?;
        if fs::symlink_metadata(dir).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Profile directory is a symlink".into())}
        if p.exists() && fs::symlink_metadata(&p).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Profile file is a symlink".into())}
        let tmp=dir.join(format!(".communication-{}.tmp",std::process::id()));
        let result=(||->Result<(),String>{
            let mut f=fs::OpenOptions::new().create_new(true).write(true).open(&tmp).map_err(|e|e.to_string())?;
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                f.set_permissions(fs::Permissions::from_mode(0o600)).map_err(|e|e.to_string())?;
            }
            f.write_all(&serde_json::to_vec_pretty(self).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            f.sync_all().map_err(|e|e.to_string())?;
            fs::rename(&tmp,&p).map_err(|e|e.to_string())
        })();
        if result.is_err(){let _=fs::remove_file(tmp);}
        result
    }
    pub fn instructions(&self)->String{
        format!("Communication alignment (user-owned, not project knowledge): language={}; detail={}; pacing={}. Follow the latest explicit user instruction for the current answer. Adapt answer length to task difficulty and safety. A social utterance is not automatically a software change request. Do not reinterpret a correction of style or language as a request to re-investigate source files.",self.language,self.detail,self.pacing)
    }
    pub fn describe(&self)->String{format!("Language: {} | Detail: {} | Pacing: {}",self.language,self.detail,self.pacing)}
    pub fn set(&mut self,key:&str,value:&str)->Result<(),String>{
        match (key,value){
            ("language","auto"|"pt-BR"|"en")=>self.language=value.into(),
            ("detail","concise"|"balanced"|"detailed")=>self.detail=value.into(),
            ("pacing","adaptive"|"step")=>self.pacing=value.into(),
            _=>return Err("Use /align language auto|pt-BR|en; /align detail concise|balanced|detailed; /align pacing adaptive|step".into())
        }
        Ok(())
    }
}
// Simple explicit preference statements are handled without any LLM request.
pub fn explicit_alignment(input:&str)->Option<(&'static str,&'static str)>{
    let normalized=input.trim().trim_end_matches(['.', '!', '?']).trim()
        .to_lowercase().replace("ê","e").replace("é","e");
    let phrase=normalized.strip_suffix(" por favor")
        .or_else(||normalized.strip_suffix(" please"))
        .unwrap_or(&normalized).trim();
    // Match a small intent grammar, not a list of every accented/polite variant.
    // Only exact standalone preference commands are persisted; ordinary questions
    // containing language names should still reach the conversation.
    if ["em portugues","fale em portugues","responda em portugues",
        "pode falar em portugues","por favor fale em portugues",
        "prefiro portugues"].contains(&phrase) {
        return Some(("language","pt-BR"));
    }
    if ["in english","respond in english","speak in english",
        "please speak in english","please respond in english"].contains(&phrase){
        return Some(("language","en"));
    }
    match phrase {
        "seja mais objetivo"|"mais objetivo"|"respostas mais curtas"|"seja mais breve"=>Some(("detail","concise")),
        "mais detalhes"|"explique com mais detalhes"=>Some(("detail","detailed")),
        "passo a passo"|"vamos passo a passo"=>Some(("pacing","step")),
        _=>None,
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn different_people_same_pattern(){
        assert_eq!(explicit_alignment("Em português por favor"),Some(("language","pt-BR")));
        assert_eq!(explicit_alignment("em portugues"),Some(("language","pt-BR")));
        assert_eq!(explicit_alignment("em portugues!"),Some(("language","pt-BR")));
        assert_eq!(explicit_alignment("fale em portugues por favor"),Some(("language","pt-BR")));
        assert_eq!(explicit_alignment("pode falar em português?"),Some(("language","pt-BR")));
        assert!(explicit_alignment("explique como configurar o idioma portugues").is_none());
        assert_eq!(explicit_alignment("in english please"),Some(("language","en")));
        assert_eq!(explicit_alignment("respond in english please"),Some(("language","en")));
        assert!(explicit_alignment("analise o projeto").is_none());
    }
    #[test] fn validates_preferences(){
        let mut p=Profile::default();
        assert!(p.set("language","pt-BR").is_ok());
        assert!(p.set("language","rm -rf").is_err());
    }
}
