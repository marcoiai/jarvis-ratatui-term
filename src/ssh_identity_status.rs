//! Read-only status of a selected SSH identity for DUDE's TUI.
//! Does not read private key bytes, launch ssh-add -t or prompt for passphrases.
use std::{env, path::PathBuf, process::{Command,Stdio}};

pub fn status(name: &str) -> Result<&'static str, String> {
    if !matches!(name, "id_ed25519" | "id_rsa" | "id_ecdsa") {
        return Err("Usage: /ssh status [id_ed25519|id_rsa|id_ecdsa]".into());
    }
    let home=PathBuf::from(env::var_os("HOME").ok_or("HOME is unset")?);
    let dir=home.join(".ssh");
    let dir_meta=std::fs::symlink_metadata(&dir).map_err(|_|"SSH directory unavailable".to_string())?;
    if !dir_meta.is_dir() || dir_meta.file_type().is_symlink() {return Err("Unsafe SSH directory".into());}
    let key=dir.join(name);
    let key_meta=std::fs::symlink_metadata(&key).map_err(|_|"SSH identity unavailable".to_string())?;
    if !key_meta.is_file() || key_meta.file_type().is_symlink() {return Err("Unsafe SSH identity".into());}
    let pubkey=dir.join(format!("{name}.pub"));
    let pub_meta=std::fs::symlink_metadata(&pubkey).map_err(|_|"Public key companion unavailable".to_string())?;
    if !pub_meta.is_file() || pub_meta.file_type().is_symlink() {return Err("Unsafe public key companion".into());}
    let public=std::fs::read_to_string(&pubkey).map_err(|e|format!("Cannot read public key: {e}"))?;
    let expected=parts(public.trim()).ok_or("Invalid public key companion")?;
    if env::var_os("SSH_AUTH_SOCK").is_none(){return Err("SSH_AUTH_SOCK is unset".into());}
    let output=Command::new("ssh-add").arg("-L").stdin(Stdio::null()).output()
        .map_err(|e|format!("ssh-add unavailable: {e}"))?;
    match output.status.code() {
        Some(0)=>Ok(if String::from_utf8_lossy(&output.stdout).lines()
            .filter_map(parts).any(|item| item==expected) {
                "Requested SSH identity is loaded."
            } else { "Requested SSH identity is not loaded." }),
        Some(1)=>Ok("Requested SSH identity is not loaded."),
        _=>Err("Cannot query ssh-agent".into()),
    }
}
fn parts(line: &str) -> Option<(&str,&str)> {
    let mut words=line.split_whitespace();
    let kind=words.next()?;
    let body=words.next()?;
    if !(kind.starts_with("ssh-") || kind.starts_with("ecdsa-")) || body.is_empty() {return None;}
    Some((kind,body))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unapproved_key_paths_without_io() {
        for name in ["../id_ed25519","/tmp/key","id_ed25519.pub",""] {
            assert!(status(name).is_err());
        }
    }
    #[test]
    fn matches_public_key_ignoring_comment() {
        assert_eq!(parts("ssh-ed25519 AAAA first"),parts("ssh-ed25519 AAAA second"));
    }
}
