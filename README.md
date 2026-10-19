# DUDE — Demented Unified Development Entity

An AI-powered development companion for the terminal, built with Rust, Ratatui and local Ollama. Originally developed as Jarvis. DUDE can serve as both a personal coding assistant and (with future integrations) an Enova/NOVVS engineering assistant.

## Requirements

- Rust toolchain (edition 2024)
- Ollama running at `http://127.0.0.1:11434`
- A downloaded model (for example `ollama pull granite-code`)

## Run

```sh
ollama serve  # only if Ollama is not already running
OLLAMA_MODEL=granite-code:latest cargo run --bin dude
```

If your server is elsewhere, set `OLLAMA_HOST=http://127.0.0.1:11434`.

## Commands

- Ask a normal question to chat with Ollama. Successful exchanges remain in context.
- `!ls -la` executes a local program directly (no shell interpretation: pipes, globbing and redirects are unsupported).
- `/model granite-code:latest` changes the model during the session.
- `/clear` clears conversation and screen.
- Up/Down scroll, ESC or Ctrl+C exits.

The DUDE attempts to select an installed Ollama model at startup if the configured default is missing. Check `ollama list` if needed.

## AI file tools (read-only)

DUDE can ask Ollama to call `list_files`, `read_file` (line ranges, similar to `sed -n`), and `search_text` (literal grep-style matches with line numbers). These are Rust implementations, not unrestricted shell commands. Tool-capable Ollama models are required; support varies by model.

Set `DUDE_WORKSPACE=/path/to/project` to restrict AI file reads to a selected directory. Defaults to current directory. Some sensitive paths and symlinks are blocked, but this is **not a security sandbox**: launch DUDE in a trusted, non-sensitive workspace and don't allow untrusted content there. AI file tools do not modify files. User-typed `!command` still runs external programs under the user's permissions.

No API key is necessary. The Rust application communicates using HTTP JSON with `/api/chat` and `stream: false`.
This branch replaces the original Gemini shell script. The obsolete Gemini shell scripts and tracked key-export script were removed from this branch. **Revoke the previously exposed credential** and clean Git history separately if required; deleting files does not erase older commits.

## Notes

- One active request at a time; the UI stays interactive.
- For safety, `!` runs programs without `sh -c` or `bash -c`. It is still arbitrary command execution by the local user.
- No built-in credential storage.
