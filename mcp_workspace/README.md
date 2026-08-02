# mcp_workspace

Sandboxed root directory for the `filesystem` MCP server (see `mcp_servers.toml`).
The `file_operations` chat tool can only read/write files inside this directory —
that confinement is enforced by the `@modelcontextprotocol/server-filesystem`
process itself (it's given this path as its only allowed directory), not just by
convention.

`ghostlink.db` (the `database_query` tool's SQLite database) is also created here
at runtime and is gitignored.
