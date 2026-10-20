//! Read-only workspace indicators. Never evaluates shell startup files or project scripts.
use std::{env, path::{Path,PathBuf},process::Command};

#[derive(Clone,Debug,Default)]
pub struct Environment { pub git:String, pub tags:Vec<String>, pub root:PathBuf }

fn git(root:&Path,args:&[&str])->Option<String>{
    let result=Command::new("git").arg("-C").arg(root)
        .args(["-c","core.quotePath=false","-c","color.status=false"])
        .args(args).env("GIT_OPTIONAL_LOCKS","0").output().ok()?;
    if !result.status.success(){return None}
    Some(String::from_utf8_lossy(&result.stdout).trim().chars().take(120).collect())
}
fn exists(root:&Path, relative:&str)->bool {root.join(relative).exists()}
impl Environment {
    pub fn detect(root:&Path)->Self{
        let mut tags=Vec::new();
        let mut git_status=String::new();
        if let Some(branch)=git(root,&["symbolic-ref","--quiet","--short","HEAD"])
            .or_else(||git(root,&["rev-parse","--short","HEAD"]).map(|v|format!("@{v}"))) {
            git_status=format!("🌿 {branch}");
            if let Some(changes)=git(root,&["status","--porcelain","--untracked-files=normal"]) {
                if !changes.is_empty(){git_status.push_str(" ●");}
            }
        }
        let venv=env::var("VIRTUAL_ENV").ok().filter(|v|!v.is_empty());
        let conda=env::var("CONDA_DEFAULT_ENV").ok().filter(|v|!v.is_empty());
        if let Some(name)=venv.as_ref().and_then(|v|Path::new(v).file_name()).map(|v|v.to_string_lossy().to_string()) {
            tags.push(format!("🐍 {name} active"));
        }else if let Some(name)=conda {
            tags.push(format!("🐍 {name} conda"));
        } else if exists(root,".venv") || exists(root,"venv") {
            tags.push("🐍 venv available".into());
        }
        if exists(root,"Cargo.toml"){tags.push("🦀 Rust".into());}
        if exists(root,"pom.xml") || exists(root,"build.gradle") || exists(root,"build.gradle.kts"){tags.push("☕ Java".into());}
        if exists(root,"composer.json"){
            let framework=if exists(root,"artisan"){" Laravel"}else if exists(root,"bin/console"){" Symfony"}else{""};
            tags.push(format!("🐘 PHP{framework}"));
        }
        if exists(root,"package.json"){tags.push("📦 Node".into());}
        if exists(root,"compose.yaml")||exists(root,"compose.yml")||exists(root,"docker-compose.yml")||exists(root,"Dockerfile"){tags.push("🐳 Docker".into());}
        Self{git:git_status,tags,root:root.to_path_buf()}
    }
    pub fn status(&self)->String{
        let dir=self.root.file_name().map(|v|v.to_string_lossy().into_owned()).unwrap_or_else(||self.root.display().to_string());
        let mut parts=vec![format!("📁 {dir}")];
        if !self.git.is_empty(){parts.push(self.git.clone());}
        parts.extend(self.tags.clone());
        parts.join("  |  ")
    }
    pub fn describe(&self)->String{
        format!("Workspace: {}\nGit: {}\nDetected environments: {}\n",self.root.display(),
            if self.git.is_empty(){"not detected"}else{&self.git},
            if self.tags.is_empty(){"none".into()}else{self.tags.join(", ")})
    }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn detects_manifest_without_shell(){
    let path=std::env::temp_dir().join(format!("dude-environment-test-{}",std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("Cargo.toml"),"[package]\nname=\"test\"").unwrap();
    let info=Environment::detect(&path);
    assert!(info.status().contains("Rust"));
    std::fs::remove_dir_all(path).unwrap();
 }
}
