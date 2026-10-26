# DUDE — Handoff para continuar no VS Code

> Documento de transição para o próximo agente (ChatGPT/Codex no VS Code).
> Preparado a partir de um transcript real do DUDE enviado pelo usuário e da inspeção do código na branch do PR #32. NÃO interpretar o pensamento narrado pelo modelo como log de ferramentas ou evidência de execução.

## 1. Contexto imediato e objetivo

- Repositório: `marcoiai/jarvis-ratatui-term` (DUDE, Rust + Ratatui + Ollama).
- Branch de trabalho: `fix/technical-followup-evidence-gate` — PR #32, aberto contra `main`: https://github.com/marcoiai/jarvis-ratatui-term/pull/32.
- Head consultado ao atualizar este handoff: `cbec2c52d7e289f5834d018e455bb91fc79ef25f` (revalidar com git rev-parse HEAD no VS Code).
- O usuário quer um assistente de engenharia que entenda a conversa, decida **por modelo** quando usar ferramentas read-only, leia código de verdade e justifique conclusões com arquivos/linhas. Não quer roteamento principal baseado apenas em palavras-chave.
- Configuração de teste: Ollama local/remoto, no último transcript `qwen3:4b`; modelo do terminal pode ser outro conforme `OLLAMA_MODEL`/`/model`. Uso comum: DUDE no Linux e Ollama servido pelo Mac.
- Prioridade atual: diagnóstico de qualidade, precisão das evidências e continuidade contextual; **não** focar agora em SFT, feedback ou otimizar/desativar o thinking. O usuário quer thinking e streaming visíveis por enquanto.
- Objetivo deste handoff: dar ao agente no VS Code um roteiro implementável, com testes de aceitação, para corrigir a falsa impressão de investigação completa e a mistura de conceitos de evidência.

## 2. Evidência do teste real anexado

Fonte: `Texto colado(20261024-015121).txt`, arquivo disponibilizado pelo usuário na conversa (não está versionado no repositório). Este handoff resume observações, não embute o raciocínio inteiro.

### Caso A — conversa social em inglês

- Prompt: `hey dude`.
- DUDE respondeu `Hey! How can I help? 😊`.
- Métricas: `solo total 16452ms`, `tool executions 0`, `messages_14`, `payload_bytes_8511`, `message_bytes_7529`, `tool_bytes_886`, `input 2060`, `output 501`, `stream_true`, `think_true`.
- Thinking mencionou a conversa anterior e o pedido `you can speak english`; a resposta final em inglês ficou coerente com esse teste.
- O modelo gastou bastante raciocínio repetindo decisões triviais de linguagem, mas **não alterar por enquanto** o modo de thinking por decisão do usuário.

### Caso B — revisão do projeto em inglês

- Prompt: `review current project`.
- Thinking recitou o README, Cargo.toml e uma lista de módulos (`main.rs`, `router.rs`, `file_tools.rs`, `memory.rs`, `evidence.rs` etc.), e escreveu que iria responder em português porque o alinhamento dizia `language=pt-BR`.
- A resposta final saiu em **português**, embora o prompt fosse inglês e o usuário estivesse testando comunicação em inglês.
- Apresentou uma enumeração de características, não um review do fluxo real de execução, falhas, trade-offs, módulos não verificados ou testes. Nenhuma citação precisa `arquivo:linha` na resposta.
- Afirmou genericamente que `Cargo.toml`, `README.md` e 10 módulos haviam sido 'verificados'. **Não é possível deduzir só deste texto se cada módulo foi lido por tool call do modelo**, porque o bootstrap injeta excertos automaticamente; verificar os eventos de ferramentas e as métricas da rodada.
- Auditoria semântica exibiu `{"claims": []}` e corretamente informou que isso não equivale à aprovação global. Para uma resposta com diversas afirmações técnicas, a auditoria vazia é de utilidade baixa e deve ser tratada como inconclusiva, não como validação.
- O transcript foi exportado **antes** da linha final de métricas desse segundo pedido. Não inventar tempo total, token count ou número de tool calls para esse caso.

### Erro conceitual confirmado no conteúdo

