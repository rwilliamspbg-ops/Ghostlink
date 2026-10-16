from ghostlink_client import GhostlinkClient

client = GhostlinkClient(
    base_url="http://localhost:8000",
    api_key="your-api-key",
)

# List available models
models = client.models.list()
print("Available models:", models)

# Chat completion
response = client.chat.completions.create(
    model="llama3.2",
    messages=[
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "What is 17 * 23?"},
    ],
)

print("Response:", response.choices[0].message.content)
