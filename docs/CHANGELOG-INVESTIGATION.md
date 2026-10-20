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

## 2026-10-20 — Etapa 4: checkpoint e recuperação de pedidos

- `Memory::save_pending`: grava o pedido antes do dispatch em `.dude/pending_request.json` com timestamp UTC e escrita temporária seguida de rename; o pedido fica separado de Knowledge e do diário bruto.
- `Memory::pending` / `clear_pending`: consulta e descarte do pedido pendente, com bloqueio de arquivos symlink.
- Startup alerta sobre pedido interrompido; `/recover` carrega o texto na entrada para revisão e reenvio manual, `/recover discard` descarta.
- Em resposta concluída, limpa o marcador após gravar histórico/journal; falhas mantêm o pedido disponível para nova tentativa.
- Teste verifica recuperação através de nova instância de Memory e limpeza explícita.
- Limitações: não salva chunks nem execuções parciais; não tem autosave horário; novo pedido substitui o marcador anterior; escrita em disco não equivale a backup externo. Validar `cargo fmt`, `cargo check` e `cargo test` no ambiente local.

## 2026-10-20 — Etapa 5: progresso da investigação em disco

- Novo módulo `src/progress.rs`, que grava eventos de ferramentas em `.dude/investigation_progress.jsonl` com timestamp UTC, nome da ferramenta, caminho, resultado, cache hit e intervalo de linhas.
- `EvidenceCache::execute` registra progresso tanto para execução normal quanto para leitura via cache.
- Metadados apenas: nenhum conteúdo de arquivo, expressão de busca ou resposta do modelo no journal de progresso; Knowledge continua separado.
- Cada evento faz append e `sync_data`; teste unitário verifica persistência dos metadados e ausência do padrão de busca.
- **Limite:** o journal é histórico append-only e ainda não é consumido para retomar a sequência de ferramentas automaticamente. A recuperação do pedido continua usando `/recover`. O estado em memória das evidências não é reconstruído. Erros de gravação do progresso não interrompem a investigação (best effort); ainda falta feedback operacional e compactação por sessão. Testes locais pendentes.

## 2026-10-20 — Etapa 6: aviso conservador de percentuais de cobertura

- Novo módulo `src/claim_audit.rs` sinaliza alegações numéricas de cobertura associadas a porcentagens e termos como cobertura/componentes/fluxos. Inclui testes positivos e negativos.
- Resposta final do Ollama mantém o texto gerado, mas recebe alerta explícito caso uma porcentagem de cobertura seja citada sem medição do runtime. O alerta também é transmitido à TUI.
- Esta checagem NÃO valida semanticamente afirmações, caminhos nem citações; é um primeiro alerta determinístico e limitado. Evitar declarar parecer inteiramente validado.
- Identidade visual e cabeçalho **DUDE**, incluindo nome e demais identificações existentes, preservados; mudanças limitadas à análise e à mensagem de auditoria.
- Pendente: compilação e testes locais, filtro mais robusto de `<think>` e validação completa de referências de origem.

## 2026-10-20 — Cabeçalho DUDE fixo na TUI

- Identificação `D.U.D.E. — Demented Unified Development Entity` e subtítulo Enova/IA local separados do texto rolável, em duas linhas permanentes no topo.
- A rolagem vertical afeta apenas o painel de respostas; `/clear` limpa somente o histórico visual, não o cabeçalho.
- O texto de inicialização permanece no histórico inicial, sem duplicar o nome/subtítulo que agora são renderizados separadamente.
- Painéis de entrada, ambiente e ajuda reposicionados sob o cabeçalho fixo.
- Compilação e teste visual local pendentes.

## 2026-10-20 — Correção do banner fixo completo

- Diagnóstico após teste do usuário: versão anterior fixava somente nome/subtítulo; demais informações de inicialização continuavam em `app.output` e desapareciam com rolagem ou `/clear`.
- `App::banner()` agora contém também sanidade, diagnóstico NOVVS, café, aviso de aprovação e saudação, em cinco linhas permanentes.
- `draw()` reserva cinco linhas independentes para o banner; a conversa permanece em outro painel com rolagem.
- Textos duplicados foram removidos da mensagem inicial do transcript.
- Observação: a cópia local apresentava 17 arquivos modificados; preservar diferenças antes de atualizar remoto. Teste visual local continua pendente.

## 2026-10-20 — Posição original do cabeçalho fixo

- Correção após esclarecimento do usuário: a apresentação deve ficar **abaixo do campo de entrada**, no início da área de resposta, não acima de todos os painéis.
- Layout reordenado para `input → banner DUDE fixo → conversa rolável → ambiente → ajuda`.
- `/clear` continua limpando somente a conversa e a rolagem não move a apresentação.
- Teste visual local ainda necessário.

## 2026-10-20 — Filtro do raciocínio exposto (streaming)

