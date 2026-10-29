#!/usr/bin/env bash
# DUDE (Linux) -> local bridge -> SSH tunnel -> MLX server (Mac).
# Run on Linux after starting MLX on the Mac. Only processes started here are stopped.
set -Eeuo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MAC_HOST="${DUDE_MLX_MAC_HOST:-10.10.10.95}"
MAC_USER="${DUDE_MLX_MAC_USER:-}"
MLX_PORT="${DUDE_MLX_REMOTE_PORT:-8081}"
TUNNEL_PORT="${DUDE_MLX_TUNNEL_PORT:-18081}"
BRIDGE_PORT="${DUDE_MLX_BRIDGE_PORT:-11882}"
MODEL="${DUDE_MLX_MODEL:-}"
BRIDGE_MODEL="${DUDE_MLX_BRIDGE_MODEL:-mlx-local}"
TUNNEL_PID=""
BRIDGE_PID=""
LOG_DIR=""
cleanup() {
  local exit_status=$?
  trap - EXIT INT TERM
  if [[ -n "$LOG_DIR" && -d "$LOG_DIR" ]]; then
    if ((exit_status != 0)); then
      echo "DUDE MLX launcher failed. SSH/bridge logs:" >&2
      for logfile in "$LOG_DIR"/ssh.log "$LOG_DIR"/bridge.log; do
        if [[ -s "$logfile" ]]; then
          echo "--- $logfile" >&2
          tail -n 12 "$logfile" >&2
        fi
      done
    fi
    rm -rf -- "$LOG_DIR"
  fi
  if [[ -n "$BRIDGE_PID" ]]; then kill "$BRIDGE_PID" 2>/dev/null || true; wait "$BRIDGE_PID" 2>/dev/null || true; fi
  if [[ -n "$TUNNEL_PID" ]]; then kill "$TUNNEL_PID" 2>/dev/null || true; wait "$TUNNEL_PID" 2>/dev/null || true; fi
  exit "$exit_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command in ssh python3 curl cargo; do
  command -v "$command" >/dev/null || { echo "Missing command: $command" >&2; exit 1; }
done
for port in "$MLX_PORT" "$TUNNEL_PORT" "$BRIDGE_PORT"; do
  [[ "$port" =~ ^[0-9]+$ ]] && (( port>=1 && port<=65535 )) || { echo "Invalid port: $port" >&2; exit 2; }
done
if [[ ! "$MAC_HOST" =~ ^[a-zA-Z0-9._:-]+$ ]]; then echo "Invalid MAC_HOST" >&2; exit 2; fi
if [[ -n "$MAC_USER" && ! "$MAC_USER" =~ ^[a-zA-Z0-9_-]+$ ]]; then echo "Invalid MAC_USER" >&2; exit 2; fi
LOG_DIR="$(mktemp -d)"
target="$MAC_HOST"
if [[ -n "$MAC_USER" ]]; then target="$MAC_USER@$MAC_HOST"; fi
echo "DUDE MLX: tunnel localhost:$TUNNEL_PORT -> $target:127.0.0.1:$MLX_PORT"
# BatchMode avoids unexpected interactive key prompts; existing SSH host verification applies.
ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes -o ConnectTimeout=8 \
  -L "127.0.0.1:$TUNNEL_PORT:127.0.0.1:$MLX_PORT" "$target" </dev/null >"$LOG_DIR/ssh.log" 2>&1 &
TUNNEL_PID=$!
ready=0
for _ in {1..30}; do
  if ! kill -0 "$TUNNEL_PID" 2>/dev/null; then echo "SSH tunnel exited" >&2; exit 1; fi
  if curl --silent --fail --max-time 1 "http://127.0.0.1:$TUNNEL_PORT/v1/models" >/dev/null; then ready=1; break; fi
  sleep 1
done
if ((ready==0)); then echo "MLX is not reachable through SSH. Start mlx_lm.server on the Mac." >&2; exit 1; fi
# Resolve the actual model advertised by the upstream server; do not silently
# label a remote model "default_model" or guess among multiple choices.
model_list="$(curl --silent --show-error --fail --max-time 5 "http://127.0.0.1:$TUNNEL_PORT/v1/models")" || {
  echo "Cannot read MLX /v1/models" >&2; exit 1;
}
if [[ -z "$MODEL" ]]; then
  # If several models are advertised, offer a numbered choice on the terminal.
  # A single available model needs no prompt.
  available_models="$(printf '%s' "$model_list" | python3 -c '
