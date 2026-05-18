# GitHub Skill

Provides GitHub repository and PR operations for AI agents.

## How to use

Requires a `GITHUB_TOKEN` environment variable with repo access.

```bash
brief check  YourTask.brief     # checks @enum("open","closed","all") coverage in tests
brief verify YourTask.brief     # verifies @github-repo repos are accessible
brief serve  YourTask.brief     # start MCP server for AI (requires valid lock)
```

**`brief.toml` entry (uses the official Anthropic MCP server — installable today):**
```toml
[skills.GitHub]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-github"]
```

## Interface

```
fn getFile(repo: @github-repo String, path: String, branch: @nonEmpty String) -> FileContent
fn createPR(repo: @github-repo String, title: @nonEmpty String, body: String, head: @nonEmpty String, base: @nonEmpty String) -> PullRequest
fn listIssues(repo: @github-repo String, state: @enum("open","closed","all") String) -> IssueList
fn createIssue(repo: @github-repo String, title: @nonEmpty String, body: String) -> Issue
fn getCommit(repo: @github-repo String, sha: @nonEmpty String) -> Commit
```

## Parameters

- `repo` — GitHub repository in `owner/name` format (e.g. `"octocat/hello-world"`)
- `branch` — branch name, must not be empty
- `state` — issue filter: `"open"`, `"closed"`, or `"all"`

## Returns

- `FileContent` — `{ path: String, content: String, sha: String }`
- `PullRequest` — `{ number: Int, url: String, title: String }`
- `IssueList` — `{ issues: Array<{ number: Int, title: String, state: String }> }`
- `Issue` — `{ number: Int, url: String, title: String }`
- `Commit` — `{ sha: String, message: String, author: String }`

## Errors

- `RepoNotFound` — repository does not exist or is inaccessible
- `BranchNotFound` — specified branch does not exist
- `Unauthorized` — missing or invalid GitHub token
