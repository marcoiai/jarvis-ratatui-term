# Registro de mudanças — Investigação baseada em evidências

## 2026-10-20 — Etapa 1: bloquear análises sem leitura de fontes

### Sintoma reproduzido pelo usuário
- Pergunta: `analisa o projeto atual e retorna?`
- Métricas: 132783 ms, `tool executions 0`, `evidence items 0`.
- Resposta afirmava riscos técnicos sem apresentar arquivos/linhas inspecionados.

### Código existente revisado
- `src/investigation.rs`: inventário limitado, baseline de arquivos reais e política de investigação.
- `src/evidence.rs`: observações de ferramentas, cobertura, referências observadas.
- `src/file_tools.rs`: list_files, read_file e search_text com limites de acesso.
- `src/main.rs`: roteamento Solo/Symphony e invocação de ferramentas.

### Alterações
1. Adicionada `require_evidence_for_overview` com teste unitário.
2. `query_provider` agora impede que pedidos amplos de análise de projeto recebam uma resposta sem ao menos uma observação de ferramenta bem-sucedida.
3. Em caso de ausência de evidências, retorna erro explícito orientando conferir `/workspace` e permissões.

### Limitações e próximos passos
- Não comprova a correção de cada alegação do modelo: observação != verificação semântica.
- `len()` conta observações bem-sucedidas, inclusive listagens; a próxima etapa deve exigir pelo menos uma leitura `read_file` de fonte e conferir referências apresentadas.
- O checkout local do Mac pode estar diferente do branch principal no GitHub; comparar com `git status` e `git log` antes de aplicar.
- Executar `cargo fmt --check && cargo check && cargo test` no ambiente local (não executados por esta alteração remota).
- Investigar por que o teste do usuário mostrou zero observações, já que o `baseline()` da versão no GitHub deveria ler arquivos existentes.
- Em seguida: metas de cobertura de módulos, orçamento e estado explícito de investigação, e integração com Knowledge/perguntas de revisão.


## 2026-10-20 — Etapa 2: impedir vazamento de blocos de raciocínio

### Evidência recebida
- Execução Solo em 306153 ms; 14 observações, 13 arquivos-fonte distintos lidos, 0 erros de ferramenta.
- A saída mostrava texto intermediário seguido por `</think>` e só depois a resposta.
- A resposta recomendava configurar `DUDE_TOOL_ROUNDS` apesar de já existir no projeto.

### Implementação
- Novo `src/response_filter.rs`: filtro incremental dos blocos **explicitamente delimitados** por `<think>...</think>`, inclusive quando a tag chega quebrada entre chunks.
- `src/main.rs` passa o streaming pelo filtro antes de publicar `ChatChunk` e antes de acrescentar conteúdo ao histórico; aplica o mesmo tratamento à resposta não-streaming sem tool calls.
- Um bloco `<think>` que não termina gera erro explícito, em vez de publicar raciocínio parcial.
- Testes unitários cobrindo tags fragmentadas, resposta comum e bloco incompleto.

### Limites e validação
- Não reduz os tokens ou a duração da inferência do Ollama; apenas impede exposição de blocos reconhecidos na TUI.
- Pensamento enviado como texto puro, sem delimitadores, não pode ser separado de resposta final com segurança sem alterar o protocolo/modelo.
- Validar no Mac: `cargo fmt --check`, `cargo check`, `cargo test` e reexecutar análise Solo com Qwen.
- Próximo passo: exigir pelo menos uma observação do tipo `read_file` e validar conclusões contra referências (uma lista de arquivos não basta).
\n## 2026-10-20 — Comando para sair\n\n- Adicionados `/exit`, `/quit` e `/sair` no tratamento de Enter da TUI; o loop termina e a rotina existente em `main()` restaura o terminal.\n- Mantidos Esc e Ctrl+C. A atualização também documenta os comandos em `/help` e no README.\n- Validar localmente com `cargo check` e `cargo test`; não executados no Mac pelo conector.\n
## 2026-10-20 — Knowledge não pergunta ao usuário

- Diagnóstico: no Solo, o segundo passe do modelo tenta extrair JSON de candidatos; quando JSON ou chamada falhava, o resultado era ignorado silenciosamente, sem candidato e sem pergunta.
- O DUDE agora informa explicitamente se novos candidatos foram criados, se não houve candidato válido, se a extração falhou ou se houve erro de gravação.
- Candidatos pendentes continuam sendo apresentados para aprovação com o mecanismo existente `ask_next_knowledge()`; nenhuma memória é aprovada sem consentimento.
- Pendente: melhorar robustez da extração em modelos locais, evitar segundo passe lento quando desnecessário, testar `cargo check` e `cargo test` localmente.

## 2026-10-20 — Etapa 3: Laravel funcional e garantia de leitura real

- Análise do IPTV Laravel expôs baseline superficial: 4 fontes (`README.md`, `composer.json`, `package.json`, `public/index.php`) em 229488 ms, sem fluxos funcionais.
- `investigation::baseline` passa a incluir rotas, bootstrap, configuração de banco, controllers, models, services e teste representativo, quando existentes, mantendo o limite de amostragem e ferramentas read-only.
- Arquivos Laravel funcionais recebem prioridade na composição do contexto limitado.
- Teste unitário Laravel valida presença de conteúdo de rota/controller e evidência de linha.
- Nova `EvidenceCache::source_read_count()` distingue leitura bem-sucedida de simples inventário; gate de análise ampla usa leituras reais.
- Memória: `history.jsonl` registra conversas; `memory.md` não recebe candidatos sem aprovação; `knowledge_candidates.json` armazena propostas pendentes. Não há prova de aprovação/persistência de Knowledge nos logs do IPTV. Não declarar que memórias factuais já foram salvas.
- Pendente: testes `cargo fmt --check`, `cargo check`, `cargo test` e execução end-to-end no Mac; investigação de custo do segundo passe de extração; seleção dinâmica de módulos por relevância.

## 2026-10-20 — Separação de memória operacional e aprendizado

- **Memória operacional bruta:** arquivo `.dude/journal/YYYY-MM-DD.jsonl` (data UTC), com timestamp e cada troca concluída, sem sumarização ou aprovação de fatos.
- Escrita append-only seguida de `sync_data()` a cada troca concluída; não depende do extrator de Knowledge.
- **Histórico de contexto:** `.dude/history.jsonl` continua com conversas recentes recuperáveis.
- **Aprendizado curado:** candidatos em `.dude/knowledge_candidates.json` e fatos aprovados em `.dude/memory.md`, sempre sob revisão do usuário.
- Teste adicionando verificação de persistência do journal após reabertura, sem promoção automática para memory.md.
- **Limites:** ainda não existe temporizador/autosave horário independente; a gravação acontece após respostas concluídas enquanto o programa roda. Não registra chunks de respostas incompletas nem tarefas interrompidas; `sync_data` não equivale a backup externo.
- `chrono` adicionado ao Cargo.toml; atualizar Cargo.lock localmente e validar build/testes.
