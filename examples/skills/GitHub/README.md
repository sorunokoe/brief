# GitHub Skill

Provides GitHub repository, PR, and issue operations for AI agents.

## How to use

Requires a `GITHUB_PERSONAL_ACCESS_TOKEN` environment variable with repo access.

```bash
brief check  YourTask.brief     # type-check @enum("open","closed","all") coverage in tests
brief verify YourTask.brief     # verifies @nonEmpty params are non-empty
brief serve  YourTask.brief     # start MCP server for AI (requires valid lock)
```

**`brief.toml` entry (uses the official Anthropic MCP server — installable today):**
```toml
[skills.GitHub]
mcp_command = ["npx", "-y", "@modelcontextprotocol/server-github"]
```

## Interface

Tool names match `@modelcontextprotocol/server-github` exactly (snake_case).

```
fn get_file_contents(owner: @nonEmpty String, repo: @nonEmpty String, path: String, ref: @nonEmpty String) -> FileContent
fn create_pull_request(owner: @nonEmpty String, repo: @nonEmpty String, title: @nonEmpty String, body: String, head: @nonEmpty String, base: @nonEmpty String) -> PullRequest
fn list_issues(owner: @nonEmpty String, repo: @nonEmpty String, state: @enum("open","closed","all") String) -> IssueList
fn create_issue(owner: @nonEmpty String, repo: @nonEmpty String, title: @nonEmpty String, body: String) -> Issue
fn get_pull_request(owner: @nonEmpty String, repo: @nonEmpty String, pullNumber: Int) -> PullRequest
```

## Parameters

- `owner` — GitHub username or organization (e.g. `"octocat"`)
- `repo` — repository name without owner prefix (e.g. `"hello-world"`)
- `ref` — branch name, tag, or commit SHA
- `state` — issue filter: `"open"`, `"closed"`, or `"all"` — `brief check` verifies all 3 are covered in tests

## Returns

- `FileContent` — `{ path: String, content: String, sha: String }`
- `PullRequest` — `{ number: Int, url: String, title: String }`
- `IssueList` — `{ issues: Array<{ number: Int, title: String, state: String }> }`
- `Issue` — `{ number: Int, url: String, title: String }`

## Errors

- `RepoNotFound` — repository does not exist or is inaccessible
- `BranchNotFound` — specified branch does not exist
- `Unauthorized` — missing or invalid GitHub token
