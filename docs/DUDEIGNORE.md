# .dudeignore — exclusões por workspace

Crie **`.dudeignore` na raiz do projeto** (o diretório definido por `DUDE_WORKSPACE` ou `/workspace`). O arquivo utiliza a sintaxe de padrões do **gitignore**: comentários com `#`, `*`, `**`, diretórios com `/`, padrões ancorados e regras de negação com `!`.

Exemplo:

```gitignore
# Dependências ou código gerado
vendor/
dist/
coverage/
*.min.js
*.log

# Exceções específicas
!important.log
```

Aplicação: `workspace_map`, `list_files`, `search_text` e `read_file`. Acesso direto a arquivo excluído gera erro. A exclusão de um diretório impede a visita a seus descendentes (para reincluir um arquivo, seu diretório não pode permanecer excluído). Alterações no arquivo são lidas na próxima execução de ferramenta, sem reiniciar o DUDE.

**Proteções fixas:** segredos e caminhos internos como `.git`, `.dude`, `.env`, `.ssh` continuam bloqueados independentemente do conteúdo de `.dudeignore`. O próprio `.dudeignore` não é exposto como conteúdo de ferramenta.

**Escopo:** não altera o Git nem substitui `.gitignore`; afeta somente a visão que o DUDE tem dos arquivos daquele workspace. Para atualizar dependências de Rust, execute `cargo check` no projeto (o `Cargo.lock` deve acompanhar `Cargo.toml`, quando versionado).

**Validação:** executar `cargo test` e comparar `workspace_map`, `search_text`, `read_file` em um workspace com e sem regras. Ainda não foi validado com execução real nessa entrega.
