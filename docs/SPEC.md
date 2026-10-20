# DUDE — Especificação funcional e técnica (living spec)

Data de referência: 2026-10-20. Esta spec distingue **implementado**, **parcial** e **planejado**; não implica que todos os testes passaram no ambiente do usuário.

## Escopo e atores

Atores: desenvolvedor humano, DUDE runtime Rust, provedor de IA (Ollama, Groq, Gemini), ferramentas do workspace, memória/Knowledge. A plataforma é principalmente TUI Rust/Ratatui em Linux/macOS, com inferência local ou remota.

### FR-01 — Conversação e roteamento [Implementado]

- Entrada natural; saudações e `/help` tratados localmente em `router.rs`.
- Comandos de TUI e `!program args` processados em `main.rs`; `!` não é shell completo (sem pipes/redirecionamentos/globbing).
- Roteador escolhe Solo ou Symphony, incluindo override `symphony:` e heurística conservadora.
- `/mode`, `/solo`, `/role`, `/provider`, `/model`, `/workspace`, `/clear`, `/cloud`, `/align`, `/knowledge`, `/memory`, `/metrics` e credenciais.
- Alignment pt-BR cobre parte das mensagens locais; internacionalização integral permanece incompleta.

### FR-02 — Provedores [Implementado/parcial]

- `ollama` via HTTP `/api/chat`; ferramentas em rodadas sem streaming, síntese final com streaming.
- Verifica `/api/tags`; tenta `/api/pull` se modelo preferido ausente, com fallback quando há modelo instalado.
- Host vem de `OLLAMA_HOST` (padrão loopback); modelo de `OLLAMA_MODEL` (padrão qwen3:4b).
- Groq/Gemini são opt-in, precisam de chave/consentimento para nuvem.
- `openai` consta entre nomes aceitos de papel, mas adaptador não habilitado.
- Continuidade em caso de falha de conexão durante investigação não é resiliente; recuperação/retry contextual é planejada.

### FR-03 — Investigação e Evidence [Implementado/parcial]

- Ferramentas de leitura: `list_files`, `read_file`, `search_text`, executadas em Rust, delimitadas ao workspace e limitadas por orçamento.
- Inventário e baseline de arquivos representativos, inclusive apps/configuração de Symfony legado; limite por profundidade e caracteres.
- `EvidenceCache` registra leituras, observações, eventos, cache hits e manifesto de arquivos/intervalos solicitados.
- Pedidos de aprofundamento herdam intenção técnica quando há pergunta anterior compatível.
- **Não há ainda validador semântico de todas as afirmações nem cálculo de cobertura de revisão do código.** Não confundir arquivos lidos com módulos totalmente examinados.
- Critério futuro: cada afirmação técnica importante deve poder vincular-se a um conjunto verificável de observações; contradições de cobertura devem ser sinalizadas pelo runtime, não apenas por prompt.

### FR-04 — Solo / Symphony [Implementado]

- Solo: papel selecionado e seu provedor.
- Symphony: Investigator (busca evidências), Architect (solução/trade-offs), Reviewer (crítica e síntese).
- A execução de Symphony é sequencial e recebe handoffs; não é debate paralelo contínuo.
- UI exibe somente papel/provedores relevantes ao modo ativo.

### FR-05 — Knowledge [Implementado/parcial]

- Discovery: Reviewer em Symphony propõe JSON de candidatos; Solo executa segunda inferência quando houve evidência de ferramentas.
- Candidate: `classification` = documented, verified ou inferred, claim, evidências, pergunta e status.
- Validation: confere que referências `arquivo:linha` apareceram em saídas reais de `read_file`; se não, rebaixa a inferred e não permite aprovação como fato.
- Review: TUI pergunta ao usuário sobre candidato pendente; `sim` aprova apenas documentado/verificado com referências observadas; `não` rejeita; `/knowledge` permite revisão manual.
- Promotion: aprovado é acrescentado a `.dude/memory.md`.
- **Limitação:** referência de linha observada NÃO garante que o conteúdo dessa linha sustente a inferência; falta verificação semântica e gerenciamento refinado de conflitos, obsolescência e atualização da fonte.
- Um candidato não deve virar memória sem consentimento humano.

### FR-06 — Memória [Implementado/parcial]

- Memória local por workspace `.dude/`: `context.md`, `decisions.md`, `memory.md`, `history.jsonl`; `checkpoints.md` quando solicitado.
- Perfil global do usuário `~/.config/dude/communication.json`; credenciais locais em pasta de configuração com verificações de permissões.
- Reaproveitamento de histórico recente e contexto curado. Não equivale a memória semântica global/projeto/tarefa com recuperação vetorial automática.
- Requisitos futuros: versionar fatos com proveniência, expiração/atualização e checkpoints por tarefa.

### FR-07 — Métricas [Implementado]

- `.dude/metrics.jsonl`: modo, duração, estágios, tokens reportados, chamadas e cache de tools, eventos e cobertura amostral.
- Os tokens são contados somente quando a API os fornece; não tratá-los como contagem integral garantida.

### FR-08 — Agente executor supervisionado [Planejado]

- Elaborar plano e critérios de aceite; gerar diff antes de gravar; mostrar arquivos afetados.
- Aprovação explícita por lote/ação; aplicar mudanças atomicamente, rastrear baseline e gerar rollback.
- Execução de testes por política e permissões; limites de tempo/recursos, saída e logs.
- Review de alterações após testes e loop de correção, com parada por orçamento.
- Sem prometer mutação remota ou deploy sem autorização e integração próprias.

## Requisitos não funcionais

- **Segurança:** caminhos sensíveis bloqueados em ferramentas de IA, sem assumir sandbox forte. Comandos `!` fornecidos pelo usuário executam com as permissões locais.
- **Resiliência:** falha de Ollama deve produzir erro compreensível; retomar trabalho em andamento é requisito futuro.
- **Usabilidade:** comandos locais devem respeitar Alignment, sem criar investigação automática para saudações.
- **Auditabilidade:** registro de execução separa observações, inferências e ações.
- **Portabilidade:** Rust compilável no sistema de destino; não confundir compatibilidade pretendida com teste executado.

## Critérios de aceite do marco de execução supervisionada

1. DUDE lê repositório e identifica arquivos relevantes sem declarar cobertura inventada.
2. Propõe plano e diff revisáveis **antes** da mutação.
3. Nenhum arquivo é alterado pelo agente antes do consentimento.
4. Testes definidos na tarefa rodam com logs, timeout e relato explícito de falhas.
5. Possível desfazer alterações aplicadas pelo agente.
6. Resultado e decisões são persistidos com proveniência e aprovação; hipóteses nunca viram fatos automaticamente.

## Referências de implementação

`src/main.rs`, `router.rs`, `roles.rs`, `file_tools.rs`, `investigation.rs`, `evidence.rs`, `knowledge.rs`, `memory.rs`, `alignment.rs`, `cloud.rs`, `credentials.rs`, `environment.rs`, `metrics.rs`.

Esta documentação descreve o repositório na data de referência. Quando houver divergência, revisar o código atual e atualizar a spec.
