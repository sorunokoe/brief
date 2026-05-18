# GitHub Skill

Provides GitHub repository and PR operations for AI agents.

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
