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
