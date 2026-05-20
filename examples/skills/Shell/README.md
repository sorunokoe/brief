# Shell Skill

Executes shell commands safely within a configured sandbox.

## How to use

```bash
brief check  YourTask.brief     # checks @enum("bash","sh","python","node") and @range coverage
brief verify YourTask.brief     # verifies @shell-command executables exist in PATH
brief serve  YourTask.brief     # start MCP server for AI (requires valid lock)
```

**`brief.toml` entry:**
```toml
[skills.Shell]
# 🔧 Replace with your actual shell MCP server
mcp_command = ["npx", "-y", "@brief/shell-skill"]

[verifiers."@shell-command"]
# ✅ Ships with Brief — checks that a command exists in PATH
skill = "builtin:shell-command"
```

## Interface

```
fn run(command: @shell-command String, args: Array<String>, timeoutSeconds: @range(1, 300) Int) -> CommandResult
fn runScript(script: @nonEmpty String, interpreter: @enum("bash","sh","python","node") String) -> CommandResult
fn commandExists(command: @shell-command String) -> Bool
```

## Parameters

- `command` — executable name or absolute path (e.g. `"git"`, `"npm"`, `"/usr/bin/curl"`)
- `args` — array of arguments passed to the command
- `timeoutSeconds` — execution timeout between 1 and 300 seconds
- `script` — shell script body (inline), must not be empty
- `interpreter` — script interpreter: `"bash"`, `"sh"`, `"python"`, or `"node"`

## Returns

- `CommandResult` — `{ exitCode: Int, stdout: String, stderr: String, duration: Int }`

## Errors

- `CommandNotFound` — command does not exist in PATH or specified path
- `Timeout` — command exceeded `timeoutSeconds`
- `PermissionDenied` — command not allowed in sandbox
