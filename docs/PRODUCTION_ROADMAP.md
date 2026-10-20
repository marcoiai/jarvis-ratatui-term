# DUDE — Production Engineering Roadmap

Status: **experimental**. Nothing here is declared production-ready without evidence.

## Philosophy

- Truth over novelty. Prefer reproducible evidence and falsifiable claims.
- Human approval before side effects; models advise, humans authorize.
- Explicit project context, decision rationale and audit trails.
- Preserve the original Jarvis history and incrementally refactor.
- Provider independence and local-first privacy.

## Release gates

### P0 — Baseline stability (first)
- [ ] `cargo check --locked` on macOS and Linux
- [ ] `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test` with unit and integration tests
- [ ] Terminal cleanup on panic and normal exit
- [ ] Regression test for scroll (short, long, Unicode, resize)
- [ ] Remove dead code and unused dependencies
- [ ] Resolve exposed historical API key and review repository secrets
- [ ] Ensure checked-in Cargo.lock matches Cargo.toml

### P1 — Durable memory and context
- [ ] Typed schema with version number and migrations
- [ ] Atomic writes, bounded file sizes, recovery from partial writes
- [ ] Explicit separation of user-provided facts, hypotheses, decisions, and observations
- [ ] Decisions with IDs, date, alternatives, rationale, evidence, status and supersedes relationship
- [ ] Project / task / global memory namespaces
- [ ] Token-budgeted retrieval, provenance and citations to source files
- [ ] Checkpoints with restore tests; inspectable retention and deletion
- [ ] No silent rewriting of historical decisions
- [ ] User-controlled privacy and protection from accidental Git commits
- [ ] Tests for restarts, malformed JSONL, concurrent writes, invalid paths, large histories

### P2 — Provider adapters
- [ ] Common provider trait with chat, tool calls, model discovery, capabilities and errors
- [ ] Ollama adapter with tool-call tests and model lifecycle
- [ ] Gemini adapter with independently configured credentials and verified entitlement path
- [ ] OpenAI adapter with independent API billing/configuration
- [ ] Provider contract tests with local mock HTTP servers
- [ ] Clear privacy gate for cloud transmission of code or context
- [ ] Per-provider budgets, timeouts, cancellation, retry and backoff

### P3 — Engineering workflow
- [ ] Reviewable proposals, decisions, execution plans and evidence
- [ ] Human approval gate for all writes and external side effects
- [ ] Workspace confinement robust against symlink/path attacks
- [ ] Dry-run and diff for code mutations
- [ ] Audit log with the operator, model, tool, parameters and results
- [ ] Explicit uncertainty and opportunity to challenge/reject hypotheses
- [ ] Model disagreement review (optional council) after basic workflow is proven

### P4 — Operational maturity
- [ ] CI for supported OS targets and supported Rust version
- [ ] Reproducible releases, SBOM and dependency audits
- [ ] Structured logs, diagnostics, configuration validation
- [ ] Load and fault-injection tests, recovery drills and performance budgets
- [ ] Documentation for install, permissions, backup, restore and incident response
- [ ] Release candidate verification on real macOS/Linux hardware

## Evidence rule

Every gate must link to passing test output, a reviewed diff or a reproducible manual verification. A successful commit is not evidence of a successful build.

## First iteration

1. Stabilize the current compiler/test baseline.
2. Introduce memory repository interfaces and fixture-based tests.
3. Keep the CLI functional while replacing fragile memory internals.
4. Complete the provider abstraction and test against mock responses.
5. Add review/approval and audit before any automatic file mutation.

The current `.dude/` memory format is an experimental prototype. Back up your workspace's memory before attempting future migrations.
