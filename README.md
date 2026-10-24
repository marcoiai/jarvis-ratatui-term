# DUDE — Demented Unified Development Entity

> **Documentação de arquitetura e produto:** [Visão de AI Co-Engineering](docs/VISION.md) · [Especificação funcional/técnica](docs/SPEC.md) · [Arquitetura e diagramas](docs/ARCHITECTURE.md) · [Roadmap e estado real](docs/ROADMAP.md).
>
> Os diagramas distinguem o runtime existente da arquitetura-alvo. **O executor supervisionado ainda não está implementado.** GitHub Actions estão desativadas; validação Rust é local.


An AI-powered development companion for the terminal, built with Rust, Ratatui and local Ollama. Originally developed as Jarvis. DUDE can serve as both a personal coding assistant and (with future integrations) an Enova/NOVVS engineering assistant.

## Requirements

- Rust toolchain (edition 2024)
- Ollama running at `http://127.0.0.1:11434`
- Ollama running (DUDE downloads `qwen3:4b` automatically when missing)

## Run

```sh
ollama serve  # only if Ollama is not already running
OLLAMA_MODEL=qwen3:4b cargo run --bin dude
```

If your server is elsewhere, set `OLLAMA_HOST=http://127.0.0.1:11434`.

## Commands

- Ask a normal question to chat with Ollama. Successful exchanges remain in context.
- `!ls -la` executes a local program directly (no shell interpretation: pipes, globbing and redirects are unsupported).
- `/model granite-code:latest` changes the model during the session.
- `/export` saves the full visible DUDE transcript to a timestamped UTF-8 `.txt` file (including streamed Ollama thinking, answers and metrics). Works even while a response is still streaming.
- After every completed answer or error, DUDE also refreshes `<workspace>/.dude/exports/latest.txt` automatically. Copy or attach that file instead of manually selecting terminal text. `/export` creates `<workspace>/.dude/exports/session-YYYYMMDD-HHMMSS-XXXXXXXXX.txt` without overwriting prior snapshots. The TUI prints the full snapshot path.
- Exports are **local only** and may contain sensitive prompts, project excerpts, model reasoning or errors. Review them before sharing, and avoid adding `.dude/exports/` to Git.
- `/clear` clears conversation and screen.
- `/exit`, `/quit` or `/sair` exits the TUI cleanly (also Esc or Ctrl+C).
- Up/Down scroll, ESC or Ctrl+C exits.

DUDE prefers `qwen3:4b` and checks `/api/tags` at startup. If missing, it downloads the model through `/api/pull` while keeping the interface responsive. It does **not** download when the requested model is already installed. If a download fails and another model is installed, it falls back to that model. Ollama itself must be running. You can override the preferred model with `OLLAMA_MODEL` and inspect installed models using `ollama list`.

## AI file tools (read-only)

DUDE can ask Ollama to call `list_files`, `read_file` (line ranges, similar to `sed -n`), and `search_text` (literal grep-style matches with line numbers). These are Rust implementations, not unrestricted shell commands. Tool-capable Ollama models are required; support varies by model.

Set `DUDE_WORKSPACE=/path/to/project` to restrict AI file reads to a selected directory. Defaults to current directory. Some sensitive paths and symlinks are blocked, but this is **not a security sandbox**: launch DUDE in a trusted, non-sensitive workspace and don't allow untrusted content there. AI file tools do not modify files. User-typed `!command` still runs external programs under the user's permissions.

No API key is necessary. The Rust application communicates using HTTP JSON with `/api/chat`. Interactive Ollama replies use `stream: true`, forwarding `message.thinking` and `message.content` separately to the TUI. Non-interactive knowledge extraction remains non-streaming.

For thinking-capable Qwen3 models, DUDE enables Ollama's thinking stream by default. The thinking text is shown live under **🧠 Raciocínio (Ollama)**, followed by the streamed **💬 Resposta**. Thinking is not added to the persisted conversation history. Set `DUDE_OLLAMA_THINK=0` to disable thinking (and reduce generation work), or `DUDE_OLLAMA_THINK=1` to request it explicitly with another compatible model. Non-thinking-capable models may reject `think: true`; leave it unset or use `0` for those models.
This branch replaces the original Gemini shell script. The obsolete Gemini shell scripts and tracked key-export script were removed from this branch. **Revoke the previously exposed credential** and clean Git history separately if required; deleting files does not erase older commits.

