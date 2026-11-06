import unittest
from io import BytesIO
from urllib.error import HTTPError
from mlx_ollama_bridge import messages_for_mlx, to_mlx, to_ollama, upstream_error_detail


class BridgeTests(unittest.TestCase):
    def test_tool_arguments_roundtrip(self):
        entries = [
            {"role": "user", "content": "read file"},
            {"role": "assistant", "content": "", "tool_calls": [
                {"function": {"name": "read_file", "arguments": {"path": "src/main.rs"}}}
            ]},
            {"role": "tool", "tool_name": "read_file", "content": "1: fn main() {}"},
        ]
        converted = messages_for_mlx(entries)
        self.assertEqual(converted[1]["tool_calls"][0]["id"], "dude_1")
        self.assertEqual(converted[2]["tool_call_id"], "dude_1")

    def test_system_messages_are_consolidated_at_start(self):
        entries = [
            {"role": "system", "content": "Base safety instructions."},
            {"role": "user", "content": "Inspect src/main.rs"},
            {"role": "system", "content": "Cite evidence from files."},
            {"role": "assistant", "content": "Searching", "tool_calls": [
                {"function": {"name": "read_file", "arguments": {"path": "src/main.rs"}}}
            ]},
            {"role": "tool", "content": "1: fn main() {}"},
        ]
        converted = messages_for_mlx(entries)
        self.assertEqual([item["role"] for item in converted],
                         ["system", "user", "assistant", "tool"])
        self.assertEqual(converted[0]["content"].splitlines(),
                         ["Base safety instructions.", "", "Cite evidence from files."])
        self.assertEqual(converted[3]["tool_call_id"],
                         converted[2]["tool_calls"][0]["id"])

    def test_system_message_after_user_is_moved_to_start(self):
        converted = messages_for_mlx([
            {"role": "user", "content": "Hello"},
            {"role": "system", "content": "Use actual evidence"},
        ])
        self.assertEqual(converted, [
            {"role": "system", "content": "Use actual evidence"},
            {"role": "user", "content": "Hello"},
        ])

    def test_tool_result_without_call_is_rejected(self):
        with self.assertRaises(ValueError):
            messages_for_mlx([{"role": "tool", "content": "fake"}])

    def test_openai_tool_calls_become_ollama_calls(self):
        result = {"choices": [{"message": {"content": "", "tool_calls": [
            {"function": {"name": "read_file", "arguments": '{"path":"Cargo.toml"}'}}
        ]}}], "usage": {"prompt_tokens": 17, "completion_tokens": 9}}
        converted = to_ollama(result)
        self.assertEqual(converted["message"]["tool_calls"][0]["function"]["arguments"]["path"], "Cargo.toml")
        self.assertEqual(converted["eval_count"], 9)

    def test_bridge_reports_upstream_mlx_error(self):
        upstream = HTTPError(
            "http://127.0.0.1:8083/v1/chat/completions", 404,
            "Not Found", {}, BytesIO(b'{"error":"prompt exceeds context window"}')
        )
        self.assertEqual(upstream_error_detail(upstream), "prompt exceeds context window")

    def test_bridge_requests_no_thinking(self):
        payload = to_mlx({"messages": [{"role": "user", "content": "Hello"}]})
        self.assertEqual(payload["chat_template_kwargs"], {"enable_thinking": False})

    def test_payload_disables_upstream_streaming(self):
        result = to_mlx({"model": "local", "stream": True,
                         "messages": [{"role": "user", "content": "hi"}],
                         "tools": [{"type": "function", "function": {"name": "read_file"}}]})
        self.assertFalse(result["stream"])
        self.assertEqual(result["tools"][0]["function"]["name"], "read_file")


if __name__ == "__main__":
    unittest.main()
