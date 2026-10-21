# DUDE SFT dataset (bootstrap)

This is a **small synthetic starter dataset, not production fine-tuning data**. The 16 examples cover eight competencies; no project files were inferred or used to fabricate observations. Expand with consented, curated, verified developer sessions and independent evaluation scenarios before training.

Categories: investigacao, localizacao, diagnostico, planejamento, implementacao, ferramentas, continuidade, comunicacao.

Each example has `category`, `group` and `messages`. A group denotes the same scenario/project family; grouping prevents closely related train/eval examples from splitting across sets. Do not include secrets, private repo contents, API keys or unlicensed code.

## Build

```bash
python3 training/build_dataset.py --input training/examples --output training/output
python3 -m pip install transformers
python3 training/build_dataset.py --input training/examples --output training/output --format text --tokenizer Qwen/Qwen3.5-4B
```

The tokenizer ID is an **example**: use the exact model/checkpoint selected for training. The builder delegates special-token formatting to `AutoTokenizer.apply_chat_template`; do not handwrite Llama control tokens into Qwen data. Message-based JSONL is the canonical intermediate representation. Generated `training/output/` files are not committed.

## Quality gate

- JSON schema, roles, conversation turns and duplicates are validated.
- Split is deterministic by scenario group (80/20 hash assignment), not by individual message.
- Fail when the dataset is too small to produce both train and eval subsets.
- Add **held-out real workspace challenges**, human grading of investigation accuracy, hallucinations, correct tool use, and continuity against the unmodified base model.
- The starter examples are for validating the pipeline only; they do not establish fine-tuning benefits.
- This version supports text-only conversation examples. Tool calls need a later explicit schema and template-aware representation.