- O modelo escreveu 'cache de evidências limitado a 150 entradas (32 KB cada)' e atribuiu esses números tanto a `file_tools.rs` quanto a `evidence.rs`.
- No código atual: `src/file_tools.rs` declara `MAX_ENTRIES = 150` e `MAX_BYTES = 32768`, limites da operação de ferramentas (listagem/leitura); **não** são os limites de armazenamento do cache de evidências.
- Em `src/evidence.rs`, inserção em cache depende de `cache.len() < 80` e `result.len() <= 16000` (bytes); os registros de observação têm limite separado (`records.len() < 100`). Afirmar que 'cache de evidências = 150 itens de 32 KB' é misturar subsistemas.
- Também afirmou 'não permite execução de comandos shell', frase ambígua: o DUDE tem `!command` que executa programas externos, mas **sem interpretação por shell** (pipes/globbing/redirecionamento não funcionam como num shell). Não descrever isso como impossibilidade absoluta de executar comandos externos ou como sandbox de segurança.
- Não assumir segurança plena: bloqueios de caminhos sensíveis não tornam o runtime um sandbox. Conferir implementações e limitações antes de afirmar proteção.

## 2B. Segundo transcript: aprofundamento não progride apesar das tools (24/10/2026)

Fonte: arquivo de log anexado na conversa, Texto colado(20261024-020823).txt. Transcript integral não versionado, por privacidade.

### Teste C: "whats up"
- Resposta em inglês, curta e social. 26.693 ms, zero tools, zero arquivos lidos, input 1.457 e output 960 tokens. Thinking muito extenso para o resultado, mas NÃO desligar thinking, conforme decisão do usuário.

### Teste D: "can you review curret project?"
- Resposta de visão geral em inglês. 86.377 ms: 55.027 ms da rodada Ollama e 31.277 ms de auditoria semântica; 14 execuções de tools; 13 arquivos-fonte lidos. Tokens: input 9.525, output 1.114.
- Declara incorretamente que o cache de evidências em src/evidence.rs possui 150 entradas de 32 KB. Realidade: src/file_tools.rs define MAX_ENTRIES=150 e MAX_BYTES=32768 para limites de ferramentas; src/evidence.rs armazena itens em cache somente se cache.len()<80 e result.len()<=16000 bytes. Há ainda limite próprio de 100 registros de observação.
- Resposta termina com "No unverified claims made. This is a verified snapshot" apesar de conter erro factual e cobertura amostral. A autovalidação textual do LLM não é evidência.
- O avaliador semântico usa referências amplas (README.md:1-120, src/main.rs:1-100), que podem estar presentes na coleta sem sustentar a afirmação individual.
- Contradição do auditor: marca como unsupported a frase "DUDE does not have a secure sandbox", ao mesmo tempo em que justifica com a declaração no README de que o DUDE NÃO é um sandbox. Também confunde .gitignore com impossibilidade técnica de compartilhar.
- Conhecimento opcional em segundo plano retornou timeout de 35 s. Não contabilizar esses 35 segundos como parte dos 86.377 ms da resposta principal.

### Teste E: "ok, can you review more?" — P0
- O Qwen reconheceu que deveria usar list_files e read_file, mas gastou longo raciocínio narrando os próximos passos e até emitiu pseudo XML de tool calls e JSON dentro do thinking.
- ATENÇÃO: textos de tool_call no thinking NÃO demonstram execução real. Só message.tool_calls nativos processados pelo runtime e seus eventos de execução contam.
- Métricas verificáveis do turno: total 196.784 ms; 4 rodadas Ollama de 55.019, 14.835, 66.208, 60.689 ms; input 13.429/output 4.609 tokens; 3 tools executadas; 3 evidence items; **apenas 1 source read, src/main.rs**. Tool errors 0.
- A resposta inventou "Source reads: 14" e listou 13 arquivos anteriores + diretório src/ como se fossem evidência atual. Contradiz o relatório do runtime: Source reads: 1, Files: src/main.rs. Distinguir leituras do turno, leituras herdadas da conversa e inventário.
- Exibiu como conclusões "verificadas" tanto "Workspace seguro" como propriedades de outros arquivos que admitiu não ter revisado. Leitura de configuração de workspace não prova segurança integral.
- Ao invés de aprofundar e concluir a análise, sugeriu ao usuário que verificasse src/file_tools.rs e src/memory.rs. O DUDE tinha acesso read-only e deveria investigar ele próprio.
- O segundo turno não exibiu auditoria semântica apesar de revisar código: hipótese forte a testar é que src/investigation.rs::needs_baseline exige padrões como "review"+"project" e não reconhece o follow-up curto "review more"; o gate pós-auditoria de src/main.rs::query_provider depende desse resultado. Não transformar conversa social em revisão técnica por palavras soltas.

