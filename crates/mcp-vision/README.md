# mcp-vision

Local stdio [MCP](https://modelcontextprotocol.io/) server exposing a single
`analyze_image` tool for [Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink)
chat's vision tool — backed by a local Ollama vision model, no external API calls.

**Not published to crates.io** (`publish = false`) — an internal component of the main
Ghostlink repository, spawned as a child process over stdio by the Ghostlink chat runtime.

## What it does

One tool, `analyze_image(image, prompt) -> string`: takes a base64-encoded image (and an
optional prompt describing what to look for), forwards it to a local Ollama vision model
over HTTP, and returns the model's description/analysis as text. Everything stays local —
no image data leaves the machine running Ollama.

## Running standalone (for testing)

```bash
cargo run -p mcp-vision
```

Requires a local Ollama instance running a vision-capable model. Speaks MCP over stdio —
use an MCP-aware client or Ghostlink's own chat tool-call loop (`mcp_servers.toml` in the
main repo registers it).

## Testing

```bash
cargo test -p mcp-vision
```

## More

See the [main repository](https://github.com/rwilliamspbg-ops/Ghostlink) — this crate is
one of three local MCP tool servers alongside
[`mcp-calculator`](https://github.com/rwilliamspbg-ops/Ghostlink/tree/main/crates/mcp-calculator)
and [`mcp-rag`](https://github.com/rwilliamspbg-ops/Ghostlink/tree/main/crates/mcp-rag).

## License

MIT — see [`LICENSE`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/LICENSE) in
the main repository.
