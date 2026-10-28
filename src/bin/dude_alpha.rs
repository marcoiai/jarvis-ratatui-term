//! DUDE Alpha: interactive read-only shell for the specialist binaries.
//! No legacy DUDE executor, no automatic writes, no arbitrary shell commands.
use std::{env, io::{self, Write}, path::PathBuf, process::{Command, ExitCode}};

fn specialist(name: &str, args: &[&str]) -> Result<(), String> {
    let exe = env::current_exe().map_err(|e|e.to_string())?
        .with_file_name(format!("{name}{}", env::consts::EXE_SUFFIX));
    if !exe.is_file() {
        return Err(format!("Missing {}. Build with: cargo build --bins", exe.display()));
    }
    let status = Command::new(exe).args(args).env("GIT_OPTIONAL_LOCKS", "0")
        .status().map_err(|e|e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("{name} exited: {status}")) }
}
fn help() {
    println!("DUDE Alpha (read-only)");
    println!("  inspect                  Inspect destination workspace");
    println!("  inspect-source           Inspect source workspace");
    println!("  diff <relative-file>     Show first diff hunk");
    println!("  coverage <relative-file> Collect diff hunk metadata");
    println!("  analyze <file> <hunk>    Ask local MLX/Ollama to analyze a hunk");
    println!("  help / exit              Show help or quit");
    println!("No writes, patch application, or arbitrary commands supported.");
}
fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("Usage: dude_alpha SOURCE_WORKSPACE DEST_WORKSPACE");
        return ExitCode::from(2);
    }
    let source = match PathBuf::from(&args[0]).canonicalize() {
        Ok(p) if p.is_dir() => p,
        _ => { eprintln!("Source workspace must be an existing directory"); return ExitCode::from(2); }
    };
    let dest = match PathBuf::from(&args[1]).canonicalize() {
        Ok(p) if p.is_dir() => p,
        _ => { eprintln!("Destination workspace must be an existing directory"); return ExitCode::from(2); }
    };
    let source = source.to_string_lossy().to_string();
    let dest = dest.to_string_lossy().to_string();
    println!("Source: {source}\nDestination: {dest}");
    help();
    loop {
        print!("\ndude-alpha> ");
        if io::stdout().flush().is_err() { return ExitCode::FAILURE; }
        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => (),
            Err(e) => { eprintln!("Input error: {e}"); return ExitCode::FAILURE; }
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        let result = match parts.as_slice() {
            [] => continue,
            ["exit" | "quit" | "/exit"] => break,
            ["help" | "/help"] => { help(); continue; },
            ["inspect"] => specialist("workspace_inspector", &[&dest]),
            ["inspect-source"] => specialist("workspace_inspector", &[&source]),
            ["diff", file] => specialist("workspace_diff", &[&source, &dest, "--file", file, "--hunk", "1", "--count", "1"]),
            ["coverage", file] => specialist("workspace_coverage", &[&source, &dest, "--file", file]),
            ["analyze", file, hunk] if hunk.parse::<usize>().is_ok_and(|n| n > 0) =>
                specialist("workspace_investigator", &[&source, &dest, "--file", file, "--hunk", hunk]),
            _ => { eprintln!("Unknown command or bad arguments. Type help."); continue; }
        };
        if let Err(err) = result { eprintln!("Error: {err}"); }
    }
    println!("Bye!");
    ExitCode::SUCCESS
}
