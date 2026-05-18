# Using Brief with Claude Code and GitHub Copilot

Brief integrates with AI coding agents via the [Model Context Protocol (MCP)](https://modelcontextprotocol.io). `brief serve` acts as an MCP server that exposes only the skills declared in your `.brief` file — no more, no less.

## How it Works

```
Claude / Copilot                brief serve             Your skills
     │                              │                       │
     │  (connects via MCP)          │                       │
     │──── initialize ─────────────▶│                       │
     │◀─── capabilities ────────────│                       │
     │                              │                       │
     │──── tools/list ─────────────▶│                       │
     │◀─── [GitHub.getFile,         │                       │
     │      GitHub.createPR, ...] ──│                       │
     │                              │                       │
     │──── tools/call ─────────────▶│                       │
     │     GitHub.getFile(...)      │──── tools/call ──────▶│
     │                              │◀─── result ───────────│
     │◀─── result ──────────────────│                       │
```

The AI only sees tools from skills listed in `uses []`. It cannot call anything else.

## Claude Code Setup

### 1. Write your .brief file

```brief
import skill "GitHub"
import skill "FileSystem"

task ReviewPR : TaskBrief uses [GitHub, FileSystem] {
    goal = "Review a pull request and write a summary to a file"

    step FetchPR {
        let pr = perform GitHub.getFile("owner/repo", "CHANGELOG.md", "main")?;
    }

    step WriteReport {
        let _ = perform FileSystem.writeFile("/workspace/review.md", "summary")?;
    }
}
```

### 2. Configure brief.toml

```toml
[project]
name = "my-project"

[skills.GitHub]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-github"]

[skills.FileSystem]
mcp_command = ["npx", "-y", "@brief/filesystem-skill"]

[verifiers."@github-repo"]
mcp_command = ["npx", "-y", "@brief/github-verifier"]

[verifiers."@local-path"]
mcp_command = ["npx", "-y", "@brief/local-path-verifier"]
```

### 3. Check and verify

```bash
brief check ReviewPR.brief     # instant type check
brief verify ReviewPR.brief    # seals contract → ReviewPR.brief.lock
```

### 4. Configure Claude Code

Add to your `.claude/settings.json` (or `~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "review-pr": {
      "command": "brief",
      "args": ["serve", "/path/to/ReviewPR.brief"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_your_token_here"
      }
    }
  }
}
```

Claude Code will now connect to `brief serve` and only see the tools defined in `ReviewPR.brief`.

### 5. Use in Claude Code

Once connected, Claude will have access to exactly these tools:
- `getFile(repo, path, branch)`
- `createPR(repo, title, body, head, base)`
- `listIssues(repo, state)`
- etc.

It cannot call any other tools — the contract is enforced at the protocol level.

## GitHub Copilot Setup

GitHub Copilot supports MCP servers via the `copilot-setup-steps.yml` mechanism or direct VS Code MCP configuration.

### VS Code settings.json

```json
{
  "github.copilot.chat.mcpServers": {
    "my-task": {
      "command": "brief",
      "args": ["serve", "${workspaceFolder}/task.brief"]
    }
  }
}
```

### Workspace MCP configuration

Create `.vscode/mcp.json`:

```json
{
  "servers": {
    "brief-task": {
      "type": "stdio",
      "command": "brief",
      "args": ["serve", "task.brief"]
    }
  }
}
```

## The Lock File — Why It Matters

`.brief.lock` is committed to git alongside your `.brief` file. It contains:

```toml
[meta]
brief_hash  = "sha256:abc123..."  # invalidates if .brief changes
verified_at = "2026-05-18T10:00:00Z"

[verified]
"@github-repo:owner/repo" = { status = "ok" }
"@local-path:/workspace"  = { status = "ok" }
"@url:https://api.github.com" = { status = "ok" }
```

`brief serve` refuses to start if:
- The lock is missing (run `brief verify` first)
- The lock is older than `max_lock_age_hours` (default: 24h)
- The `.brief` source has changed since the last verify

This means the AI always operates in a verified context — not a hopeful template.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `BRIEF_LLM_API_KEY` | API key for `brief gen` and `brief skillgen` LLM features |
| `BRIEF_LLM_PROVIDER` | `anthropic` (default) or `openai` |
| `BRIEF_LLM_MODEL` | Model to use (default: `claude-3-5-haiku-20241022` or `gpt-4o-mini`) |
| `BRIEF_LLM_URL` | Override the LLM API URL |

## Troubleshooting

**`brief serve` refuses to start:**
```
✗ Contract unsealed.
  Run `brief verify` first to seal the contract.
```
→ Run `brief verify your-file.brief` to generate the lock file.

**`brief serve` says contract is invalidated:**
```
✗ Contract invalidated — .brief file changed since last verify.
```
→ You edited the `.brief` file after verifying. Run `brief verify` again.

**`brief check` reports E309:**
```
error[E309]: annotation `@github-repo` on GitHub::getFile has no configured verifier
```
→ Add `[verifiers."@github-repo"]` to your `brief.toml`.

**`brief check` reports E303:**
```
error[E303]: .brief.lock missing — run `brief verify` to seal the contract
```
→ Normal first run. Run `brief verify` once to create the lock.