### Plano de correção e testes de regressão, em ordem

**P0 — Integridade do tool calling e seguimento contextual**
1. Inspecionar src/main.rs::{ask_ollama,read_ollama_tool_stream} com fixture NDJSON contendo pseudo tool-call em message.thinking: nenhum efeito de ferramenta deve ocorrer sem message.tool_calls estruturados.
2. Testar JSON de argumentos de tools em múltiplos fragmentos NDJSON; contar corretamente list_files e read_file e as evidências coletadas. Verificar nos eventos, e não nas alegações do modelo, se o uso foi efetivo.
3. Reproduzir sequência review current project -> ok, can you review more?; exigir continuidade da intenção técnica e novas leituras de código, ou declaração factual de impossibilidade. Se o modelo optar por não usar tools, a resposta não pode afirmar que reviu código novo.
4. Distinguir explicitamente nos dados runtime: fontes do bootstrap, leituras pedidas por tools nativas do modelo, inventário, leituras do turno e leituras históricas. As alegações numéricas da resposta devem ser consistentes com esses dados.
5. Garantir que revisão técnica de follow-up recebe o mesmo nível de evidência e auditoria de uma primeira revisão. Não substituir decisão do modelo por um detector rígido de palavras.

**P1 — Verificação semântica confiável**
6. Separar factualidade de "arquivo/linha observado": uma referência README.md:1-120 lida não prova que toda alegação de segurança, sandbox ou API esteja correta. Preferir citações pontuais e contexto da linha.
7. Diferenciar not_assessable por falta de observação, unsupported por contradição real, e partially_supported por escopo incompleto. O exemplo "não é sandbox" não pode ser rejeitado usando a própria frase que confirma essa limitação.
8. Criar teste de alegação falsa sobre cache 150/32KB vs 80/16KB, com evidência separada de src/file_tools.rs e src/evidence.rs.
9. Restringir frases absolutas como "verified snapshot" e "No unverified claims" a avaliações de runtime; a auditoria posterior não pode legitimar trecho já transmitido como verificado. Estudar status "provisório/não validado" durante streaming de revisão até haver evidências.
10. Exigir que "review more" reporte valor incremental (arquivos adicionais realmente lidos, novo fluxo analisado, achados novos e lacunas), não apenas resuma o README e recomende leituras que o DUDE poderia fazer.

**P2 — Telemetria e experiência**
11. Manter thinking e resposta em streaming, /export e os estágios distintos da TUI. Para avaliar latência: teste C 26,7 s, D 86,4 s (auditoria 31,3 s), E 196,8 s (4 rodadas, apenas 1 arquivo-fonte lido).
12. Testes Rust obrigatórios: texto pseudo-tool no thinking vs chamadas reais; argumentos fragmentados; follow-up que herda intenção; divergência source_reads no texto vs métricas; auditor com referência válida porém alegação falsa; erros de classificação de sandbox; saída em inglês após comando explícito de idioma.

### Critérios adicionais de aceitação
- Em revisões, fatos citados têm correspondência REAL com arquivo/linhas observados e status correto. Nenhuma leitura ou número histórico aparece como leitura nova do turno.
- Pensamento narrativo e pseudo-XML em thinking não viram tool calls nem evidência.
- Pedido de aprofundamento abre novos trechos relevantes ou relata explicitamente por que não conseguiu fazê-lo. Não retorna somente "abra você os arquivos".
- A auditoria diferencia ausência de evidência e contradição, e não transforma README em certificado de segurança.
- Thinking/streaming continuam ligados até nova decisão explícita do usuário.

