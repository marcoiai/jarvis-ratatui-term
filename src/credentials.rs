use std::{env, fs, path::PathBuf};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

fn dir() -> Result<PathBuf,String> {
    let home=env::var("HOME").map_err(|_|"HOME unavailable".to_string())?;
    Ok(PathBuf::from(home).join(".config/dude"))
}
fn path(provider:&str)->Result<PathBuf,String>{
    let name=match provider {"gemini"=>"gemini.key","groq"=>"groq.key",_=>return Err("Unsupported key provider".into())};
    Ok(dir()?.join(name))
}
pub fn get(provider:&str)->Option<String>{
    let env_key=match provider {"gemini"=>env::var("GEMINI_API_KEY").or_else(|_|env::var("GOOGLE_API_KEY")).ok(),"groq"=>env::var("GROQ_API_KEY").ok(),_=>None};
    if env_key.as_ref().is_some_and(|s| !s.is_empty()){return env_key;}
    let p=path(provider).ok()?;
    let meta=fs::symlink_metadata(&p).ok()?;
    if !meta.is_file() || meta.file_type().is_symlink(){return None;}
    #[cfg(unix)]
    if meta.permissions().mode() & 0o077 != 0{return None;}
    fs::read_to_string(p).ok().map(|s|s.trim().to_string()).filter(|s|!s.is_empty())
}
pub fn save(provider:&str,key:&str)->Result<(),String>{
    let value=key.trim();
    if value.is_empty() || value.len()>8192 || value.chars().any(char::is_control){
        return Err("Invalid key (empty, too long or control character)".into());
    }
    let folder=dir()?;
    fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    #[cfg(unix)]
    fs::set_permissions(&folder,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
    let target=path(provider)?;
    if let Ok(meta)=fs::symlink_metadata(&target){
        if meta.file_type().is_symlink(){return Err("Refusing symlink credential target".into());}
    }
    let tmp=folder.join(format!(".{}-{}.tmp",provider,std::process::id()));
    let result=(||->Result<(),String>{
        let mut options=fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut f=options.open(&tmp).map_err(|e|e.to_string())?;
        use std::io::Write;
        f.write_all(value.as_bytes()).map_err(|e|e.to_string())?;
        f.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&tmp,&target).map_err(|e|e.to_string())
    })();
    if result.is_err(){let _=fs::remove_file(tmp);}
    result
}
pub fn clear(provider:&str)->Result<(),String>{
    let target=path(provider)?;
    match fs::remove_file(target){
        Ok(())=>Ok(()),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),
        Err(e)=>Err(e.to_string())
    }
}