- Reproduzido pelos logs: modelos Ollama podem produzir longa deliberação em `message.content` sem `<think>` inicial, terminando apenas em `</think>`. O filtro anterior emitia o texto imediatamente, antes de saber que era raciocínio.
- O filtro agora retém saída não delimitada até encontrar `</think>` ou encerrar a resposta; respostas com `<think>` inicial continuam descartando o bloco interno. A leitura final não-streaming usa o mesmo filtro.
- `ThinkingFilter::push` passa a retornar `Result` para sinalizar buffer de conteúdo indefinido acima de 1 MB; integração Ollama propaga a falha.
- Testes cobrem fechamento fragmentado, abertura normal, resposta sem tags e limite do buffer.
- **Trade-off:** respostas normais sem `<think>` agora aparecem somente ao final (streaming visual atrasado); a prioridade é impedir vazamento do raciocínio. O processamento da geração continua acontecendo.
- A disposição da apresentação DUDE abaixo da entrada não foi alterada.
- Ainda precisa validar `cargo fmt --check`, `cargo check`, `cargo test` e teste funcional no Mac.

## 2026-10-20 — Solo: extração interna de Knowledge sem streaming

- Teste Solo: 13 arquivos lidos em 14 ferramentas, parecer conciso distinguindo pontos verificados e lacunas; nenhuma porcentagem de cobertura inventada.
- Após a análise, a segunda chamada ao Ollama para sugerir Knowledge falhou com `Ollama stream error: error decoding response body`. Nenhuma memória curada foi salva.
- Em `ask_ollama`, a resposta final só usa streaming quando existe consumidor de `ChatChunk` (`updates.is_some()`). Chamadas internas, como extração JSON de Knowledge, passam a usar resposta completa não-streaming.
- A correção evita processamento de chunks desnecessários nesse segundo passe, mas **não prova** que a falha de transporte do Ollama esteja eliminada; validar novamente no Mac. Nenhum candidato é salvo sem parsing e validação de evidências.
- Não altera o cabeçalho fixo, a análise Solo nem a necessidade de aprovação explícita de Knowledge.

## 2026-10-20 — Auditoria leve de referências nos pareceres Solo

- `EvidenceCache::has_supported_citation` verifica se ao menos uma referência `arquivo:linha` no parecer corresponde a uma linha realmente devolvida por `read_file` na investigação.
- Em pedidos de visão geral de projeto, o DUDE sinaliza pareceres sem referência confirmada como **síntese preliminar**; não bloqueia a análise nem a substitui por uma resposta genérica.
- O aviso chega ao streaming da TUI e à resposta gravada em memória operacional.
- Teste de referências ampliado para citações aceitas, linhas não lidas e respostas sem citações.
- Limitação importante: encontrar ao menos uma referência válida **não valida todas as afirmações**; a auditoria semântica por afirmação ainda está pendente. `cargo check` e `cargo test` precisam ser executados no Mac.

## 2026-10-20 — Wordmark ASCII Enova na apresentação fixa

- `App::banner()` ganhou wordmark tipográfico ASCII inspirado no nome ENOVA (não é reprodução do logotipo oficial).
- Preservada identificação `D.U.D.E. — Demented Unified Development Entity`, textos de status, idioma pt-BR/inglês e posição abaixo do campo de entrada.
- Reservadas 9 linhas para o painel fixo; rolagem e `/clear` continuam restritos à conversa.
- A alteração foi feita remotamente no GitHub; `cargo fmt --check`, `cargo check`, `cargo test` e teste visual em diferentes larguras de terminal ainda pendentes.

## 2026-10-20 — Ollama: timeout de inferência longa no Solo

- Um parecer profundo leu 13 arquivos/14 ferramentas sem falhas, mas a requisição ao `/api/chat` encerrou próximo de 180 s (`error sending request`), coincidindo com o timeout global do cliente reqwest.
- Ambos os clientes criados em `run()` passam a usar `connect_timeout(15 s)` e timeout total de 600 s, permitindo inferências locais mais lentas; mensagem de erro agora distingue falha de requisição/timeout de uma indisponibilidade comprovada do servidor.
- Isto **não prova** que todo erro de transporte venha do timeout: Ollama ou rede também podem encerrar conexões. Ainda falta observar logs do servidor e testar no Mac.
- Sem alteração de cabeçalho DUDE nem do fluxo de Knowledge.

## 2026-10-20 — Wordmark ENOVA fornecido e input abaixo

- Substituída a arte anterior pelas sete linhas de ASCII `ENOVA` fornecidas pelo engenheiro, preservando espaçamentos e conteúdo literal.
- O painel fixo comporta 11 linhas: 7 da arte e 4 linhas de identificação/status DUDE.
- A pedido do usuário, reorganizado o layout: **ENOVA + DUDE (fixo) → campo de entrada → respostas roláveis → status → ajuda**.
- `/clear` continua limpando somente o histórico visual. Verificação visual, `cargo fmt --check`, `cargo check` e `cargo test` seguem pendentes no Mac.
