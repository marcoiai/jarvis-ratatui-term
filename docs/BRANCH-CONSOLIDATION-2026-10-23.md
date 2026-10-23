# Auditoria e consolidação das branches DUDE — 2026-10-23

Base auditada: `3ef1a27cf2524269e69cc81341f84539e7cbd433` (`main` e `origin/main` após fetch). Branch curta: `chore/dude-consolidation`, criada de main em worktree isolado.

## Método e preservação

Para cada uma das 14 branches foram executados `git log --cherry-pick --right-only --no-merges main...BRANCH`, `git range-diff BASE..main BASE..BRANCH`, `git diff main BRANCH` e `git diff BASE BRANCH`, com BASE obtida por `git merge-base`. Os números abaixo são históricos contra a base inicial, não contagens pós-integração. Commits exclusivos não significam funcionalidades ainda necessárias: há dependências compartilhadas, correções substituídas e alternativas incompatíveis.

Os logs completos, diffs, SHAs, hashes dos arquivos locais, patch local e bundle Git completo estão em `.dude/consolidation-2026-10-23/`, ignorado pelo Git. O bundle foi verificado antes de qualquer exclusão. O inventário versionado ao lado deste relatório registra todos os commits exclusivos por branch. As intermediárias só podem ser excluídas após confirmar `merge-base --is-ancestor TIP origin/feat/symphony-resilience-m2`; essa branch permanece remota. Telemetria foi confirmada por equivalência de patches. As branches locais já integradas `fix/investigation-evidence-gate` e `perf/knowledge-latency-metrics` são ancestrais da main.

O Cargo.lock regenerado por Cargo é byte a byte igual ao arquivo local inicialmente modificado. Seu conteúdo é incorporado sem perda; `parse_ollama.zsh` permanece intacto e não versionado.

## Decisões por branch

| Branch | Tip original | Exclusivos por patch | Decisão e preservação |
|---|---|---:|---|
| `feat/dude-alpha-continue` | `faf2165` | 65 | Manter: metadados de leitura e cwd dos comandos portados; interpreter obrigatório, ferramentas extras e dual-local continuam exclusivos. |
| `feat/evidence-ledger-m1` | `209c766` | 3 | Excluir intermediária: integralmente ancestral de symphony-resilience-m2; auditoria textual ainda exige revisão de falsos positivos. |
| `feat/evidence-requirement-gate` | `2a7f6ee` | 14 | Excluir intermediária: preservada em symphony-resilience-m2; main mantém gate por leituras reais, não evidence.len(). |
| `feat/evidence-retrieval-m1-5-1` | `beac1b4` | 37 | Excluir intermediária: preservada em symphony-resilience-m2; retrieval depende de corrigir freshness do store. |
| `feat/intent-preserving-validation` | `4b908d3` | 23 | Excluir intermediária: preservada em symphony-resilience-m2; revisão por LLM não garante preservação da intenção. |
| `feat/interpreter-routing` | `f033657` | 7 | Excluir intermediária: preservada em symphony-resilience-m2; reescrita e roteamento precisam testes com o fluxo atual. |
| `feat/ollama-request-telemetry` | `c3a2d7d` | 2 | Excluir: ambos os patches incorporados (git cherry retorna - para ambos), mais testes HTTP. |
| `feat/persistent-evidence-m1-5` | `1f78f5b` | 30 | Excluir intermediária: preservada em symphony-resilience-m2; timestamp é capturado depois da leitura, permitindo freshness incorreta. |
| `feat/reuse-existing-skills-m1-6` | `b682caf` | 33 | Excluir intermediária: preservada em symphony-resilience-m2; ativação automática e descoberta precisam compatibilidade com paths atuais. |
| `feat/runtime-intent-resolver` | `2c261e5` | 17 | Excluir intermediária: preservada em symphony-resilience-m2; heurística pode consumir perguntas mistas e reporta apenas modelo Ollama. |
| `feat/symphony-resilience-m2` | `f87f930` | 41 | Manter: modelos por papel e checkpoint portados; demais módulos M1/M1.5/M1.6 não aprovados para main nesta auditoria. |
| `feat/validation-gate-m1-2` | `9755edd` | 10 | Excluir intermediária: preservada em symphony-resilience-m2; validação adicional não substitui gates atuais. |
| `fix/ollama-tool-parser-recovery` | `0bc8d03` | 1 | Excluir: recuperação portada e adaptada ao filtro atual; original também preservado em symphony-resilience-m2. |
| `fix/symphony-checkpoint-901506a-build` | `fd53eff` | 42 | Manter: proteção de listagem portada; .dudeignore, workspace map, SFT e slicing Groq ainda exclusivos; logo antigo superado. |

