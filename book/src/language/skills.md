# Skills & Imports

## Importing Skills

```brief
import skill "GraphQL"
import skill "Auth"
```

## Skill Resolution

`brief check` searches in this order:
1. `brief.toml` `[skills]` table override
2. `.claude/skills/<Name>/<Name>.briefskill` (next to the `.brief` file)
3. `.claude/skills/<Name>/<Name>.briefskill` (cwd)

No `.briefskill` found → **W101** (warning only).

## Generating Skill Interfaces

```bash
brief skillgen .claude/skills/GraphQL/
# ✅ Interface generated: .claude/skills/GraphQL/GraphQL.briefskill
```

## Installing from Registry

```bash
brief add skill GraphQL
```

## MCP-Backed Skills

```brief
type GitHubMCP = @mcp GitHub
import skill "GitHubMCP"
```
