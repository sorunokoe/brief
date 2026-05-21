# FileSystem Skill

Provides safe, sandboxed file system operations for AI agents.

## How to use

1. Copy `examples/skills/brief.toml` to your project root and set the allowed directories.
2. Write a `.brief` task that imports and uses `FileSystem`.
3. Run the enforcement chain:

```bash
brief check  YourTask.brief     # type-check @local-path annotations, ~0ms
brief verify YourTask.brief     # verify paths exist → writes .brief.lock
brief serve  YourTask.brief     # start MCP server for AI (requires valid lock)
```

**`brief.toml` entry (uses the official Anthropic MCP server — installable today):**
```toml
[skills.FileSystem]
# List one or more allowed directories as arguments (the server sandboxes to these paths)
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/workspace"]
```

See `TransformFile.brief` for a working example.

## Interface

Tool names match `@modelcontextprotocol/server-filesystem` exactly (snake_case).

```
fn read_file(path: @local-path String) -> FileContent
fn write_file(path: @local-path String, content: String) -> Unit
fn list_directory(path: @local-path String) -> DirectoryListing
fn get_file_info(path: @local-path String) -> FileInfo
fn directory_tree(path: @local-path String) -> DirectoryTree
```

## Parameters

- `path` — absolute or relative path within the sandbox. Must be a real, accessible path.

## Returns

- `FileContent` — `{ path: String, content: String, size: Int }`
- `FileInfo` — `{ path: String, size: Int, created: String, modified: String, isDirectory: Bool }`
- `DirectoryListing` — `{ path: String, entries: Array<{ name: String, type: "file" | "dir", size: Int }> }`
- `DirectoryTree` — nested directory structure starting from `path`

## Errors

- `PathNotFound` — path does not exist
- `PermissionDenied` — path is outside allowed directories
- `IsDirectory` — read_file called on a directory
