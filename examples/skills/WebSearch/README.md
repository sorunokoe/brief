# WebSearch Skill

Provides web search and page fetching for AI agents.

## How to use

1. Copy `examples/skills/brief.toml` to your project root.
2. Configure `BRAVE_API_KEY` env var (or swap for another search MCP server).
3. Run the enforcement chain:

```bash
brief check  ResearchTopic.brief    # checks @range boundaries in test block, ~0ms
brief verify ResearchTopic.brief    # validates @url annotations → writes .brief.lock
brief serve  ResearchTopic.brief    # start MCP server for AI (requires valid lock)
```

See `ResearchTopic.brief` for a working example showing `@range` boundary coverage in tests.

## Interface

```
fn search(query: @nonEmpty String, maxResults: @range(1, 20) Int) -> SearchResults
fn fetchPage(url: @url String) -> PageContent
fn summarise(url: @url String, maxTokens: @range(100, 4000) Int) -> Summary
```

## Parameters

- `query` — search query string, must not be empty
- `maxResults` — number of results to return, between 1 and 20
- `url` — a valid http or https URL
- `maxTokens` — maximum tokens in the summary, between 100 and 4000

## Returns

- `SearchResults` — `{ results: Array<{ title: String, url: String, snippet: String }> }`
- `PageContent` — `{ url: String, title: String, text: String, fetchedAt: String }`
- `Summary` — `{ url: String, summary: String, tokenCount: Int }`

## Errors

- `SearchFailed` — search API unavailable
- `FetchFailed` — URL unreachable or returned an error
- `InvalidURL` — URL format is invalid