import json, sys
try:
    payload = json.load(sys.stdin)
    ids = [item["id"] for item in payload.get("data", []) if isinstance(item, dict) and isinstance(item.get("id"), str) and item["id"]]
    if not ids: sys.exit("MLX did not advertise any model")
    print("\n".join(ids))
except (ValueError, TypeError, KeyError, AttributeError) as exc:
    sys.exit("Invalid MLX model discovery response: " + str(exc))
')" || exit 1
  mapfile -t choices <<< "$available_models"
  if (( ${#choices[@]} > 1 )); then
    [[ -t 0 ]] || { echo "Several MLX models found; set DUDE_MLX_MODEL for non-interactive use." >&2; exit 1; }
    echo "Available MLX models:"
    for i in "${!choices[@]}"; do printf '  %d) %s\n' "$((i+1))" "${choices[i]}"; done
    read -r -p "Select model [1-${#choices[@]}]: " choice
    [[ "$choice" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#choices[@]} )) || { echo "Invalid model selection" >&2; exit 1; }
    MODEL="${choices[choice-1]}"
  else
    MODEL="${choices[0]}"
  fi
fi
if [[ -n "$MODEL" && -n "${DUDE_MLX_MODEL:-}" ]]; then
  if ! printf '%s' "$model_list" | python3 -c '
import json, sys
selected = sys.argv[1]
try:
    ids = [item.get("id") for item in json.load(sys.stdin).get("data", []) if isinstance(item, dict)]
except (ValueError, TypeError, AttributeError):
    sys.exit("Invalid MLX model discovery response")
if selected not in ids:
    sys.exit("Selected DUDE_MLX_MODEL is not listed by MLX: " + selected)
' "$MODEL"; then exit 1; fi
fi
echo "MLX model: $MODEL (from /v1/models)"
echo "MLX reachable. Starting bridge on localhost:$BRIDGE_PORT"
MLX_MODEL="$MODEL" DUDE_BRIDGE_MODEL="$BRIDGE_MODEL" python3 "$ROOT/tools/mlx_ollama_bridge.py" \
  --listen 127.0.0.1 --mlx-url "http://127.0.0.1:$TUNNEL_PORT" --port "$BRIDGE_PORT" </dev/null >"$LOG_DIR/bridge.log" 2>&1 &
BRIDGE_PID=$!
ready=0
for _ in {1..15}; do
  if ! kill -0 "$BRIDGE_PID" 2>/dev/null; then echo "Bridge exited" >&2; exit 1; fi
  if curl --silent --fail --max-time 1 "http://127.0.0.1:$BRIDGE_PORT/api/tags" >/dev/null; then ready=1; break; fi
  sleep 1
done
if ((ready==0)); then echo "Bridge failed to start" >&2; exit 1; fi
echo "Starting DUDE. /ssh status checks local Linux ssh-agent. Ctrl+C exits and closes owned bridge/tunnel."
cd "$ROOT"
OLLAMA_HOST="http://127.0.0.1:$BRIDGE_PORT" OLLAMA_MODEL="$BRIDGE_MODEL" \
DUDE_INTERPRETER_BACKEND=mlx DUDE_INTERPRETER_HOST="http://127.0.0.1:$TUNNEL_PORT" \
DUDE_INTERPRETER_MODEL="$MODEL" cargo run --bin dude