## Integrações validadas

- `ce37dfb`: lockfile sincronizado com as dependências já declaradas na main.
- `310aaca` e `0815acd`: telemetria de requisições, retries e tokens, provenientes de `a3e677f` e `c3a2d7d`.
- `f0b5390`: recuperação específica do parser XML Ollama, proveniente de `0bc8d03`.
- `ee428de`: adapta recuperação ao filtro de raciocínio, rejeita tool calls em retry sem ferramentas e mede falhas de JSON; porta limites explícitos de leitura (`dc900fe`, `106579a`), listagem protegida (`21f2413`) e cwd selecionado para comandos (`3b03336`).
- `c45a480` e `e86d7a0`: modelos por papel (`6e49294`, `6ec8698`), mantendo métricas e extração de Knowledge com o modelo selecionado.
- `85db3fc` e integração subsequente: armazenamento de checkpoint (`f87f930`), conectado ao fluxo atual e validado sem replay automático. Falha de persistência interrompe a sequência antes de outra chamada.

Os gates atuais, aprovação humana de Knowledge, journal, recovery de request, compaction Groq, banner TUI, status do runtime e bracketed paste são preservados. Não foram substituídos os arquivos inteiros das branches antigas. O checkpoint registra inclusive Reviewer; não interpreta histórico como conhecimento aprovado.

## Trabalho exclusivo mantido e motivos

1. **Alpha (`faf2165`)**: interpreter obrigatório pode alterar a disponibilidade e o roteamento; a versão antiga delega ao modelo a necessidade de tools. `view_diff` sobre `.` não aplica o filtro de paths sensíveis ao conteúdo retornado pelo Git. Novas buscas, mapa, memória contextual e dual-local precisam ser portados com os gates atuais. Propostas de greeting/default de modelo e filtros antigos não substituem as correções posteriores da main.
2. **M1/M2 (`f87f930`)**: `evidence_store::capture` obtém o stamp do arquivo depois de coletar o trecho, podendo rotular conteúdo antigo como atual. A revisão automática usa heurísticas de linguagem e retorna conteúdo do modelo sem o filtro atual de raciocínio. `requires_fresh_source` na versão antiga usa contagem de observações em pontos onde main exige leituras de fonte. O corte de handoff em 4500 caracteres pode omitir parte do Architect. Export de Knowledge, skills, runtime intent e EvidenceLedger permanecem recuperáveis nesta branch; não foram confundidos com trabalho já integrado.
3. **Checkpoint/build antigo (`fd53eff`)**: `.dudeignore`, workspace map, dataset SFT e slicing Groq são propostas independentes. Slicing troca o pipeline atual de compaction e pode usar a instrução de finalização como pedido original ao buscar o último user message. O logo dessa branch foi superado pelo banner atual, que monta spans sem cortar bytes UTF-8. O ajuste Gemini de build é semelhante a correção já existente; range-diff não é prova de equivalência funcional.

Essas três branches não são obsoletas e não serão excluídas enquanto suas pendências estiverem abertas. A auditoria e a limpeza segura foram executadas; não se afirma integração completa de todas as funcionalidades históricas.

## Validação

- Base: `cargo check` passou após regeneração necessária do lockfile; `cargo test --locked`: **45 passed**.
- Versão consolidada: `cargo check --locked`: passou; `cargo test --locked`: **58 passed, 0 failed, 0 ignored**; `cargo clippy --locked --all-targets`: passou com avisos preexistentes (dead_code, collapsible_if e too_many_arguments).
- Novos testes cobrem HTTP real em loopback para erro XML, recuperação sem fontes, resposta só com raciocínio, tool call indevida, retry vazio, métricas de JSON inválido; também cobertura de linhas, truncamento Unicode, listagem protegida, cwd de comando, configuração legada/modelos por papel, recuperação de request e checkpoint após falha do Reviewer.
- O sandbox bloqueou inicialmente bind de loopback: 5 falhas de infraestrutura. A mesma suíte passou fora desse bloqueio. Nenhum teste foi ignorado ou removido.
- Não houve teste com provedores reais nem inspeção interativa da TUI. Testes e gates existentes da TUI/Knowledge passaram; credenciais e disponibilidade de modelos remotos continuam dependentes do ambiente.

## Execução de integração e limpeza

A execução final, SHAs publicados e exclusões confirmadas serão registrados abaixo após verificar o remoto. Sem force push, reset destrutivo ou descarte de arquivos locais.
