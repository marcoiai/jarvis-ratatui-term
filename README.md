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

No API key is necessary. The Rust application communicates using HTTP JSON with `/api/chat` and `stream: false`.
This branch replaces the original Gemini shell script. The obsolete Gemini shell scripts and tracked key-export script were removed from this branch. **Revoke the previously exposed credential** and clean Git history separately if required; deleting files does not erase older commits.

## Notes

- One active request at a time; the UI stays interactive.
- For safety, `!` runs programs without `sh -c` or `bash -c`. It is still arbitrary command execution by the local user.
- No built-in credential storage.
