"""Deterministic negative-path provider for the locked Bruno planner stages.

Never used for the real-planner acceptance. No request text or credentials are
logged; usage is omitted deliberately to exercise conservative reservations.
"""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

parser = argparse.ArgumentParser()
parser.add_argument("--port", type=int, required=True)
parser.add_argument("--mode", choices=["invalid", "repair", "no-fit"], required=True)
args = parser.parse_args()
attempts = 0


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"ready")

    def do_POST(self):
        global attempts
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        attempts += 1
        decision = "no_fit" if args.mode == "no-fit" else "abstain"
        content = json.dumps({
            "schema_version": 1, "intent": "analysis",
            "analysis": {"schema_version": 1, "decision": decision, "confidence": "low", "nodes": []},
        })
        if args.mode == "invalid" or (args.mode == "repair" and attempts == 1):
            content = "```json\n" + content + "\n```"
        payload = json.dumps({
            "id": "negative-path-fixture",
            "object": "chat.completion",
            "model": "planner-test-fixture",
            "system_fingerprint": None,
            "choices": [{"index": 0, "finish_reason": "stop", "logprobs": None,
                         "message": {"role": "assistant", "content": content,
                                        "reasoning_content": "fixture hidden reasoning"}}],
        }).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
