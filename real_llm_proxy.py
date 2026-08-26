#!/usr/bin/env python3
"""
Real LLM Gateway Proxy
Routes requests between Ollama and Ghostlink Backend.
"""
import json
import requests
from http.server import HTTPServer, BaseHTTPRequestHandler
import sys
import os
import re

OLLAMA_URL = os.getenv('GHOSTLINK_OLLAMA_URL', "http://127.0.0.1:11434").strip().rstrip('/')
BACKEND_URL = os.getenv('GHOSTLINK_BACKEND_URL', "http://127.0.0.1:8003").strip().rstrip('/')
BIND_HOST = os.getenv('GHOSTLINK_PROXY_HOST', '127.0.0.1').strip() or '127.0.0.1'
MODEL = "neural-chat"
CHAT_BACKEND = "backend"
REQUEST_TIMEOUT_SECONDS = 180
HEADER_NAME_RE = re.compile(r"^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$")
ALLOWED_RESPONSE_HEADERS = {
    'cache-control': 'Cache-Control',
    'content-type': 'Content-Type',
    'etag': 'ETag',
    'expires': 'Expires',
    'last-modified': 'Last-Modified',
    'pragma': 'Pragma',
    'vary': 'Vary',
    'www-authenticate': 'WWW-Authenticate',
    'location': 'Location',
}


def _sanitize_header_name(name: str) -> str | None:
    candidate = str(name).strip()
    if HEADER_NAME_RE.fullmatch(candidate):
        return candidate
    return None


def _sanitize_header_value(value: str) -> str:
    candidate = str(value).replace('\r', '').replace('\n', '')
    return ''.join(
        ch
        for ch in candidate
        if ch == '\t' or 0x20 <= ord(ch) <= 0x7E
    ).strip()

class GatewayHandler(BaseHTTPRequestHandler):
    def send_cors_headers(self):
        """Send CORS headers to allow cross-origin requests from the frontend."""
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type, Accept, Authorization')
        self.send_header('Access-Control-Max-Age', '3600')

    def do_OPTIONS(self):
        """Handle CORS preflight requests."""
        self.send_response(200)
        self.send_cors_headers()
        self.end_headers()

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
            self.send_cors_headers()
            for k, v in resp.headers.items():
                header_key = k.lower()
                if header_key in ALLOWED_RESPONSE_HEADERS:
                    self.send_header(ALLOWED_RESPONSE_HEADERS[header_key], _sanitize_header_value(v))

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
        if self.path == '/health':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_cors_headers()
            self.end_headers()
            self.wfile.write(json.dumps({'status': 'ok'}).encode())
            return
        if self.path == '/api/inference/chat':
            self.handle_chat()
        elif self.path.startswith('/api/models'):
            # Use Rust backend model API contract for list/load/download/progress/delete.
            self.handle_proxy(BACKEND_URL)
        else:
            # Default to Rust Backend
            self.handle_proxy(BACKEND_URL)

    def handle_chat(self):
        """Handle chat via distributed backend by default, with Ollama fallback mode."""
        if CHAT_BACKEND == 'ollama':
            self.handle_chat_via_ollama()
            return
        self.handle_proxy(BACKEND_URL)

    def handle_chat_via_ollama(self):
        """Compatibility path for deployments that explicitly want Ollama chat."""
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

            resp = requests.post(
                f'{OLLAMA_URL}/api/generate',
                json=payload,
                timeout=REQUEST_TIMEOUT_SECONDS,
            )
            resp.raise_for_status()

            ollama_response = resp.json()
            response_text = ollama_response.get('response', '')

            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_cors_headers()

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
            self.send_cors_headers()
            self.end_headers()
            self.wfile.write(json.dumps({'error': str(e)}).encode())

    def log_message(self, format, *args):
        # Only log non-health check requests to reduce noise
        if '/health' not in args[0]:
            print(f'[Gateway] {format % args}')

if __name__ == '__main__':
    CHAT_BACKEND = (sys.argv[1] if len(sys.argv) > 1 else 'backend').strip().lower()
    if CHAT_BACKEND not in {'backend', 'ollama'}:
        CHAT_BACKEND = 'backend'

    REQUEST_TIMEOUT_SECONDS = int(
        os.getenv('GHOSTLINK_PROXY_TIMEOUT_S', str(REQUEST_TIMEOUT_SECONDS))
    )
    MODEL = os.getenv('GHOSTLINK_PROXY_MODEL', MODEL)

    port = int(os.getenv('GHOSTLINK_PROXY_PORT', '9999'))
    server = HTTPServer((BIND_HOST, port), GatewayHandler)
    print(f'[Gateway] Ghostlink Studio Gateway running on http://{BIND_HOST}:{port}')
    if CHAT_BACKEND == 'ollama':
        print(f'  -> /api/inference/chat  => Ollama ({OLLAMA_URL}, model={MODEL})')
    else:
        print(f'  -> /api/inference/chat  => Rust Backend distributed chat ({BACKEND_URL})')
    print(f'  -> /api/models/*        => Rust Backend ({BACKEND_URL})')
    print(f'  -> *                    => Rust Backend ({BACKEND_URL})')
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print('[Gateway] Shutting down')
