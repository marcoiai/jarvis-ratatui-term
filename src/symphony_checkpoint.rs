//! Stage checkpoint only. No unsafe automatic replay or implicit cloud requests.
use std::{fs,io::Write,path::Path};
use serde_json::json;
pub fn save(workspace:&Path,question:&str,stage:&str,provider:&str,model:&str,
    response:&str,coverage:&str)->Result<(),String>{
    let dir=workspace.join(".dude");
    if dir.exists() && fs::symlink_metadata(&dir).map_err(|e|e.to_string())?.file_type().is_symlink(){
        return Err("Checkpoint directory symlink".into())
    }
    fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let target=dir.join("symphony_checkpoint.json");
    if target.exists() && fs::symlink_metadata(&target).map_err(|e|e.to_string())?.file_type().is_symlink(){
        return Err("Checkpoint target symlink".into())
    }
    let data=serde_json::to_vec_pretty(&json!({
        "stage":stage,"provider":provider,"model":model,
        "question":question.chars().take(2000).collect::<String>(),
        "response":response.chars().take(14000).collect::<String>(),
        "coverage":coverage,
        "note":"Historical checkpoint; not validated for automatic resume."
    })).map_err(|e|e.to_string())?;
    let temp=dir.join(format!(".symphony-{}.tmp",std::process::id()));
    let result=(||->Result<(),String>{
        let mut file=fs::OpenOptions::new().create_new(true).write(true).open(&temp).map_err(|e|e.to_string())?;
        file.write_all(&data).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())?;
        fs::rename(&temp,&target).map_err(|e|e.to_string())
    })();
    if result.is_err(){let _=fs::remove_file(&temp);}
    result
}
