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
