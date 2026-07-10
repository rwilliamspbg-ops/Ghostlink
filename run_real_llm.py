#!/usr/bin/env python3
"""
Ghostlink Studio with real LLM responses.
Redirects /api/inference/chat to /v1/chat/completions for real inference.
"""

import sys
import subprocess
import threading
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
import json
import requests
from urllib.parse import urlparse

# Proxy server to redirect mock endpoints to real LLM
class LLMProxyHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path == '/api/inference/chat':
            content_length = int(self.headers.get('Content-Length', 0))
            body = self.rfile.read(content_length)
            
            try:
                req = json.loads(body)
                message = req.get('message', '')
                system_prompt = req.get('system_prompt', 'You are a helpful AI assistant.')
                temperature = req.get('temperature', 0.7)
                max_tokens = req.get('max_tokens', 256)
                
                # Call real /v1/chat/completions endpoint
                v1_payload = {
                    'model': 'mistral',
                    'messages': [
                        {'role': 'system', 'content': system_prompt},
                        {'role': 'user', 'content': message}
                    ],
                    'temperature': temperature,
                    'max_tokens': max_tokens,
                    'stream': False
                }
                
                # Call the backend /v1/chat/completions
                resp = requests.post('http://127.0.0.1:8003/v1/chat/completions', json=v1_payload, timeout=30)
                resp.raise_for_status()
                data = resp.json()
                
                # Extract response
                response_text = ''
                if 'choices' in data and len(data['choices']) > 0:
                    choice = data['choices'][0]
                    if 'message' in choice:
                        response_text = choice['message'].get('content', '')
                
                # Format as /api/inference/chat response
                api_response = {
                    'response': response_text,
                    'request_id': data.get('id', 'req-unknown'),
                    'tokens_estimated': len(message.split()),
                    'metrics': {
                        'throughput': 0,
                        'p95_ms': 0
                    }
                }
                
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(json.dumps(api_response).encode())
            except Exception as e:
                self.send_response(500)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(json.dumps({'error': str(e)}).encode())
        else:
            # Pass through to backend
            self.send_response(404)
            self.end_headers()
    
    def log_message(self, format, *args):
        pass  # Suppress logging

def start_proxy_server():
    """Start proxy server on port 8004."""
    server = HTTPServer(('127.0.0.1', 8004), LLMProxyHandler)
    print('[Proxy] Real LLM proxy listening on http://127.0.0.1:8004')
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server

if __name__ == '__main__':
    print('[Ghostlink] Starting with real LLM responses...')
    print('[Ghostlink] Proxy server will redirect /api/inference/chat to real /v1/chat/completions')
    
    # Start proxy
    proxy = start_proxy_server()
    
    # Start GUI
    print('[Ghostlink] Launching GUI...')
    try:
        import ghostlink_gui_tkinter
        sys.exit(ghostlink_gui_tkinter.main())
    except KeyboardInterrupt:
        proxy.shutdown()
        print('[Ghostlink] Shutting down')
