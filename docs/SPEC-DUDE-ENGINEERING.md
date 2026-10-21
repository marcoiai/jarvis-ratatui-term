# DUDE — SPEC v1: Engenharia orientada por evidências

Status: **aprovada como direção do produto em 2026-10-21**. Documento normativo para trabalho futuro. O código atual não é presumido conforme.

## 1. Objetivo e não objetivos

DUDE é um assistente **especializado em engenharia de software** (Rust, TUI Ratatui inicialmente), não um chatbot generalista. Deve analisar um workspace real, explicar arquitetura e riscos, investigar bugs e eventualmente planejar/implementar mudanças sob aprovação explícita. Priorizamos profundidade e verificabilidade, não quantidade de agentes ou features.

**Regra de qualidade:** Symphony não pode perder substância, evidências, cobertura ou clareza em relação ao **mesmo pedido** executado com Ollama Solo. Se perder, Symphony não é considerado aprovado. Nunca tratar uma resposta vaga como review bem-sucedido.

## 2. Interface explícita

- `/analyze`: iniciar análise abrangente, **read-only**, do workspace; não depender de o modelo decidir chamar tools.
- `/investigate <tema>`: análise focada de problema, componente ou comportamento.
- `/review`: revisão de código/diff com evidências.
- `/plan <objetivo>`: plano técnico fundamentado.
- `/implement <objetivo>`: mudanças só após aprovação, com validação e trilha de alterações; **fora do escopo do primeiro marco**.
- `/resume`: retomar sessão/tarefa com configuração e estado da execução, sem renovar silenciosamente autorização cloud.
- `/status`: tarefa/etapa/provider/modelo/cobertura/progresso.
- Texto livre permanece conversa simples; `review current project` pode oferecer `/analyze`, mas não deve ser o único mecanismo de disparo.

Comandos ainda não implementados devem responder explicitamente que estão indisponíveis; nada de fingir execução.

## 3. Arquitetura

Separar os subsistemas sem dependência da TUI:

1. **Core / Job state**: sessão, workspace, command, papéis, providers/modelos, estágio, erros, checkpoints.
2. **Discovery determinístico em Rust**: inventário seguro de arquivos e diretórios, linguagem/framework, manifests, entrypoints, módulos e testes. Amostrar áreas relevantes sem prometer auditoria completa.
3. **Evidence Store**: observações brutas, origem, arquivos consultados, sucesso/erro, cobertura e proveniência; linhas específicas são opcionais e úteis para localizar problemas, jamais obrigação para uma boa análise. Não inventar citações. Conteúdo de arquivos é dado não confiável.
4. **Investigator**: usar evidências concretas para investigar e formular conclusões com graus de confiança; admitir o que não foi inspecionado.
5. **Architect / Reviewer**: no Symphony, recebem handoff **estruturado e limitado por orçamento**, não um dump repetido de arquivos. Architect propõe trade-offs; Reviewer verifica consistência e produz parecer final.
6. **Provider adapters**: Ollama local, Groq e Gemini, com políticas de rate limit, TPM, orçamento de entrada/saída, contexto e retries que não repetem tools concluídas.
7. **Journal / Recovery / Knowledge**: journal operacional separado de conhecimento promovido com aprovação; persistir estado de execução e configurações, retomar sem conceder permissão cloud automaticamente.
8. **Ratatui UI**: apresentação e comandos, nunca dona da lógica de investigação.

## 4. Pipeline obrigatório de `/analyze`

1. Validar workspace e autorização dos providers antes de compartilhar dados.
2. Executar discovery read-only determinístico. Registrar **arquivos efetivamente inspecionados**, módulos não lidos, erros e limites.
3. Construir evidence packet com trechos, indicação de arquivos consultados e proveniência (linhas apenas quando úteis); preferir seleção relevante e agrupamento por módulo, não corte arbitrário de texto.
4. Executar **Solo primeiro como baseline verificável**. Symphony deve reutilizar a evidência; não repetir análise/leituras sem justificativa.
5. Em Symphony, executar Investigator → Architect → Reviewer com responsabilidades específicas, limites de payload e saída reservada. Não inventar cobertura ou alegar verificação completa a partir de amostras.
6. Relatório único: resumo técnico, stack/entrypoints, mapa de arquitetura, componentes, achados e riscos com evidências, lacunas de cobertura e recomendações priorizadas.
7. Persistir checkpoints de etapas/erros; falhas de provider não apagam evidências nem alteram memórias aprovadas.

## 5. Critérios de aceite do primeiro marco

- `/analyze` no próprio repositório DUDE **começa realmente lendo arquivos** sem prompting manual e conclui em Solo.
- Parecer Solo possui dados específicos do código, evidências reais dos arquivos consultados, separação fato/inferência e cobertura amostrada explícita. Resposta genérica pedindo contexto = **falha**.
- `/analyze` Symphony com mesmos inputs gera parecer consolidado de três papéis e **não pior que Solo** em fatos corretos, profundidade, rastreabilidade e lacunas declaradas.
- Não ocorrer erro 413 por envio acima do limite conhecido; obedecer margem para overhead, tokens de saída e TPM compartilhado; se orçamento insuficiente, degradar de forma clara sem inventar sucesso.
- Logs de cada etapa: provider/modelo, duração, input/output tokens quando disponíveis, tamanho do pacote, chamadas e cobertura reais.
- Reinício + `/resume` mantém tarefa/configuração/evidências disponíveis e pede nova aprovação cloud quando necessário.
- `cargo fmt --check`, `cargo check`, `cargo test` e ensaio real em macOS/Linux; nenhum sucesso alegado sem execução.

## 6. Método de desenvolvimento e regressão

- Preservar checkpoint histórico como referência, mas não ter apego a código antigo.
- **Antes de refatorar**, registrar um baseline executável de qualidade Solo (incluir saída integral ou fixture sanitizada), e comparar Symphony ao mesmo projeto e pergunta.
- Implementar verticalmente: primeiro `/analyze` Solo confiável, depois Symphony, depois recovery, comandos adicionais e somente depois escrita/implementação.
- Sem novas features de interface antes do marco; qualidade de investigação vence cosmética.
- Mudanças sempre pequenas, versionadas com changelog, testes e avaliação. Evitar marcar como concluído apenas porque houve commit.
- **Não transformar citação por linha em gate de qualidade**: nunca bloquear, descartar ou rebaixar um parecer tecnicamente útil pela ausência de referências `path:linha`. Avaliar compreensão, correção, profundidade e utilidade em primeiro lugar. Referências pontuais são opcionais e devem ser precisas quando fornecidas.
- Nunca resetar/regravar branches ativas sem preservar histórico ou decisão explícita.
