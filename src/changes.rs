//! Opt-in, per-action changes for the local direct Ollama mode.
//! The model may PROPOSE an action, but only the human /approve command applies it.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use serde_json::{json, Value};

const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_CHANGE_BYTES: usize = 24 * 1024;
const MAX_DIFF_LINES: usize = 220;

#[derive(Clone, Debug)]
pub enum PreparedAction {
    File {
        path: String,
        before: Option<String>,
        after: String,
        preview: String,
    },
    Command {
        program: String,
        args: Vec<String>,
        preview: String,
    },
}

impl PreparedAction {
    pub fn preview(&self) -> &str {
        match self {
            Self::File { preview, .. } | Self::Command { preview, .. } => preview,
        }
    }
}

pub fn is_action(name: &str) -> bool {
    matches!(name, "edit_file" | "create_file" | "run_command")
}

pub fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{
            "name":"edit_file",
            "description":"PROPOSE replacing exactly one existing text fragment in a workspace file. Does NOT write anything until the human sees a diff and types /approve. First read the file to obtain the exact old_text.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Relative file path inside the workspace"},
                "old_text":{"type":"string","description":"Exact existing text, unique within file"},
                "new_text":{"type":"string","description":"Replacement text"}
            },"required":["path","old_text","new_text"]}
        }}),
        json!({"type":"function","function":{
            "name":"create_file",
            "description":"PROPOSE creating a new UTF-8 text file. Does NOT write until the human reviews its full content and types /approve. Parent folder must exist.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Relative path to a NEW file"},
                "content":{"type":"string","description":"Full new UTF-8 file contents"}
            },"required":["path","content"]}
        }}),
        json!({"type":"function","function":{
            "name":"run_command",
            "description":"PROPOSE running a build/test command in the workspace. Does NOT execute until the human explicitly types /approve. No shell, pipes or redirection; supported programs: cargo, pnpm, npm, pytest, ctest, cmake, make, mvn, gradle.",
            "parameters":{"type":"object","properties":{
                "program":{"type":"string"},
                "args":{"type":"array","items":{"type":"string"}}
            },"required":["program","args"]}
        }})
    ]
}

// Reject traversal, sensitive areas, symlinks (including intermediate parents),
// non-files and paths outside the trusted workspace.
fn workspace_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || relative.len() > 500 {
        return Err("Caminho vazio ou longo demais".into());
    }
    let mut full = root.to_path_buf();
    let mut components = 0usize;
    for comp in Path::new(relative).components() {
        let Component::Normal(name) = comp else {
            return Err("Somente caminhos relativos simples são permitidos".into());
        };
        let name = name.to_string_lossy();
        if name.starts_with(".env") || matches!(name.as_ref(),
            ".git" | ".dude" | ".ssh" | ".aws" | ".config" | "target" |
            "node_modules" | "id_rsa" | "id_ed25519") {
            return Err("Caminho sensível/bloqueado".into());
        }
        components += 1;
        full.push(name.as_ref());
        match fs::symlink_metadata(&full) {
            Ok(meta) if meta.file_type().is_symlink() => return Err("Symlink bloqueado".into()),
            Ok(_) => (),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(err.to_string()),
        }
    }
    if components == 0 { return Err("Selecione um arquivo".into()); }
    let parent = full.parent().ok_or("Diretório pai ausente")?
        .canonicalize().map_err(|e| format!("Diretório pai inválido: {e}"))?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) {
        return Err("Arquivo fora do workspace".into());
    }
    if !parent.is_dir() { return Err("Diretório pai inválido".into()); }
    Ok(full)
}

fn read_text(path: &Path) -> Result<String, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Somente arquivos regulares são editáveis".into());
    }
    if meta.len() > MAX_FILE_BYTES as u64 {
        return Err("Arquivo maior que 512 KiB; edite um trecho menor por outro mecanismo".into());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

fn diff_preview(path: &str, first_line: usize, removed: &str, added: &str, new_file: bool) -> Result<String,String> {
    let a: Vec<_> = if new_file { Vec::new() } else { removed.lines().collect() };
    let b: Vec<_> = added.lines().collect();
    if a.len() + b.len() > MAX_DIFF_LINES || removed.len() + added.len() > MAX_CHANGE_BYTES * 2 {
        return Err("Diff grande demais para revisão no TUI; divida a alteração".into());
    }
    let mut diff = format!(
        "--- {}\n+++ b/{path}\n@@ trecho na linha {first_line} @@\n",
        if new_file { "/dev/null".to_string() } else { format!("a/{path}") }
    );
    for line in &a { diff.push('-'); diff.push_str(line); diff.push('\n'); }
    for line in &b { diff.push('+'); diff.push_str(line); diff.push('\n'); }
    if diff.len() > 48_000 { return Err("Diff ultrapassa o limite de revisão".into()); }
    Ok(diff)
}

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str)
        .ok_or_else(|| format!("Argumento obrigatório ausente: {key}"))
}

