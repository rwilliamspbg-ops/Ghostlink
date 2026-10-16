import { GhostlinkClient } from "ghostlink-client";

const client = new GhostlinkClient({
  baseURL: "http://localhost:8000",
  apiKey: "your-api-key",
});

async function main() {
  // List available models
  const models = await client.models.list();
  console.log("Available models:", models);

  // Chat completion
  const response = await client.chat.completions.create({
    model: "llama3.2",
    messages: [
      { role: "system", content: "You are a helpful assistant." },
      { role: "user", content: "What is 17 * 23?" },
    ],
  });

  console.log("Response:", response.choices[0].message.content);
}

main().catch(console.error);
