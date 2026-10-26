# DUDE Coach v0.2 — integração ao runtime

Branch: feat/dude-coach-runtime-v02. Esta mudança depende do corpus do Coach v0.1.

## Implementado

- O ponto comum de chamada aos provedores (query_provider) injeta um **contrato de ferramentas** em um system message. As ferramentas de IA são atualmente somente leitura (list_files, read_file, search_text), ou não estão habilitadas na chamada.
- Um roteador determinístico seleciona **no máximo dois exemplos sintéticos** da categoria apropriada do arquivo golden_cases.jsonl, embutido no binário com include_str!. Perguntas simples não carregam exemplos; o holdout nunca é injetado.
- O guard de resposta bloqueia **promessas explícitas em primeira pessoa** de aplicar patches, executar testes ou realizar deploy/commit, retornando uma explicação de capacidade. Não afirma que isso valida todas as alegações semanticamente.
- A extração interna de Knowledge (JSON) é excluída para não quebrar o formato esperado; solo e as etapas do Symphony passam pelo mesmo ponto.
- DESATIVAR para diagnóstico: DUDE_COACH=off. O padrão é ligado. A ferramenta !comando, executada diretamente pelo usuário, permanece inalterada.

## Limites e riscos

- **Não há treinamento de pesos.** Esses exemplos apenas condicionam o prompt do modelo.
- O guard é determinístico e conservador: pode haver falsos positivos/negativos linguísticos. A avaliação semântica do DUDE permanece independente.
- O guard só controla respostas geradas neste fluxo. Não prova que um provider remoto recusará solicitações perigosas, nem substitui verificações de autorização no executor.
- A resposta final do Ollama no código atual é obtida sem streaming; a auditoria semântica opcional emite atualizações separadas, não equivalentes a uma resposta já validada.
- A avaliação E001–E012 continua sendo um benchmark do modelo bruto. Para avaliar o runtime, usar os testes Rust e os testes de integração mockados abaixo.
- Dado que o Coach v0.1 é uma dependência de build via include_str!, publicar esta branch sem a pasta training/coach_v01 não compila.

## Validação local recomendada

Na raiz do worktree (não dentro de training/coach_v01):

```bash
cargo fmt --all -- --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

O módulo coach.rs inclui testes de seleção de contexto, holdout isolado, capacidade e negações. consolidation_tests.rs usa um servidor Ollama simulado para verificar que promessas são bloqueadas, que o fluxo de leitura registra evidências e que a extração JSON de Knowledge não é alterada.

**Estes comandos ainda não foram executados no hardware do usuário por esta alteração remota.** Se houver falha, reportar saídas reais antes de considerar concluído.


## Correção após primeiro teste real (E004)

Com a pergunta sobre aplicar patch em src/main.rs e executar cargo test, o Qwen
perguntou se tinha acesso ao workspace e se deveria pedir permissão para ler,
sem chamar ferramentas. A v0.2 agora:

- Faz uma leitura limitada (até 120 linhas) **se** o pedido de mutação nomeia
  explicitamente um arquivo-fonte relativo e a sessão tem ferramentas de leitura.
  Toda resolução e bloqueio de caminhos sensíveis continuam no file_tools existente.
- Registra a pré-leitura no EvidenceCache e envia o trecho ao modelo como dados
  não confiáveis. Não concede autorização de escrita, shell ou execução.
- Corrige deterministicamente respostas que ficam pedindo autorização para ler
  arquivos já acessíveis, sem fingir que executou patch/testes.
- Mantém o comportamento normal para perguntas que não envolvem mutação.
- Inclui teste de integração que reproduz a frase problemática, confirma uma
  leitura real autorizada e verifica que a saída não pede permissão outra vez.

Quando o pedido não especifica o que mudar, o DUDE pode pedir a alteração
desejada, mas não deve pedir uma permissão de leitura já concedida.

## Correção após o segundo teste real — auditoria de 13 arquivos (26/10/2026)

O DUDE coletou 13 leituras sem erro, mas a avaliação semântica classificou
quase tudo como não avaliável. Isso expôs duas limitações:

- Referências como `src/main.rs:6-10` eram rejeitadas sem conferir o intervalo.
  O validador agora aceita intervalos até 80 linhas **somente quando todas as
  linhas numeradas foram efetivamente observadas por read_file**; isso não
  comprova semanticamente a afirmação.
- O auditor cortava a exportação de evidências nos primeiros 12 mil caracteres.
  Ele agora recebe uma amostra distribuída entre as fontes lidas, mantendo
  os detalhes completos no EvidenceCache para validar as referências.

O auditor também recebe uma instrução explícita para NÃO tratar imports
de módulos como prova de enumerações implementadas noutros arquivos. Uma
checagem adicional avisa se uma afirmação nomeia `src/router.rs`, mas suas
referências confirmadas só apontam para `src/main.rs`.

Há testes Rust para intervalos incompletos, visibilidade de fontes lidas no fim
da investigação e divergência entre o arquivo afirmado e o arquivo citado.
A auditoria continua assistida por modelo, não é uma prova formal.

A mudança **não reclassifica nem edita** o baseline 10/12 anterior. Não foi
executado cargo test neste ambiente remoto; rodar localmente antes do merge.
