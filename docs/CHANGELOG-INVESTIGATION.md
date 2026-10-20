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
