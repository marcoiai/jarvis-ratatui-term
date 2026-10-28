//! Explicit, local-only SSH-agent credential helper.
//! Never reads private-key contents or accepts passphrases through DUDE/LLM.
//! Usage: cargo run --bin dude_ssh_agent -- status|ensure [identity]
use std::{env, io::{self, IsTerminal, Write}, path::{Path, PathBuf}, process::{Command, ExitCode, Stdio}};

const TTL: &str = "900"; // 15 minutes
const IDENTITIES: &[&str] = &["id_ed25519", "id_rsa", "id_ecdsa"];

fn identity_path(home: &Path, name: &str) -> Result<PathBuf, String> {
    if !IDENTITIES.contains(&name) { return Err("Identity must be one of id_ed25519, id_rsa, id_ecdsa".into()); }
    let dir=home.join(".ssh");
    let meta=std::fs::symlink_metadata(&dir).map_err(|_|"SSH directory unavailable".to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() { return Err("Unsafe SSH directory".into()); }
    let key=dir.join(name);
    let meta=std::fs::symlink_metadata(&key).map_err(|_|"Selected private key is unavailable".to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {return Err("Identity is not a regular private-key file".into());}
    Ok(key)
}

fn agent_status() -> Result<bool, String> {
    if env::var_os("SSH_AUTH_SOCK").is_none() {return Err("SSH_AUTH_SOCK is unset; start/connect ssh-agent first".into());}
    let status=Command::new("ssh-add").arg("-l").stdin(Stdio::null()).stdout(Stdio::null())
        .stderr(Stdio::null()).status().map_err(|e|format!("ssh-add unavailable: {e}"))?;
    match status.code() {
        Some(0)=>Ok(true),
        Some(1)=>Ok(false),
        Some(2)=>Err("Cannot contact ssh-agent".into()),
        _=>Err("ssh-add -l failed".into()),
    }
}

fn ensure(name: &str) -> Result<(), String> {
    let home=PathBuf::from(env::var_os("HOME").ok_or("HOME is unset")?);
    let key=identity_path(&home,name)?;
    let _ = agent_status()?; // checks agent reachability; not proof that selected key is loaded
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("Interactive terminal required for approval and system passphrase prompt".into());
    }
    eprintln!("DUDE SSH: authorize loading identity {} into your ssh-agent for up to 15 minutes? [y/N]",key.display());
    eprint!("> "); io::stderr().flush().map_err(|e|e.to_string())?;
    let mut answer=String::new();
    io::stdin().read_line(&mut answer).map_err(|e|e.to_string())?;
    if !matches!(answer.trim(),"y"|"Y"|"yes"|"YES") {return Err("User denied SSH identity loading".into());}
    // ssh-add inherits terminal handles: the OS/ssh-add requests the passphrase.
    // No key bytes or passphrase pass through Rust, telemetry, or model context.
    let status=Command::new("ssh-add").args(["-t",TTL]).arg(&key)
        .env_remove("SSH_ASKPASS").env_remove("SSH_ASKPASS_REQUIRE")
        .stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
        .status().map_err(|e|format!("ssh-add failed to launch: {e}"))?;
    if !status.success(){return Err("SSH identity was not loaded".into());}
    println!("SSH identity loaded in agent with a 15-minute lifetime. No passphrase was saved by DUDE.");
    Ok(())
}

fn main() -> ExitCode {
    let args:Vec<String>=env::args().skip(1).collect();
    let result=match args.as_slice() {
        [cmd] if cmd=="status" => agent_status().map(|has|{
            println!("{}",if has {"SSH agent accessible; one or more identities are loaded"} else {"SSH agent accessible; no identities loaded"});
        }),
        [cmd] if cmd=="ensure" => ensure("id_ed25519"),
        [cmd,name] if cmd=="ensure" => ensure(name),
        _=>Err("Usage: dude_ssh_agent status | ensure [id_ed25519|id_rsa|id_ecdsa]".into()),
    };
    match result {Ok(())=>ExitCode::SUCCESS,Err(e)=>{eprintln!("{e}");ExitCode::FAILURE}}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_arbitrary_identity_names() {
        let home=Path::new("/tmp");
        for name in ["../key","/tmp/key","id_ed25519.pub","config",""] {
            assert!(identity_path(home,name).is_err());
        }
    }
    #[test]
    fn rejects_symlink_keys() {
        let dir=env::temp_dir().join(format!("dude-ssh-test-{}",std::process::id()));
        let ssh=dir.join(".ssh");
        let _=std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&ssh).unwrap();
        #[cfg(unix)] {
            std::os::unix::fs::symlink("/etc/hosts",ssh.join("id_ed25519")).unwrap();
            assert!(identity_path(&dir,"id_ed25519").is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
