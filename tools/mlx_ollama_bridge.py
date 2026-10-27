#!/usr/bin/env python3
"""Ollama-compatible proxy to an MLX OpenAI-style chat endpoint.

DUDE continues to own file tools, evidence checks and approval. The proxy
translates API envelopes; it does not execute tool calls.
"""
import argparse
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen
from urllib.error import HTTPError, URLError


def messages_for_mlx(messages):
    output, pending = [], []
    call_index = 0
    for entry in messages:
        role = entry.get("role")
        if role == "assistant" and entry.get("tool_calls"):
            calls = []
            for call in entry["tool_calls"]:
                call_index += 1
                function = call.get("function", {})
                args = function.get("arguments", {})
                if not isinstance(args, str):
                    args = json.dumps(args, ensure_ascii=False)
                call_id = call.get("id") or f"dude_{call_index}"
                calls.append({"id": call_id, "type": "function", "function": {
                    "name": function.get("name", ""), "arguments": args}})
                pending.append(call_id)
            output.append({"role": "assistant", "content": entry.get("content", ""), "tool_calls": calls})
        elif role == "tool":
            if not pending:
                raise ValueError("tool response without a matching tool request")
            output.append({"role": "tool", "tool_call_id": pending.pop(0),
                           "content": entry.get("content", "")})
        else:
            output.append({"role": role, "content": entry.get("content", "")})
    return output


def to_mlx(body):
    result = {"model": os.getenv("MLX_MODEL", "default_model"),
              "messages": messages_for_mlx(body.get("messages", [])),
              "stream": False}
    if body.get("tools"):
        result["tools"] = body["tools"]
    options = body.get("options") or {}
    if "temperature" in options:
        result["temperature"] = options["temperature"]
    result["max_tokens"] = int(os.getenv("MLX_MAX_TOKENS", options.get("num_predict", 1024)))
    return result


def to_ollama(result):
    message = result["choices"][0]["message"]
    calls = []
    for item in message.get("tool_calls") or []:
        fn = item.get("function", {})
        raw = fn.get("arguments") or "{}"
        try:
            arguments = json.loads(raw) if isinstance(raw, str) else raw
        except (ValueError, TypeError):
            arguments = {}  # DUDE validates arguments before execution.
        calls.append({"function": {"name": fn.get("name", ""), "arguments": arguments}})
    usage = result.get("usage") or {}
    return {"message": {"role": "assistant", "content": message.get("content") or "",
                        "tool_calls": calls},
            "done": True, "prompt_eval_count": usage.get("prompt_tokens", 0),
            "eval_count": usage.get("completion_tokens", 0)}


class Handler(BaseHTTPRequestHandler):
    upstream = "http://127.0.0.1:8082"

    def send_json(self, status, payload, ndjson=False):
        raw = json.dumps(payload, ensure_ascii=False).encode()
        if ndjson:
            raw += b"\n"
        self.send_response(status)
        self.send_header("Content-Type", "application/x-ndjson" if ndjson else "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        if self.path == "/api/tags":
            name = os.getenv("DUDE_BRIDGE_MODEL", "mlx-local")
            self.send_json(200, {"models": [{"name": name, "model": name}]})
        else:
            self.send_json(404, {"error": "unsupported endpoint"})

    def do_POST(self):
        if self.path != "/api/chat":
            self.send_json(404, {"error": "unsupported endpoint"})
            return
        try:
            count = int(self.headers.get("Content-Length", "0"))
            if not 0 < count <= 2000000:
                raise ValueError("invalid request length")
            request = json.loads(self.rfile.read(count))
            payload = to_mlx(request)
            req = Request(self.upstream.rstrip("/") + "/v1/chat/completions",
                          data=json.dumps(payload).encode(),
                          headers={"Content-Type": "application/json"})
            with urlopen(req, timeout=180) as response:
                result = json.load(response)
            self.send_json(200, to_ollama(result), ndjson=bool(request.get("stream")))
        except (KeyError, TypeError, ValueError) as e:
            self.send_json(400, {"error": str(e)})
        except HTTPError as e:
            self.send_json(502, {"error": f"MLX returned HTTP {e.code}"})
        except (URLError, TimeoutError, OSError) as e:
            self.send_json(502, {"error": f"MLX unavailable: {e}"})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=11882)
    parser.add_argument("--mlx-url", default="http://127.0.0.1:8082")
    args = parser.parse_args()
    Handler.upstream = args.mlx_url
    print(f"DUDE MLX bridge: {args.listen}:{args.port} -> {args.mlx_url}", flush=True)
    ThreadingHTTPServer((args.listen, args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
