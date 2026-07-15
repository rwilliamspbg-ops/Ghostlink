import requests

# Check for Ollama
try:
    resp = requests.get('http://127.0.0.1:11434/api/tags', timeout=2)
    print('Ollama available')
    models = resp.json().get('models', [])
    for m in models:
        print(f"  Model: {m.get('name')}")
except Exception as e:
    print(f'Ollama not available: {e}')

# Check for OpenAI API
try:
    import os
    key = os.getenv('OPENAI_API_KEY')
    if key:
        print('OpenAI API key available')
    else:
        print('No OpenAI API key')
except:
    pass