## Notes

- One active request at a time; the UI stays interactive.
- For safety, `!` runs programs without `sh -c` or `bash -c`. It is still arbitrary command execution by the local user.
- `/apikey` manages saved Gemini/Groq credentials.

## Persistent AI Co-Engineering memory

DUDE uses a project-local `.dude/` directory (relative to `DUDE_WORKSPACE`, or current directory). It creates:

- `context.md`: manually editable project goals, constraints and references.
- `memory.md`: explicit durable facts recorded using `/remember FACT`.
- `decisions.md`: append-only engineering decisions using `/decision Decision and rationale`.
- `checkpoints.md`: session checkpoints using `/checkpoint SUMMARY`.
- `history.jsonl`: successful user/assistant exchanges, restored up to the most recent 8 exchanges when starting again.

Type `/memory` or `/coeng status` to see current project context. The system context is refreshed before each chat request. The app limits material loaded into the model, but **history on disk is retained until you delete it yourself**. `/clear` clears the active chat, not persistent storage.

**Privacy:** `.dude/` can contain proprietary material. Do not commit or share it accidentally; add `.dude/` to your project's `.gitignore`. The built-in AI file tools block reading it. This is a first-step implementation of the co-engineering methodology, not automatic verification of facts or a secure storage vault.

Principles adopted from the [AI Co-Engineering research](https://github.com/marcoiai/human-ai-coengineering): evidence over novelty, justified engineering decisions, explicit uncertainty and traceability. Recorded decisions are not automatically validated or approved. Always review them.

## Groq Cloud (opt-in)

Groq chat is available through an OpenAI-compatible Chat Completions endpoint. Configure `GROQ_API_KEY` in the shell before launching DUDE. You may set `GROQ_MODEL` (default `llama-3.3-70b-versatile`; check live model availability in Groq Cloud).

In the TUI, type `/providers` then `/provider groq` to opt into cloud requests; use `/provider ollama` to return to local Qwen. **Selecting Groq sends your current chat and the loaded project context to Groq on subsequent prompts**. Do not select it for confidential projects unless explicitly authorized. The Groq adapter currently supports chat text only, not file tool calling. Your Groq API credentials are separate from OpenAI, Gemini and Ollama.

This is an initial integration, not an audited provider sandbox. The complete provider trait remains on the roadmap.

## Switching workspaces

Use `/workspace` to print the active workspace, or `/workspace ~/Projects/levelup` to explicitly switch to a different directory. The command expands `~`, verifies the target exists, and reloads that project's separate `.dude/` memory and conversation history. AI tools remain confined to the selected workspace and cannot switch it themselves.

Alternatively launch DUDE with `DUDE_WORKSPACE="$HOME/Projects/levelup" cargo run`. The directory must exist and be accessible. `/workspace` is a user command, not an LLM tool.


## Provider roles and Symphony

DUDE supports independent provider assignment to three roles in **both** modes:

- Investigator: explores the workspace and gathers evidence.
- Architect: organizes findings and develops proposals.
- Reviewer: checks claims and prepares the final response.

The selected providers are saved per workspace in `.dude/roles.json`. In **Solo**, DUDE uses only the selected role. In **Symphony**, DUDE invokes all three roles in order, sharing bounded intermediate findings and showing **only one final response** from the Reviewer (no intermediate agent transcript).

```text
/roles
/role investigator ollama
/role architect gemini
/role reviewer groq
/role-model investigator qwen3:4b
/mode solo
/solo architect
/mode symphony
/cloud allow
```

`/mode symfony` is an alias. `/cloud deny` revokes cloud permission for the current session. Permission is **off on startup and after workspace changes**. A cloud role receives conversation history, project memory/context and results of explicitly requested read-only file tools; do not enable cloud for proprietary workspaces unless authorized. Shell commands (`!command`) remain explicitly user initiated.

Supported runtime adapters: Ollama local (`OLLAMA_MODEL`, default `qwen3:4b`); Groq (`GROQ_API_KEY`, optional `GROQ_MODEL`, default `openai/gpt-oss-120b`); Gemini (`GEMINI_API_KEY` or `GOOGLE_API_KEY`, optional `GEMINI_MODEL`, default `gemini-2.5-flash`). Gemini API access and quota are separate from the consumer Google AI Pro subscription. OpenAI stays a configurable placeholder and will fail closed if selected.

If any Symphony stage errors, DUDE displays an error instead of silently treating partial results as a complete three-role answer. Shared tools are read-only, and each provider is limited to eight rounds of tool calls.

`/role-model ROLE MODEL` overrides the model for that role in Solo and Symphony, including Solo's Knowledge extraction. Roles without an override use the provider default. Overrides are saved with the role configuration and included in pending-request recovery. Changing a provider does not clear its model override; set an appropriate model for the new provider.

**Validation status (2026-10-23):** `cargo check --locked` and `cargo test --locked` pass, including local HTTP fixtures for model dispatch and checkpoint recovery. Live provider credentials, model availability and quota are not covered by those fixtures.

## API keys inside DUDE

Cloud API keys can be entered without environment variables:

```text
/apikey
/apikey gemini
/apikey groq
/apikey clear gemini
/apikey clear groq
```

After `/apikey gemini` or `/apikey groq`, type the key into the **masked input** and press Enter. Esc cancels. Missing cloud keys also trigger this prompt before a request is sent. Secrets are never written to chat history, project memory or the repository. Saved credentials live in `~/.config/dude/gemini.key` or `~/.config/dude/groq.key` with owner-only permissions on Unix; existing environment variables take precedence. The current question needs to be sent again after saving a prompted missing key. `/cloud allow` remains separately required to authorize sending project context to external services.

This stores keys in local restricted files, not the operating system's encrypted keychain; protect access to your home directory.

## Groq rate limits in Symphony

A Groq HTTP 429 can occur when Investigator, Architect and Reviewer share the account's token-per-minute quota. The Groq adapter now retries up to three times after the initial request, honoring the suggested retry delay in seconds (or Retry-After when available), with a 75-second cap on each delay. Intermediate Symphony handoffs are capped at 5,000 characters per stage and should be concise. If the quota is still unavailable, the request fails without claiming a completed Symphony answer. Workspace switches deliberately revoke `/cloud allow`; authorize again after changing projects.

## Reviewable engineering knowledge

In Symphony mode, the Reviewer is instructed to propose a structured set of claims, each classified as `documented`, `verified`, or `inferred`, and accompanied by source references. Proposals are stored per workspace in `.dude/knowledge_candidates.json` as `pending` and are **not** written automatically to curated memory.

```
/knowledge
/knowledge approve 1
/knowledge reject 2
/memory
```

Approval is limited to documented/verified claims with source references; inferred claims require further investigation. Approved claims are appended to `.dude/memory.md` with their classifications and references. Rejected proposals remain in the candidate journal with `rejected` status. Review the actual code before approval: the Reviewer can still invent or misinterpret evidence references. Duplicate claims are skipped. The structured output is optional; if the model omits or malforms its proposal, the ordinary answer remains available and no knowledge is promoted.

This is a first candidate-review workflow, not a semantic vector memory or a verified source-line validator. Run `cargo check` and `cargo test` locally to validate changes before use.

## Complexity routing

DUDE classifies requests deterministically before contacting any provider. In Symphony mode, empty-help prompts like `?`, `/help`, and basic greetings receive local responses without a network call. Brief questions are sent to the selected Solo role, while longer requests containing engineering-analysis markers use Investigator → Architect → Reviewer. Use `symphony: YOUR QUESTION` to force all roles even when Solo mode is selected, or `solo: YOUR QUESTION` to force one provider in Symphony mode. Regular `/mode solo` always uses the Solo provider unless a Symphony override is present.

This is deliberately a simple, transparent heuristic, not an LLM intent classifier. It avoids unnecessary requests but can misclassify unusual prompts; use the explicit prefixes for deterministic behavior. Unit tests cover greetings, basic questions, complex engineering reviews, and overrides.

## Request metrics and commercial evaluation

Every dispatched Solo/Symphony request records metadata in `.dude/metrics.jsonl`:
- mode and end-to-end elapsed milliseconds
- each stage's provider, duration and success/error status
- read-only file-tool execution count, evidence-cache hit count and distinct cached evidence item count
- overall success/error status

Run `/metrics` to inspect the latest 8 raw JSONL records. Each model response includes a compact summary of these measurements. Trivial requests answered locally do not dispatch an AI request and do not produce metrics. Logs contain no prompt text, source code or API keys. This is **operational instrumentation**, not yet a benchmark of answer quality. Token usage is recorded from responses that expose it: Ollama `prompt_eval_count`/`eval_count`, Groq `usage.prompt_tokens`/`completion_tokens`, and Gemini `usageMetadata.promptTokenCount`/`candidatesTokenCount`. Missing fields are not estimated, so counts may be incomplete (including failed HTTP calls). Monetary cost remains unimplemented because provider prices and quotas vary.

Use the same engineering task in `solo: ...` and `symphony: ...` to compare latency and tool usage. Independently grade correctness against actual source lines before claiming Symphony improves quality.

### Benchmark protocol (initial)

1. Select a pinned workspace commit and a question with checkable expected observations; keep provider model names and role assignments unchanged.
2. Run `solo: ...` and `symphony: ...` against that same repository state (use fresh sessions if previous answers could bias later results).
3. Inspect `/metrics` for total and per-stage milliseconds, provider-reported input/output tokens, tool executions, and cache hits. Compare measured counts, not guessed financial charges.
4. Manually grade correctness: supported claims, false claims, missed high-priority findings, and usefulness of recommendations. File citations are not proof until checked.
5. Repeat across multiple tasks before drawing conclusions; no quality score is automatically generated from model output.

Known limitation: the current UI includes the metrics summary in chat history; this may slightly increase later context size. Separating telemetry from model-facing history remains future cleanup.

## Workspace status indicators

DUDE renders a compact, read-only workspace bar inspired by shell prompts: directory 📁, Git branch and a dirty-tree marker 🌿, active Python virtualenv or Conda 🐍, available inactive venv 🐍, Rust 🦀, Java ☕, PHP/Laravel/Symfony 🐘, Node 📦 and Docker/Compose 🐳. Only detected stacks appear. It never sources `.zshrc`, `.envrc` or executes workspace scripts; Git status is obtained through `git -C` with an argument list (no shell). A `Dockerfile` indicator means configuration exists, **not** that the daemon or container is running. `VIRTUAL_ENV` and `CONDA_DEFAULT_ENV` reflect environment variables inherited by DUDE at launch; changes made in another shell cannot be observed automatically in the existing DUDE process.

Indicators are refreshed on startup, on `/workspace PATH`, and using `/env` or `/env refresh`. The bar is currently UI-only; workspace metadata is not silently forwarded to cloud providers. It is not a shell prompt emulator or a continuously updating file watcher.

## Personal communication alignment (meta-pattern)

Communication preferences are a **per-developer profile**, separate from repository knowledge. The `marco-personal-communication` LevelUp skill inspired the separation of communication defaults, explicit overrides, pacing, and opt-in persistence; that private skill is **not** automatically distributed to other employees.

DUDE now loads `~/.config/dude/communication.json` for the current OS user and applies its language, verbosity, and pacing instructions in both Solo and Symphony. It does not copy the profile into `.dude/`, and switching workspace keeps personal settings. The current implementation is a small foundation for a broader alignment pattern, not a generic skill interpreter.

Commands:
- `/align` — inspect the profile.
- `/align language pt-BR` or `/align language en` (also `auto`).
- `/align detail concise|balanced|detailed`.
- `/align pacing adaptive|step`.
- `/align reset` — restore defaults.

Common explicit requests like `em português por favor`, `in english please`, `mais objetivo`, and `passo a passo` update the user's profile locally without an API request. The latest instruction for a particular task still takes precedence in the prompt. The implementation deliberately does not infer persistent preferences from every offhand utterance.

Known limits: detection currently uses a conservative set of exact phrases; it is not natural-language preference extraction. Profiles store no identity, API keys or project secrets. Tests cover alignment validation and common triggers; compilation must be verified with Cargo.

## Status bar icon themes

The workspace status bar uses Nerd Fonts glyphs by default (requires a Nerd Font configured in the terminal, e.g. a MesloLGS NF or JetBrainsMono Nerd Font). DUDE cannot reliably detect terminal fonts. Set the icon style when launching the application:

```sh
cargo run                     # Nerd Fonts (default)
DUDE_ICON_STYLE=unicode cargo run # previous emoji style
DUDE_ICON_STYLE=ascii cargo run   # portable, no special font required
```

The status is refreshed on startup, after switching workspaces, and with `/env refresh`. Newer binary changes require restarting DUDE after `git pull` and recompilation.

## Investigation limits and remote Ollama

```sh
OLLAMA_HOST=http://10.10.10.159:11434 \
OLLAMA_MODEL=qwen3:4b-instruct-2507-q4_K_M \
OLLAMA_NUM_CTX=16384 DUDE_TOOL_ROUNDS=12 \
DUDE_WORKSPACE=/path/to/project cargo run --release
```

`OLLAMA_NUM_CTX` defaults to 16384 (valid range 4096–131072); choose a value supported by your model and available memory. `DUDE_TOOL_ROUNDS` defaults to 8 (valid range 1–32); this limits model/tool rounds, not individual calls. Invalid settings use the default. Ollama, Groq and Gemini reserve one additional request without tools to synthesize the evidence when this budget is exhausted. Provider/network/context errors can still interrupt synthesis.

Before tool-enabled investigation, DUDE supplies a bounded filename inventory (up to 200 entries, depth 3). Hidden paths, symlinks and common dependency/build directories are omitted. This is a sample, not an exhaustive audit. Investigation guidance asks the model to cover major areas, cite evidence and disclose gaps; this is not a deterministic per-module scheduler. Larger projects may require focused follow-up requests. Context/history compaction is not yet implemented.

`/apikey` shows key status. `/apikey gemini` and `/apikey groq` open masked entry; `/apikey clear PROVIDER` removes a saved key. Keys are stored by the existing credentials module; `/cloud allow` is still required for cloud use. Ollama does not require a key.

Broad project-understanding requests (for example, `entenda o projeto atual`) now bootstrap actual README, manifest and entry-point excerpts before the first provider call. Rust module declarations in `src/main.rs` provide additional source candidates. At most 12 candidate files are read, with bounded excerpts; these reads appear in the shared evidence cache and execution metrics. This is deterministic initial reading, not exhaustive coverage. Other layouts may still require model-directed tools. Use `/workspace` to verify the project being analyzed.

Standalone language preferences such as `voce pode a partir de agora falar sempre em portugues?` are saved locally with a short acknowledgement and no model call. Mixed requests are not silently consumed as preferences. Technical intent detection gates the inventory, investigation policy and file tools; ordinary conversation receives communication alignment without investigation scaffolding. This detection is a conservative keyword heuristic, not general intent understanding.

## Raw operational memory versus learned knowledge

DUDE keeps separate layers. Successful completed exchanges are appended with UTC timestamps to `.dude/journal/YYYY-MM-DD.jsonl` and synced to disk, independently of the optional Knowledge extraction step. `.dude/history.jsonl` still provides recent chat context. Only reviewed/approved Knowledge candidates enter `.dude/memory.md`; raw journal entries are never automatically promoted to facts. Daily filenames do not imply a background timer: there is no hourly scheduler and unfinished responses are not yet journaled. Protect `.dude/` from Git and backups shared externally; journal entries may contain sensitive code or text.

## Recovery of unfinished requests

Before dispatching a model request, DUDE stores the current user request in `.dude/pending_request.json`. On normal successful completion it removes the recovery marker. If the process crashes or a request fails, the marker remains. At startup the TUI warns about an interrupted request. Type `/recover` to put it back into the editable input (press Enter to retry), or `/recover discard` to remove the marker. No request is replayed automatically. A subsequent submitted request replaces the current pending marker; this is a recovery mechanism for the latest request, not a queue. Response chunks and partially completed tool work are not checkpointed. This is local persistence, not a separate/offsite backup.

Symphony also saves the most recently completed stage in `.dude/symphony_checkpoint.json`, with its role, provider, model, bounded response and coverage. If a later stage fails, this historical record remains available for inspection. A checkpoint write failure stops the pipeline before the next stage. This file does not enable automatic replay or stage-level resume; `/recover` still retries the original request.

Ollama's specific XML tool-parser failure receives at most one text-only recovery request using observed source excerpts. Recovery output passes through the reasoning filter and remains subject to the current evidence gate. Empty output and unexpected tool calls fail closed. Per-request telemetry records round, message count, payload size and retry token usage without recording prompt bodies in the labels.

Source reads identify requested excerpt boundaries, total file lines and output truncation. `complete_file=true` describes returned source coverage, not semantic verification. Explicit `!command` invocations run in the selected DUDE workspace.
