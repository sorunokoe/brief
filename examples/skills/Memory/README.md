# Memory Skill

Provides persistent key-value memory for AI agents across sessions.

## How to use

```bash
brief check  YourTask.brief     # checks @range(0, 86400) and @nonEmpty statically
brief verify YourTask.brief     # (no dynamic annotations — verify is a no-op here)
brief serve  YourTask.brief     # start MCP server for AI
```

**`brief.toml` entry (official Anthropic MCP server — installable today):**
```toml
[skills.Memory]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-memory"]
```

## Interface

```
fn store(key: @nonEmpty String, value: String, ttlSeconds: @range(0, 86400) Int) -> Unit
fn retrieve(key: @nonEmpty String) -> MemoryEntry
fn delete(key: @nonEmpty String) -> Unit
fn list(prefix: String) -> MemoryList
```

## Parameters

- `key` — memory key, must not be empty. Use namespaced keys: `"task:state"`, `"session:context"`.
- `value` — any string value to store
- `ttlSeconds` — time-to-live in seconds (0 = never expires, max 24h = 86400)
- `prefix` — key prefix filter for listing (empty string = list all)

## Returns

- `MemoryEntry` — `{ key: String, value: String, createdAt: String, expiresAt: String | null }`
- `MemoryList` — `{ entries: Array<{ key: String, createdAt: String }> }`

## Errors

- `KeyNotFound` — key does not exist or has expired
- `StoreFull` — memory store capacity exceeded
