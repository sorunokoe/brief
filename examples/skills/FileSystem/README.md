# FileSystem Skill

Provides safe, sandboxed file system operations for AI agents.

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