pub fn prepare(root: &Path, tool: &str, args: &Value) -> Result<PreparedAction, String> {
    match tool {
        "edit_file" => {
            let relative = arg(args, "path")?;
            let old_text = arg(args, "old_text")?;
            let new_text = arg(args, "new_text")?;
            if old_text.is_empty() || old_text.len() > MAX_CHANGE_BYTES ||
                new_text.len() > MAX_CHANGE_BYTES {
                return Err("old_text deve ser não vazio; limite de 24 KiB por trecho".into());
            }
            let path = workspace_file(root, relative)?;
            let before = read_text(&path)?;
            let offsets: Vec<_> = before.match_indices(old_text).map(|(i,_)|i).collect();
            if offsets.len() != 1 {
                return Err(format!("Trecho antigo deve aparecer exatamente uma vez (encontrado {})", offsets.len()));
            }
            let at = offsets[0];
            let first_line = before[..at].bytes().filter(|b|*b==b'\n').count()+1;
            let after = format!("{}{}{}", &before[..at], new_text, &before[at+old_text.len()..]);
            if before == after { return Err("Nenhuma alteração proposta".into()); }
            if after.len() > MAX_FILE_BYTES { return Err("Arquivo resultante grande demais".into()); }
            let preview = diff_preview(relative,first_line,old_text,new_text,false)?;
            Ok(PreparedAction::File {
                path: relative.to_string(), before: Some(before), after, preview
            })
        }
        "create_file" => {
            let relative = arg(args, "path")?;
            let content = arg(args, "content")?;
            if content.is_empty() || content.len() > MAX_CHANGE_BYTES {
                return Err("Arquivo novo precisa ter conteúdo de até 24 KiB".into());
            }
            let path = workspace_file(root, relative)?;
            match fs::symlink_metadata(&path) {
                Ok(_) => return Err("Arquivo já existe; use edit_file".into()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
                Err(err) => return Err(err.to_string()),
            }
            let preview = diff_preview(relative,1,"",content,true)?;
            Ok(PreparedAction::File {
                path: relative.to_string(), before: None,
                after: content.to_string(), preview
            })
        }
        "run_command" => {
            let program = arg(args, "program")?;
            let permitted = ["cargo", "pnpm", "npm", "pytest", "ctest",
                             "cmake", "make", "mvn", "gradle"];
            if !permitted.contains(&program) { return Err("Programa não permitido para run_command".into()); }
            let args = args.get("args").and_then(Value::as_array).ok_or("args deve ser uma lista")?;
            if args.is_empty() || args.len() > 30 { return Err("Informe de 1 a 30 argumentos".into()); }
            let mut argv = Vec::new();
            for item in args {
                let value = item.as_str().ok_or("Argumentos precisam ser strings")?;
                if value.len() > 256 || value.contains(['\0','\n','\r']) {
                    return Err("Argumento inválido".into());
                }
                argv.push(value.to_string());
            }
            // Avoid common package-manager bypasses and executable dispatch.
            if program == "cargo" && !matches!(argv[0].as_str(),
                "check" | "test" | "build" | "fmt" | "clippy") {
                return Err("Cargo limitado a check/test/build/fmt/clippy".into());
            }
            if program == "npm" && !matches!(argv[0].as_str(),"run" | "test") {
                return Err("npm limitado a run/test".into());
            }
            if program == "pnpm" && !matches!(argv[0].as_str(),"run" | "test" | "check" | "lint" | "build") {
                return Err("pnpm limitado a run/test/check/lint/build".into());
            }
            let preview = format!(
                "COMANDO PROPOSTO (não executado)\nWorkspace: {}\nPrograma: {program}\nArgumentos: {}\n⚠ Build/test pode executar código do projeto com suas permissões locais.",
                root.display(), argv.iter().map(|x|format!("{x:?}")).collect::<Vec<_>>().join(" ")
            );
            Ok(PreparedAction::Command { program: program.to_string(), args: argv, preview })
        }
        _ => Err(format!("Ferramenta não autorizada: {tool}")),
    }
}

fn write_file(root: &Path, relative: &str, before: Option<&str>, after: &str) -> Result<String,String> {
    let dest = workspace_file(root,relative)?;
    match before {
        Some(expected) => {
            let actual = read_text(&dest)?;
            if actual != expected { return Err("Arquivo mudou após a proposta. Refaça a investigação antes de aprovar.".into()); }
        }
        None => {
            if fs::symlink_metadata(&dest).is_ok() {
                return Err("Arquivo criado por outro processo. Proposta cancelada.".into());
            }
        }
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|e|e.to_string())?.as_nanos();
    let temp = dest.with_file_name(format!(".dude-edit-{}-{stamp}.tmp",std::process::id()));
    let result = (|| -> Result<(),String> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)
            .map_err(|e|e.to_string())?;
        file.write_all(after.as_bytes()).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e|e.to_string())?;
        if before.is_some() {
            let permissions = fs::metadata(&dest).map_err(|e|e.to_string())?.permissions();
            fs::set_permissions(&temp,permissions).map_err(|e|e.to_string())?;
        }
        fs::rename(&temp,&dest).map_err(|e|e.to_string())?;
        Ok(())
    })();
    if result.is_err() { let _ = fs::remove_file(&temp); }
    result?;
    Ok(format!("Alteração aplicada em {relative}. Revise o diff com git diff e rode os testes."))
}

