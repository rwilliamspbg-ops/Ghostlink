#!/usr/bin/env python3
"""
Real LLM proxy using built-in http.server
Routes /api/inference/chat to Ollama neural-chat model
"""
import json
import requests
from http.server import HTTPServer, BaseHTTPRequestHandler
import sys

OLLAMA_URL = "http://127.0.0.1:11434"
MODEL = "neural-chat"

class LLMHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == '/health':
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps({"status": "ok", "model": MODEL}).encode())
        else:
            self.send_response(404)
            self.end_headers()
    
    def do_POST(self):
        if self.path == '/api/inference/chat':
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
                    'temperature': temperature
                }
                
                resp = requests.post(f'{OLLAMA_URL}/api/generate', json=payload, timeout=60)
                resp.raise_for_status()
                
                ollama_response = resp.json()
                response_text = ollama_response.get('response', '')
                
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                
                result = {
                    'response': response_text,
                    'request_id': 'req-llm',
                    'model': MODEL
                }
                self.wfile.write(json.dumps(result).encode())
            except Exception as e:
                self.send_response(500)
                self.send_header('Content-Type', 'application/json')
                self.end_headers()
                self.wfile.write(json.dumps({'error': str(e)}).encode())
        else:
            self.send_response(404)
            self.end_headers()
    
    def log_message(self, format, *args):
        print(f'[Proxy] {format % args}')

if __name__ == '__main__':
    server = HTTPServer(('127.0.0.1', 9999), LLMHandler)
    print('[Proxy] Real LLM Proxy running on http://127.0.0.1:9999')
    print('[Proxy] Model: neural-chat via Ollama')
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print('[Proxy] Shutting down')
