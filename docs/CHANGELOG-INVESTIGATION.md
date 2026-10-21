# Investigation checkpoint changelog

## 2026-10-21 — Restore build of Symphony checkpoint 901506a

- Created isolated branch at historical checkpoint `901506a2fe8e9c4cbd86c25f52afea2de17dffee`, without modifying existing working branches.
- Fixed `src/cloud.rs` Gemini adapter signature to accept `allow_tools` and `evidence`, matching its body and caller in `src/main.rs`.
- This addresses the five compiler errors reported on this checkpoint; compilation, tests and Symphony end-to-end execution remain unverified and must be run on the Mac.

## 2026-10-21 — SPEC v1: DUDE orientado por evidências

- Adicionada `docs/SPEC-DUDE-ENGINEERING.md` com definição do produto especializado, comandos explícitos, pipeline `/analyze`, arquitetura em camadas e critérios de aceite objetivos.
- Qualidade de análise Solo é baseline obrigatório; Symphony precisa demonstrar qualidade igual ou superior no mesmo projeto e pedido.
- Próximo trabalho: obter baseline Solo verificável e executar a primeira fatia vertical de `/analyze`, sem priorizar funcionalidades cosméticas.
- SPEC apenas documentada nesta mudança; o runtime atual ainda não implementa os novos contratos.

## 2026-10-21 — Qualidade sem gate artificial de linha

- SPEC corrigida: referência `path:linha` deixa de ser critério obrigatório de aceitação. A ausência de número de linha não invalida análise profunda, autônoma e tecnicamente correta.
- Priorizar compreensão do código, relações entre módulos, riscos, limitações honestas e utilidade. Linhas são opcionais quando ajudam na localização de problemas e devem ser precisas se citadas.
- Essa alteração é somente documental; nenhum código de runtime ou gate de evidências foi modificado nesta revisão.

## 2026-10-21 — Solo dual-local opcional (Ollama)

- Adicionado `/dual on|off` e `/dual` para ativar/inspecionar segunda opinião local sem alterar o modo Solo padrão. `/reason-model MODELO` escolhe o segundo modelo.
- O primeiro estágio usa o modelo local já configurado (`/model`, normalmente `qwen3:4b`) e mantém as mesmas ferramentas de leitura. O segundo estágio usa Ollama no mesmo `OLLAMA_HOST`, **sem ferramentas**, para acrescentar achados e planejamento, não substituir o relatório original.
- Segundo modelo padrão `qwen2.5-coder:7b`; substituível por `OLLAMA_REASONING_MODEL`. `DUDE_DUAL_LOCAL=1` habilita no início.
- Falhas do segundo estágio preservam a resposta completa do primeiro. Um segundo modelo igual ao primeiro não dispara inferência duplicada.
- Sem novos gates de citações, nenhuma alteração no Solo desligado, Symphony ou seleção de arquivos.
- **Pendente**: `cargo fmt --check`, `cargo check`, `cargo test` e avaliação Mac/Linux contra o baseline de qualidade, incluindo impacto de latência.

## 2026-10-21 — Ocultar blocos de raciocínio embutidos em respostas Ollama

- Log de teste Dual-Local mostrou texto intermediário e marcador `</think>` antes do parecer. O processamento de `ask_ollama` agora remove blocos `<think>...</think>` da resposta exibida, inclusive saída com marcador de fechamento isolado.
- Preserva conteúdo final após o fechamento e respostas normais, sem alterar a investigação ou adicionar critérios obrigatórios de citação. Testes unitários do filtro incluídos.
- Sem validação de compilação/execução nesta ferramenta; testar `cargo check`, `cargo test` e execução real em Mac/Linux.

## 2026-10-21 — Continue: Ollama estruturado e memória relevante

