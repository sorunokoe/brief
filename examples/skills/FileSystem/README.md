# FileSystem Skill

Provides safe, sandboxed file system operations for AI agents.

## How to use

1. Copy `examples/skills/brief.toml` to your project root and set your `mcp_command` for this skill.
2. Write a `.brief` task that imports and uses `FileSystem`.
3. Run the enforcement chain:

```bash
brief check  YourTask.brief     # type-check @local-path annotations, ~0ms
brief verify YourTask.brief     # verify paths exist → writes .brief.lock
brief serve  YourTask.brief     # start MCP server for AI (requires valid lock)
```

See `TransformFile.brief` for a working example.

## Interface

```
fn readFile(path: @local-path String) -> FileContent
fn writeFile(path: @local-path String, content: String) -> Unit
fn listDirectory(path: @local-path String) -> DirectoryListing
fn fileExists(path: @local-path String) -> Bool
fn deleteFile(path: @local-path String) -> Unit
```

## Parameters

- `path` — absolute or relative path within the sandbox. Must be a real, accessible path.

## Returns

- `FileContent` — `{ path: String, content: String, size: Int }`
- `DirectoryListing` — `{ path: String, entries: Array<{ name: String, type: "file" | "dir", size: Int }> }`

## Errors

- `PathNotFound` — path does not exist
- `PermissionDenied` — path is outside sandbox
- `IsDirectory` — readFile called on a directory
