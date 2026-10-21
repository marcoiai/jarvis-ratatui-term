#!/usr/bin/env python3
"""Build and validate DUDE SFT JSONL without silently inventing training evidence."""
import argparse
import hashlib
import json
from pathlib import Path

CATEGORIES = ("investigacao", "localizacao", "diagnostico", "planejamento",
              "implementacao", "ferramentas", "continuidade", "comunicacao")
ROLES = {"system", "user", "assistant"}
SEED = "dude-sft-v1"

def fingerprint(messages):
    raw = json.dumps(messages, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(raw.encode()).hexdigest()

def validate(entry, origin):
    if not isinstance(entry, dict) or entry.get("category") not in CATEGORIES:
        raise ValueError(f"{origin}: invalid category")
    messages = entry.get("messages")
    if not isinstance(messages, list) or len(messages) < 2:
        raise ValueError(f"{origin}: need at least user and assistant messages")
    for m in messages:
        if not isinstance(m, dict) or m.get("role") not in ROLES or not isinstance(m.get("content"), str) or not m["content"].strip():
            raise ValueError(f"{origin}: invalid message")
        if set(m) != {"role", "content"}:
            raise ValueError(f"{origin}: unexpected message properties (tool calls require separate schema)")
        if any(t in m["content"] for t in ("<|im_start|>", "<|im_end|>")):
            raise ValueError(f"{origin}: message contains reserved chat tokens")
    if messages[0]["role"] == "system":
        messages = messages[1:]
    if messages[0]["role"] != "user" or messages[-1]["role"] != "assistant":
        raise ValueError(f"{origin}: conversation must start with user and end with assistant")
    for a, b in zip(messages, messages[1:]):
        if a["role"] == b["role"]:
            raise ValueError(f"{origin}: consecutive same-role messages")
    return entry

def read_entries(folder):
    entries = []
    for path in sorted(folder.glob("*.jsonl")):
        with path.open(encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, 1):
                if line.strip():
                    e = validate(json.loads(line), f"{path}:{lineno}")
                    entries.append(e)
    if not entries:
        raise ValueError(f"No entries in {folder}")
    fingerprints = [fingerprint(e["messages"]) for e in entries]
    if len(fingerprints) != len(set(fingerprints)):
        raise ValueError("Duplicate conversations detected")
    return entries

def split_by_group(entries):
    # Group by scenario/source to prevent paraphrases leaking into evaluation.
    groups = {}
    for e in entries:
        group = e.get("group")
        if not isinstance(group, str) or not group.strip():
            raise ValueError("Each example requires a stable 'group' (scenario/project family)")
        groups.setdefault(group, []).append(e)
    train, eval_ = [], []
    for group, items in sorted(groups.items()):
        digest = hashlib.sha256((SEED + ":" + group).encode()).digest()
        (eval_ if int.from_bytes(digest[:4], "big") % 5 == 0 else train).extend(items)
    if not train or not eval_:
        raise ValueError("Dataset too small to guarantee both train/eval groups; add independent scenarios")
    return train, eval_

def write_jsonl(path, rows):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

def convert(entries, tokenizer_id):
    try:
        from transformers import AutoTokenizer
    except ImportError as e:
        raise SystemExit("Install transformers: python -m pip install transformers") from e
    tokenizer = AutoTokenizer.from_pretrained(tokenizer_id, trust_remote_code=False)
    if not tokenizer.chat_template:
        raise ValueError("Tokenizer has no chat template")
    return [{"text": tokenizer.apply_chat_template(e["messages"],
                tokenize=False, add_generation_prompt=False)} for e in entries]

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--input", type=Path, default=Path("training/examples"))
    p.add_argument("--output", type=Path, default=Path("training/output"))
    p.add_argument("--tokenizer", help="Exact HF model/tokenizer id used for SFT")
    p.add_argument("--format", choices=["messages", "text"], default="messages")
    args = p.parse_args()
    examples = read_entries(args.input)
    train, eval_ = split_by_group(examples)
    if args.format == "text":
        if not args.tokenizer:
            p.error("--format text requires --tokenizer (the exact training model)")
        prepared = [convert(train, args.tokenizer), convert(eval_, args.tokenizer)]
    else:
        prepared = [[{"messages": e["messages"]} for e in train],
                    [{"messages": e["messages"]} for e in eval_]]
    write_jsonl(args.output / "train.jsonl", prepared[0])
    write_jsonl(args.output / "eval.jsonl", prepared[1])
    counts = {category: sum(e["category"] == category for e in examples) for category in CATEGORIES}
    print(json.dumps({"train": len(train), "eval": len(eval_), "categories": counts,
                      "format": args.format}, ensure_ascii=False, indent=2))

if __name__ == "__main__":
    main()
