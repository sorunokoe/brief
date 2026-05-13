# Type Aliases & Effect Groups

## Type Aliases

```brief
type Email   = @matches("[^@]+@[^@]+") String
type Slug    = @matches("[a-z0-9-]+")  String
type UserID  = String
```

## Effect Groups

```brief
type AuthEffects = [Auth, Session, RBAC]

task SecureEndpoint : TaskBrief uses [AuthEffects] {
    goal = "Validate access for a restricted endpoint"
}
```

The compiler expands `AuthEffects` → `[Auth, Session, RBAC]`.
Missing group member → **E106**.

## MCP Aliases

```brief
type GitHubMCP = @mcp GitHub
```
