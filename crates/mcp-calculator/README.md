# mcp-calculator

Local stdio [MCP](https://modelcontextprotocol.io/) server exposing a single `calculate`
tool — a safe math-expression evaluator — for
[Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink) chat's calculator tool slot.

**Not published to crates.io** (`publish = false`) — this is an internal component of the
main Ghostlink repository, spawned as a child process over stdio by the Ghostlink chat
runtime, not a standalone library or service meant to be depended on externally.

## What it does

One tool, `calculate(expression: string) -> number`, backed by
[`evalexpr`](https://docs.rs/evalexpr) for safe (no arbitrary code execution) numeric
expression evaluation. Runs as a local subprocess speaking MCP over stdio
(`rmcp`'s `transport-io`) — Ghostlink's chat tool-call loop spawns it, sends a tool call,
gets a result back, same pattern as `mcp-rag` and `mcp-vision`.

## Running standalone (for testing)

```bash
cargo run -p mcp-calculator
```

Speaks MCP over stdin/stdout — not meant to be run interactively; use an MCP-aware client
or Ghostlink's own chat tool-call loop (`mcp_servers.toml` in the main repo registers it).

## Testing

```bash
cargo test -p mcp-calculator
```

## More

See the [main repository](https://github.com/rwilliamspbg-ops/Ghostlink) — this crate is
one of three local MCP tool servers alongside
[`mcp-rag`](https://github.com/rwilliamspbg-ops/Ghostlink/tree/main/crates/mcp-rag) and
[`mcp-vision`](https://github.com/rwilliamspbg-ops/Ghostlink/tree/main/crates/mcp-vision).

## License

MIT — see [`LICENSE`](https://github.com/rwilliamspbg-ops/Ghostlink/blob/main/LICENSE) in
the main repository.
