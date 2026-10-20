# DUDE — Roadmap para o objetivo de AI Co-Engineering

Referência: 2026-10-20. Este arquivo documenta estado e critérios; itens pendentes não estão concluídos.

| Marco | Estado | Evidência / próximo entregável |
|---|---|---|
| TUI Rust/Ratatui, comandos, detecção de ambiente | Implementado | `main.rs`, `environment.rs` |
| Inferência Ollama (inclusive Linux → Mac), Groq e Gemini opt-in | Implementado com limitações | `main.rs`, `cloud.rs`; falta reconexão robusta |
| Solo/Symphony com Investigator/Architect/Reviewer | Implementado | `roles.rs`, `run_symphony` |
| Investigation Engine: inventory, baseline, tool calls | Implementado/parcial | `investigation.rs`, `file_tools.rs` |
| Evidence: cache, observações, manifesto, arquivo:linha | Implementado/parcial | `evidence.rs`; falta prova semântica |
| Memory por projeto e Alignment global | Implementado/parcial | `memory.rs`, `alignment.rs` |
| Knowledge automático Solo/Symphony + pergunta sim/não | Implementado/parcial | `knowledge.rs`, `main.rs`; falta controle de obsolescência |
| Métricas, tokens reportados, rastreio tool | Implementado | `metrics.rs` |
| Regras de síntese verificadas independentemente por código | Pendente | claim ledger + evidências + checagem de contradição |
| Retomar sessão após interrupção do Ollama | Pendente | checkpoint da execução + reconnect + idempotência |
| Localização completa de mensagens via Alignment | Pendente | catálogo transversal de mensagens locais |
| Planner + proposta de patch | Pendente | planos, diffs e critérios de aceite |
| Approval Gate para arquivos/comandos | Pendente | autorização explícita e auditável |
| Patch Engine, Test Runner e rollback | Pendente | execução supervisionada, isolamento e logs |
| Reviewer pós-execução + atualização de Knowledge | Pendente | resultado verificável e memória curada |

## Ordem de trabalho

### M1 — Confiança de evidências
Ledger de afirmações referenciadas e comparação com linhas realmente observadas; eliminar contradições como “arquivo não lido” quando há leitura registrada; não inventar cobertura porcentual. Verificação semântica explícita com indicação de incerteza.

### M2 — Continuidade e resiliência
Persistir estado de investigação e retomar do último checkpoint sem duplicar mutações. Reconectar ao Ollama remoto quando possível, com erros recuperáveis e configuração persistente do servidor.

### M3 — Motor executor supervisionado
Planner → diff → consentimento → apply → testes → revisão → rollback. Começar com operações pequenas em workspace de teste e sem comandos destrutivos.

### M4 — Memória e aprendizagem duráveis
Fatos versionados, proveniência, detecção de divergência/obsolescência, revisão de conhecimento acumulado e contexto de tarefas; alinhamento transversal da linguagem.

### M5 — Distribuição
Builds nativos Linux/macOS e empacotamento; integração de runtime independente da interface; Mac pode servir Ollama, mas não precisa hospedar o app cliente.

## Política de qualidade e workflow Git

- Branch curta, PR para `main`, limpeza das branches após merge.
- GitHub Actions desativadas a pedido do mantenedor. Validar no ambiente local com `cargo fmt --all -- --check`, `cargo test` e `cargo clippy --all-targets` antes de considerar um marco verificado.
- Não inventar resultado de teste se só houve merge no GitHub.
- Preservar `Cargo.lock` modificado localmente até entender sua origem.
