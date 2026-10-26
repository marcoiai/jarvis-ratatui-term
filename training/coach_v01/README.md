# DUDE Coach v0.1

Experimentos de avaliação e supervisão para o DUDE. **Fixtures sintéticas** inspiradas em problemas reais de engenharia; não são logs de testes executados nem fine-tuning.

## Arquivos

- COACH_SYSTEM.md: política de observação, evidência, segurança e comunicação.
- golden_cases.jsonl: 37 exemplos revisáveis (treino).
- sft_messages.jsonl: os 37 exemplos no formato chat.
- tool_trajectories.jsonl: 12 trajetórias sintéticas de ferramentas, incluindo duas recusas sem chamada de ferramenta.
- holdout_eval.jsonl: 12 avaliações separadas dos exemplos de treino.
- eval_local.py: baseline do modelo bruto pelo Ollama; não mede o runtime Rust.

## Teste no Linux

```bash
cd training/coach_v01
python3 eval_local.py --dry-run
export OLLAMA_HOST=http://10.10.10.84:11434
export OLLAMA_MODEL=qwen3.5:4b
python3 eval_local.py --case E001
python3 eval_local.py --output baseline.jsonl
```

As respostas em baseline.jsonl exigem revisão: checagens por palavras-chave não comprovam correção semântica. Avaliar antes e depois de mudar prompts/código, mantendo holdout isolado. Os dados são para benchmark e preparação futura; **não** alteram o modelo nem o pipeline do DUDE. Não importar JSONL no Ollama esperando que treine GGUF Q4.

Próximos passos: replay de tools e assertions em Rust, métricas de regressão, registro de falhas reais anonimizadas, auditoria de alegações e só então avaliar LoRA/SFT.