---

## 3. Causa provável e pontos exatos de inspeção

1. `src/investigation.rs::needs_baseline` detecta pedidos amplos por padrões (ex.: `review` + `project`). `baseline` lê trechos de arquivos via `evidence.execute` e prepara até ~26K caracteres de excertos. Isso oferece dados ao Qwen **sem que ele necessariamente faça chamadas adicionais por conta própria**.
2. `src/main.rs::query_provider` injeta o baseline como **nova mensagem `role: user` depois da pergunta verdadeira**. A última mensagem do prompt pode passar a parecer um pedido/instrução do bootstrap, confundir o modelo sobre o pedido ativo, idioma e o que já investigou. Essa é uma hipótese de código para testar, não uma prova de causalidade.
3. `src/main.rs::ask_ollama` fornece tools em todas as rodadas autorizadas e permite que o modelo escolha. Preservar esse desenho; NÃO voltar a restringir calls por uma lista de palavras. Verificar `read_ollama_tool_stream` e a associação entre decisões, chamadas executadas e respostas finais.
4. `src/evidence.rs` mantém separação entre `observations`, `events`, `cache` e `source_read_count`. Tornar essa distinção explícita para o modelo e no relatório final, incluindo origem das leituras (`bootstrap automático` vs `tool call solicitado pelo modelo`) se necessário.
5. `src/claim_audit.rs::semantic_audit` aceita `claims: []` como resultado válido. Isso é aceitável se não houver afirmações verificáveis, mas não deve virar selo implícito de qualidade para um review com afirmações concretas.
6. `src/alignment.rs::Profile::instructions` usa `language=pt-BR` mesmo após pedido anterior em inglês; verificar a semântica de preferência persistida vs pedido explícito/turno atual. Não mascarar escolha de idioma com heurísticas ad hoc de tradução.
7. `src/main.rs::ChatChunk` transmite conteúdo de geração em tempo real. Qualquer novo gate de evidência precisa levar em conta que texto potencialmente não verificado já pode ter sido exibido ao usuário antes da validação posterior.

## 4. Alterações recentes já presentes no PR #32 — não refazer às cegas

- Streaming de `message.thinking` e `message.content`; Qwen3 usa `think: true` por padrão, override `DUDE_OLLAMA_THINK=0|1`.
- Filtro de streaming corrigido para não acumular toda resposta simples até `done=true`.
- Compactação de contexto: últimas 10 mensagens de diálogo, memória persistente truncada a 1800 caracteres enviados ao modelo; nada disso apaga histórico local.
- Modelo decide chamadas read-only (sem gate de tools por palavras-chave); heurísticas ainda existem para baseline e checagens pós-resposta.
- `/export` cria `.dude/exports/session-*.txt`; ao completar resposta, salva `.dude/exports/latest.txt`.
- Atualização de estados da TUI: `Raciocinando`, `Investigando arquivos`, `Auditando evidências`.
- Extração de conhecimento após resposta principal é opcional e não bloqueia o fim da interação, com timeout auxiliar de 35s; não promover conhecimento inferido a fato automaticamente.
- `evidence.observed_reference` valida intervalos `arquivo:inicio-fim` somente se **todas** as linhas citadas tiverem sido observadas.
- Até o momento deste handoff, `cargo check`, `cargo test` e testes reais TUI/Ollama **não foram validados por este agente**. Conferir antes de merge; PR está aberto.

## 5. Ordem de trabalho para o agente no VS Code

### P0 — reproduzir sem alterar comportamento

1. `git fetch origin && git switch fix/technical-followup-evidence-gate && git pull --ff-only` (se houver mudanças locais, preservar e não sobrescrever).
2. `cargo fmt --check && cargo check && cargo test` e corrigir **erros de compilação/regressões** antes de mexer na arquitetura.
3. Executar `OLLAMA_MODEL=qwen3:4b cargo run --release` apontando para o Ollama configurado. Capturar `DUDE metrics`, `.dude/metrics.jsonl`, `.dude/exports/latest.txt` e `.dude/progress*` quando disponíveis, sem expor secrets.
4. Testar em sequência: `hey dude` -> `you can speak english` -> `hey dude` -> `review current project`. Capturar estado de idioma, calls reais e tempos por estágio.