pub async fn apply(root: PathBuf, action: PreparedAction) -> Result<String,String> {
    match action {
        PreparedAction::File { path, before, after, .. } => {
            tokio::task::spawn_blocking(move ||
                write_file(&root,&path,before.as_deref(),&after)
            ).await.map_err(|e|e.to_string())?
        }
        PreparedAction::Command { program, args, .. } => {
            // No shell. Execution is only reached from the explicit /approve UI path.
            let mut command = tokio::process::Command::new(&program);
            command.args(&args).current_dir(root).stdin(Stdio::null())
                .kill_on_drop(true);
            let output = tokio::time::timeout(Duration::from_secs(120),command.output())
                .await.map_err(|_|"Comando excedeu 120 segundos".to_string())?
                .map_err(|e|e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let visible = format!(
                "Comando: {program} {}\nExit: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                args.join(" "),output.status,
                stdout.chars().take(16_000).collect::<String>(),
                stderr.chars().take(8_000).collect::<String>()
            );
            if !output.status.success() { return Err(visible); }
            Ok(visible)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dude-approved-change-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn edit_is_a_proposal_until_explicit_application() {
        let root=test_dir();
        fs::write(root.join("main.rs"),"fn old() {}\n").unwrap();
        let action=prepare(&root,"edit_file",&json!({
            "path":"main.rs","old_text":"old","new_text":"new"
        })).unwrap();
        assert!(action.preview().contains("-old"));
        assert_eq!(fs::read_to_string(root.join("main.rs")).unwrap(),"fn old() {}\n");
        let runtime=tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(apply(root.clone(),action)).unwrap();
        assert_eq!(fs::read_to_string(root.join("main.rs")).unwrap(),"fn new() {}\n");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_edits_and_suspect_paths_are_rejected() {
        let root=test_dir();
        fs::write(root.join("main.rs"),"before").unwrap();
        let action=prepare(&root,"edit_file",&json!({
            "path":"main.rs","old_text":"before","new_text":"after"
        })).unwrap();
        fs::write(root.join("main.rs"),"changed independently").unwrap();
        let runtime=tokio::runtime::Runtime::new().unwrap();
        assert!(runtime.block_on(apply(root.clone(),action)).is_err());
        for path in ["../outside","/tmp/evil",".dude/memory.md",".git/config",".env"] {
            assert!(prepare(&root,"create_file",&json!({"path":path,"content":"test"})).is_err(),"{path}");
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn command_is_only_prepared_not_executed() {
        let root=test_dir();
        let action=prepare(&root,"run_command",&json!({
            "program":"cargo","args":["test","--locked"]
        })).unwrap();
        assert!(action.preview().contains("cargo"));
        assert!(prepare(&root,"run_command",&json!({"program":"sh","args":["-c","echo risky"]})).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
