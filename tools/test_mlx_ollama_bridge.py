import unittest
from mlx_ollama_bridge import messages_for_mlx, to_mlx, to_ollama


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

    def test_payload_disables_upstream_streaming(self):
        result = to_mlx({"model": "local", "stream": True,
                         "messages": [{"role": "user", "content": "hi"}],
                         "tools": [{"type": "function", "function": {"name": "read_file"}}]})
        self.assertFalse(result["stream"])
        self.assertEqual(result["tools"][0]["function"]["name"], "read_file")


if __name__ == "__main__":
    unittest.main()