### P1 — corrigir fidelidade da investigação

1. Revisar como `baseline` é representado no prompt: dado de referência **não** deve aparecer como nova solicitação do usuário nem sobrescrever a intenção/idioma do último turno. Preferir mensagem de contexto claramente identificada, sem promover conteúdo de arquivos a instrução.
2. Garantir que reviews globais distingam: inventário de paths, excertos do bootstrap, leituras adicionais por ferramenta, trechos insuficientes, conclusões e lacunas. Se houver leitura automática, dizer isso explicitamente. Nunca declarar 'todos os arquivos verificados' com leitura amostral.
3. Corrigir mistura de limites (file_tools vs evidence) com teste de integração que verifica a resposta contra evidência observada, sem hard-code do texto da resposta. O modelo deve citar definições com `src/file_tools.rs:linha` e `src/evidence.rs:linha` separadamente.
4. Dar preferência a análise do fluxo real (`main -> submit -> route -> query_provider -> ask_ollama -> tools -> audit -> telemetry`), decisões, riscos e testes em vez de repetir índice do README. Uma revisão mínima precisa registrar escopo de leitura, achados justificáveis e lacunas.
5. Fazer a auditoria retornar veredito **inconclusivo/sem afirmações avaliadas** quando `claims=[]`; não exibir isso como aprovação. Verificar também as citações que o avaliador propõe contra `observed_reference` e limitar claims a observações sustentadas.

### P2 — idioma e experiência

1. Reproduzir inglês -> inglês na resposta `review current project`. A preferência persistida `pt-BR` deve continuar disponível, mas um pedido explícito atual de idioma deve prevalecer quando apropriado; diferenciar preferência para sessão de pergunta pontual.
2. Thinking e streaming continuam ligados durante os testes. Evitar mexer em esforço de raciocínio, idioma interno de thinking ou treinar o modelo neste ciclo.
3. Conferir `/export` e o status da UI em conversa casual, investigação, auditoria semântica e extração opcional. Avaliador e knowledge não devem parecer que o modelo principal ainda está pensando.

## 6. Critérios objetivos de aceitação

- Compilação, formatação e testes Rust passam, sem alterar a política read-only das ferramentas nem o comportamento de persistência validado.
- `hey dude` não faz leitura de projeto e responde brevemente, salvo pedido explícito de análise.
- `review current project` apresenta análise com **fontes precisas e reais**, distingue bootstrap de investigação ativa, não afirma verificação completa nem usa caminhos/linhas inexistentes.
- Reconhece a diferença entre limites do `file_tools` e do `EvidenceCache`; não chama operações externas de impossíveis apenas porque `!command` não usa interpretação de shell.
- Auditoria `claims=[]` aparece como **não conclusiva**, e nenhuma evidência inexistente é transformada em `supported`.
- Em teste com idioma inglês explícito, o resultado final respeita o inglês ou explica claramente por que não; não repetir traduções/metadiscussão de idioma na resposta final.
- Streaming de raciocínio e resposta continua funcional; o usuário consegue salvar o transcript por `/export` e consultar métricas de forma separada por etapa.

## 7. Instrução ao próximo agente

Comece **lendo este handoff, o transcript exportado se disponível, o PR #32 e as funções citadas**. Rode testes primeiro. Produza correções pequenas com teste de regressão por falha; cite arquivos e linhas realmente inspecionados, não deduza sucesso por output do modelo. Preserve o trabalho existente e **não faça merge** sem o usuário pedir.

### Comandos úteis

~~~bash
git fetch origin
git switch fix/technical-followup-evidence-gate
git pull --ff-only
cargo fmt --check
cargo check
cargo test
OLLAMA_MODEL=qwen3:4b cargo run --release
~~~

Este documento é um handoff de trabalho, **não uma confirmação de que as correções ainda pendentes foram implementadas ou testadas**.