- O adaptador Ollama agora aceita o campo separado `message.thinking`, solicita `think: false` nas chamadas de chat e remove marcadores `<think>` do histórico de chamadas com ferramentas, além da resposta exibida. Isso visa manter o canal visível limpo; o modelo ainda pode pensar internamente conforme sua implementação.
- `Memory::context_for(question)` seleciona entradas **já aprovadas** em `memory.md` e `decisions.md` relacionadas a termos da pergunta e mantém o contexto de projeto administrado pelo usuário. Não modifica nem promove conhecimento automaticamente.
- O refresh do contexto do projeto no envio de mensagens usa recuperação direcionada. Essa seleção é lexical, não vetorial; perguntas genéricas podem receber pouco conhecimento recuperado.
- Não foi introduzido ainda índice estrutural (Tree-sitter/FTS) ou cache persistente com invalidação por hash; estes exigem desenho e teste dedicados.
- Validação local pendente: `cargo fmt --check`, `cargo check`, `cargo test` e execução real com Qwen no Ollama do Mac e código no Linux.

## 2026-10-21 — Alpha Continue-inspired (branch feat/dude-alpha-continue)

- Isolated experimental alpha from the tested Solo checkpoint; preserves the existing Solo and opt-in Dual-local configurations.
- New read-only `project_map` tool exports a bounded, approximate map of source declarations and paths for Rust, Python, and select Java/PHP/C++/JavaScript/TypeScript forms. Built using the existing safe workspace scan; respects exclusion of `.dude`, `.git`, secrets, and dependency directories.
- The map is a heuristic *on-demand* overview; it is not yet Tree-sitter, full-text indexing, a persistent hash-invalidation cache, or automatic semantic retrieval. Its purpose is to provide a cheap exploration step for Qwen.
- Retains earlier Ollama structured-response changes and per-question retrieval of approved project memory.
- Tests added for map extraction and memory-directory exclusion, alongside existing memory and think-tag tests.
- TODO before alpha acceptance: run `cargo check`, `cargo test`, compare Qwen Solo before/after on the same workspace and question, record tool calls, elapsed time and answer quality.
- Symphony remains an experimental separate mode; do not claim mature shared-state multi-agent orchestration until validated.
- Codex fork intentionally **not performed**: OpenAI Codex is a distinct Apache-2.0 Rust agent; assess as independent experimental downstream before adopting its runtime/security and contribution obligations.

## 2026-10-21 — Corrective engineering pass against Continue reference

Reference implementation inspected:
- Continue `core/llm/llms/Ollama.ts` separates thinking/content/tool calls and maintains provider tool compatibility.
- Continue `core/context/providers/SearchContextProvider.ts` delegates exact workspace search to an IDE search implementation.

DUDE changes:
- Added `find_files(query)` as a bounded, read-only workspace path lookup to recover from mistaken relative paths; validated with a nested `apps/recinto/config/security.yml` fixture.
- Updated Ollama investigation instructions to prefer workspace-based discoveries over old conversation assumptions, start with map/discovery for repository reviews, open relevant implementations, and retry wrong paths via `find_files`.
- Increased the maximum tool iterations from 8 to 18 for tool-enabled requests. **This is a provisional bound, not mature planning or guaranteed depth.**
- Existing `project_map`, `context_for`, and separate Ollama `thinking` field remain in the alpha.

Known limitations:
- `scan` bounds search to the first 150 files, so `find_files` may miss files in a large repo; reporting 'not found' is not proof of absence.
- No automated rescue call is performed after a failed `read_file`; the agent must choose `find_files`.
- No actual code indexing/FTS, robust persistent evidence cache with invalidation, or structured Symphony collaboration yet.
- No compile/test execution done here; run `cargo fmt --check && cargo check && cargo test` in the checkout, then benchmark same project/prompt/model against Continue.

## 2026-10-21 — Fresh review provenance and Ollama errors

- For explicit whole-project review requests, the Ollama route now adds a fresh bounded `project_map` overview from the current workspace. It labels the inventory as paths/declarations only, not proof that implementations were read. Past assistant exchanges must not be presented as fresh evidence.
- Empty final replies are treated as errors and not recorded as successful responses.
- Connection failures now report the Ollama host/model and whether reqwest classified the failure as a connection or timeout error.
- The existing `App::complete` branch already removes the unanswered user message on errors and does not save a failed assistant reply; no fabricated fallback is introduced.
- Still incomplete: explicit per-turn tool-evidence provenance in the UI, safe persistent cache invalidation, and adaptive recovery when Ollama becomes unreachable mid-investigation. No local cargo execution yet.
