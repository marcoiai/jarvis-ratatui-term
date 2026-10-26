//! Explicit, local UTF-8 exports of the text visible in the DUDE TUI.
//! This is a transcript, not curated memory or model conversation context.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;

fn ensure_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!("Recusando diretório simbólico: {}", path.display()));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!("Não é um diretório: {}", path.display()));
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    fs::create_dir(path).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn export_directory(workspace: &Path) -> Result<PathBuf, String> {
    let memory_dir = workspace.join(".dude");
    ensure_directory(&memory_dir)?;
    let dir = memory_dir.join("exports");
    ensure_directory(&dir)?;
    Ok(dir)
}

fn write_new(path: &Path, transcript: &str) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(transcript.as_bytes()).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(path);
        return Err(error.to_string());
    }
    Ok(())
}

/// Persist the most recently completed TUI transcript so it can be shared
/// without selecting and copying thousands of characters in Ratatui.
pub fn save_latest(workspace: &Path, transcript: &str) -> Result<PathBuf, String> {
    if transcript.len() > MAX_EXPORT_BYTES {
        return Err("O transcript ultrapassou 8 MiB; use /clear para iniciar outra sessão.".into());
    }
    let dir = export_directory(workspace)?;
    let destination = dir.join("latest.txt");
    // Refuse an attacker-controlled symlink even before the atomic rename.
    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("Destino de exportação inválido ou simbólico".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let tmp = dir.join(format!(
        ".latest-{}-{}.tmp", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    write_new(&tmp, transcript)?;
    if let Err(error) = fs::rename(&tmp, &destination) {
        let _ = fs::remove_file(&tmp);
        return Err(error.to_string());
    }
    Ok(destination)
}

/// Make a uniquely named snapshot on demand. Does not overwrite past exports.
pub fn save_snapshot(workspace: &Path, transcript: &str) -> Result<PathBuf, String> {
    if transcript.len() > MAX_EXPORT_BYTES {
        return Err("O transcript ultrapassou 8 MiB; use /clear para iniciar outra sessão.".into());
    }
    let dir = export_directory(workspace)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S-%9f");
    let path = dir.join(format!("session-{timestamp}.txt"));
    write_new(&path, transcript)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_workspace() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dude-export-test-{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn latest_is_utf8_and_can_be_updated() {
        let workspace = temporary_workspace();
        let path = save_latest(&workspace, "> ei\n🧠 Raciocínio:\nOlá!\n💬 Resposta:\nOi, buddy!\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "> ei\n🧠 Raciocínio:\nOlá!\n💬 Resposta:\nOi, buddy!\n");
        save_latest(&workspace, "> second\nDUDE metrics: 123\n").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "> second\nDUDE metrics: 123\n");
        fs::remove_dir_all(workspace).unwrap();
    }

    #[test]
    fn snapshot_does_not_overwrite_latest() {
        let workspace = temporary_workspace();
        let latest = save_latest(&workspace, "primeira resposta").unwrap();
        let snapshot = save_snapshot(&workspace, "raciocínio e resposta").unwrap();
        assert!(snapshot.file_name().unwrap().to_string_lossy().starts_with("session-"));
        assert_eq!(fs::read_to_string(snapshot).unwrap(), "raciocínio e resposta");
        assert_eq!(fs::read_to_string(latest).unwrap(), "primeira resposta");
        fs::remove_dir_all(workspace).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_export_directory() {
        use std::os::unix::fs::symlink;
        let workspace = temporary_workspace();
        let outside = temporary_workspace();
        fs::create_dir(workspace.join(".dude")).unwrap();
        symlink(&outside, workspace.join(".dude/exports")).unwrap();
        assert!(save_latest(&workspace, "não pode sair").is_err());
        fs::remove_dir_all(workspace).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
