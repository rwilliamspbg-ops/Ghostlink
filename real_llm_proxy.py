#!/usr/bin/env python3
"""
Real LLM Gateway Proxy
Routes requests between Ollama, Model Manager, and Ghostlink Backend.
"""
import json
import requests
from http.server import HTTPServer, BaseHTTPRequestHandler
import sys
from urllib.parse import urlparse

OLLAMA_URL = "http://127.0.0.1:11434"
MODEL_MANAGER_URL = "http://127.0.0.1:8001"
BACKEND_URL = "http://127.0.0.1:8003"
MODEL = "neural-chat"

class GatewayHandler(BaseHTTPRequestHandler):
    def handle_proxy(self, target_url):
        """Forward the current request to the target URL."""
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length) if content_length > 0 else None

        # Prepare headers (excluding Host)
        headers = {k: v for k, v in self.headers.items() if k.lower() != 'host'}

        try:
            resp = requests.request(
                method=self.command,
                url=f"{target_url}{self.path}",
                headers=headers,
                data=body,
                timeout=120,
                stream=True
            )

            self.send_response(resp.status_code)
            for k, v in resp.headers.items():
                if k.lower() not in ['content-encoding', 'transfer-encoding', 'content-length']:
                    self.send_header(k, v)

            # For simplicity in this proxy, we'll read the whole response
            # In a real production proxy we'd stream it
            content = resp.content
            self.send_header('Content-Length', str(len(content)))
            self.end_headers()
            self.wfile.write(content)

        except Exception as e:
            self.send_error(502, f"Gateway Error: {str(e)}")

    def do_GET(self):
        self.route_request()

    def do_POST(self):
        self.route_request()

    def do_PUT(self):
        self.route_request()

    def do_DELETE(self):
        self.route_request()

    def route_request(self):
        if self.path == '/api/inference/chat':
            self.handle_chat()
        elif self.path.startswith('/api/models'):
            self.handle_proxy(MODEL_MANAGER_URL)
        else:
            # Default to Rust Backend
            self.handle_proxy(BACKEND_URL)

    def handle_chat(self):
        """Special handling for chat to use Ollama neural-chat."""
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)

        try:
            data = json.loads(body)
            message = data.get('message', '')
            system_prompt = data.get('system_prompt', 'You are a helpful AI assistant.')
            temperature = data.get('temperature', 0.7)

            # Call Ollama
            payload = {
                'model': MODEL,
                'prompt': message,
                'system': system_prompt,
                'stream': False,
                'options': {
                    'temperature': temperature
                }
            }

            resp = requests.post(f'{OLLAMA_URL}/api/generate', json=payload, timeout=120)
            resp.raise_for_status()

            ollama_response = resp.json()
            response_text = ollama_response.get('response', '')

            self.send_response(200)
            self.send_header('Content-Type', 'application/json')

            result = {
                'response': response_text,
                'request_id': 'req-llm',
                'model': MODEL
            }
            encoded_result = json.dumps(result).encode()
            self.send_header('Content-Length', str(len(encoded_result)))
            self.end_headers()
            self.wfile.write(encoded_result)
        except Exception as e:
            self.send_response(500)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps({'error': str(e)}).encode())

    def log_message(self, format, *args):
        # Only log non-health check requests to reduce noise
        if '/health' not in args[0]:
            print(f'[Gateway] {format % args}')

if __name__ == '__main__':
    port = 9999
    server = HTTPServer(('127.0.0.1', port), GatewayHandler)
    print(f'[Gateway] Ghostlink Studio Gateway running on http://127.0.0.1:{port}')
    print(f'  -> /api/inference/chat  => Ollama ({OLLAMA_URL})')
    print(f'  -> /api/models/*        => Model Manager ({MODEL_MANAGER_URL})')
    print(f'  -> *                    => Rust Backend ({BACKEND_URL})')
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print('[Gateway] Shutting down')
