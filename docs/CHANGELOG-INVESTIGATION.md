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

## 2026-10-21 — Solo parity with Continue Qwen 3.5 4B

- Corrected the apples-to-apples comparison: Continue was running `qwen3.5:4b`, while DUDE defaulted to `qwen3:4b`. DUDE Alpha now defaults to `qwen3.5:4b` (still overridable by `OLLAMA_MODEL` or `/model`).
- `ensure_model` no longer silently switches to the first unrelated installed model when a requested model cannot be pulled; returns the download error instead.
- Simple standalone greetings such as `oi`, `olá`, and `bom dia` already bypass Ollama in commit `46b2c9f`. Deeper source review still uses model and read-only tools.
- For reproducible testing, preload the model on the Mac: `ollama pull qwen3.5:4b`, check `curl http://MAC:11434/api/tags`, and set `OLLAMA_MODEL=qwen3.5:4b` on Linux.
- Pending: run `cargo check`, `cargo test`, verify Ollama response and performance against Continue on identical workspace/prompt. No claim of model fine-tuning/training.

## 2026-10-21 — Solo Agent tool parity, read-only explorer

- Added **glob_search** with bounded recursive traversal (up to 4000 safe workspace files) and **grep_search** case-insensitive text matching, while preserving the original `search_text` for compatibility.
- Added **view_subdirectory** for shallow tree navigation and **view_diff** for Git working-tree patch inspection (no shell execution; read-only Git subprocess).
- Extended `find_files` to use the larger workspace traversal, removing the previous 150-file search cap. Existing sensitive directories, environment files, and symlinks are skipped.
- Tool JSON definitions are provided automatically to Ollama through the existing agent handshake. No write or destructive tool permissions introduced.
- Added unit tests for glob matching, nested searches, and exclusion of `.dude` memory files.
- This is **not** full Continue equivalence: regex-based grep, Git staged/untracked diff selection, streaming action feed, persistent search index and automatic path recovery remain pending.
- GitHub file-edit environment does not run the Linux/Rust toolchain. Run `cargo fmt --check; cargo check; cargo test` before accepting these commits. Same workspace/model benchmark still required.

## 2026-10-21 — SPEC: especialização de modelos e regressão Solo

- Adicionada à SPEC a meta de criar pipeline futuro de dados/avaliação e fine-tuning local, estudando Hugging Face LLM Trainer (SFT, preferências, LoRA/QLoRA) com proteção de código e dados proprietários.
- Fixados critérios explícitos para saudação `ei` não acionar investigação e para não confundir texto `project_map(path="")` com chamada real de ferramenta.
- Reafirmada comparação justa com Continue usando a mesma versão do Qwen 3.5 4B e política de ferramentas; documentação não declara funcionalidade entregue.

## 2026-10-21 — Project reviews require real file reads

- Fixed the observable regression where Qwen invented a plausible `src/main.rs` and described it as verified. `ask_ollama` now reads up to four existing, non-sensitive entrypoint/manifest files using the actual `read_file` implementation, with a bounded first-160-line excerpt, **before** sending a whole-project review to Ollama.
- The prompt distinguishes the successfully read snippets from the heuristic discovery map and older chat memory; additional reading remains autonomous via tools.
- If none of the candidate files can be read, review returns an explicit error instead of pretending to have inspected code.
- **Remaining limitations:** this first read set is heuristic (fixed common filenames), cannot establish full architecture or eliminate hallucinations; no complete structured tool provenance, automatic continuation, or test on the user's workspace yet. Requires `cargo fmt --check`, `cargo check`, `cargo test` and Qwen 3.5 4B runtime benchmark.

## 2026-10-21 — Shell workspace consistency

- `!comando` passa a executar com `Command::current_dir(self.workspace)` capturado no momento da execução; `/workspace` controla tanto ferramentas de arquivo quanto diretório de trabalho dos subprocessos.
- Não altera o current working directory global do processo DUDE; preserva isolamento e previsibilidade das tasks assíncronas.
- Incluído teste assíncrono com `!pwd` equivalente. Executar `cargo test` no checkout para validar.
- O parser atual separa argumentos por whitespace e não executa via shell; `!cd`, pipes e redirecionamentos não ganham semântica de shell com esta mudança.

## 2026-10-21 — Solo: proveniência de ferramentas por execução

- `EvidenceCache` passou a registrar eventos reais por chamada: nome da ferramenta, argumentos, `ok`, `error` ou `cache_hit`, com limite de 300 eventos.
- Ao concluir uma resposta com ferramentas no Ollama, o DUDE anexa um resumo legível de até 25 ações reais executadas durante a solicitação, sem confundir texto gerado pelo modelo com execução de ferramentas.
- Leituras iniciais de revisão agora passam pelo mesmo rastreamento sem releitura duplicada; cache continua restrito a uma solicitação, sem mudança na arquitetura de memória.
- Teste unitário de leituras, cache hit e caminho ausente incluído. **Ainda requer** compilação/testes no checkout Linux e teste real com Qwen 3.5 4B.
- Limites: erros de provider no meio da investigação ainda retornam erro sem resumo parcial de ações, a interface ainda não tem feed de eventos em tempo real, e o histórico de ferramentas não substitui avaliação factual do relatório.

## 2026-10-21 — Regressão encontrada ao revisar o Solo

- Corrigido erro de compilação em `EvidenceCache::export` e `EvidenceCache::len`: após a mudança de tupla para campos nomeados, os dois métodos ainda usavam `self.0`; agora usam `self.cache`.
- Expandido teste do `EvidenceCache` para cobrir `len()` e `export()`, além do registro de execução real, erro e cache hit.
- Benchmark Ghost Player: usuário confirmou Vue + Tauri e integrações YouTube, Google Custom Search, Gemini e Google Cast, corroborando descobertas do DUDE. Resposta conversacional a elogio também foi adequada. Falta observar telemetria real das ferramentas para avaliar proveniência.
- **Validação pendente:** `cargo fmt --check`, `cargo check`, `cargo test`, execução Solo com Qwen 3.5 4B. Não classificar como aprovado sem isso.

## 2026-10-21 — Solo: intenção de revisão e fatos atuais do workspace

- Corrigido o reconhecimento de revisão de projeto para incluir a frase real do benchmark: `analisar o projeto no diretorio atual e me explica o que ele faz`, além de português/inglês equivalente; elogios e conversas normais não disparam esse caminho.
- A revisão executa `grep_search` no workspace atual para os identificadores `OLLAMA_MODEL`, `history.jsonl`, `memory.json`, `router.rs`, além das leituras iniciais e mapa, fornecendo ao LLM correspondências atuais e origem em arquivos (uma busca vazia não comprova inexistência).
- Todos esses acessos passam pelo `EvidenceCache`, aparecendo no resumo real das ferramentas.
- Teste unitário de reconhecimento da intenção adicionado. **Compilação, testes e execução com Qwen ainda não validados no ambiente Linux do usuário.**
- Limitação: essas buscas focadas são uma medida para a regressão observada e não garantem exatidão total. Continua pendente aprimorar seleção contextual de arquivos, recuperação após falha e verificação da qualidade do relatório.

## 2026-10-21 — Interpreter LLM antes do Investigator

- A interpretação de **toda entrada em linguagem natural** passou a ser feita por uma chamada local separada ao Ollama antes da chamada técnica, inclusive em Solo, Dual-Local e Symphony. Comandos explícitos `/` e `!` permanecem responsabilidade do core Rust.
- Modelo Interpreter padrão: `qwen3:4b-instruct-2507-q4_K_M`, configurável com `DUDE_INTERPRETER_MODEL`; modelo Coder padrão permanece `qwen3.5:4b`.
- O Interpreter usa histórico conversacional recente e retorna JSON `{task, needs_workspace}`. O pedido original continua no contexto; a interpretação passa como instrução, **não como evidência de arquivos**. O booleano controla se as ferramentas de investigação estão disponíveis ao estágio técnico.
- Removidos o roteamento de revisão via palavras-chave, a leitura arbitrária de quatro arquivos e as buscas especiais por nomes de arquivos do próprio DUDE. Retirado também o bypass de saudações para que todas as mensagens naturais passem pelo Interpreter.
- `/mode` sem argumento agora informa o modo atual; comandos `/` desconhecidos deixam de ser enviados ao modelo.
- Testes do contrato JSON Interpreter adicionados. Não há fallback silencioso para roteamento por palavras-chave se o Interpreter falhar.
- **Pendente:** executar `cargo fmt --check`, `cargo check`, `cargo test` e benchmark com os dois modelos disponíveis no Ollama do Mac; a qualidade e latência do novo ciclo não foram medidas ainda. O Interpreter pode classificar incorretamente a necessidade de ferramentas; isso requer teste real e telemetria.

## 2026-10-21 — Solo: zero tool calls cannot pass as investigated

- Regressão reproduzida pelo log do usuário: DUDE afirmou ter verificado código, mas finalizou com `Tools executed this turn: none` e fatos antigos (`qwen3:4b`, `memory.json`, `router.rs`).
- No caminho Ollama com ferramentas habilitadas, o primeiro retorno sem execução de ferramenta solicita uma chamada estruturada efetiva e uma exploração do workspace; um segundo retorno sem leitura registrada encerra com erro explícito e não salva o relatório como se fosse verificado.
- Sem mudanças na arquitetura Interpreter → Coder/Symphony. Ainda é necessário testar se o Interpreter classifica a solicitação com `needs_workspace=true` e se o modelo Coder suporta tool calls corretamente.
- Compilação e testes locais pendentes.
