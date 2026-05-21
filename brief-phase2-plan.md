# Brief — Phase 2: The Permission, Verification, and Audit Layer

## Product Identity (reframed by TRIZ + rubber duck review)

Brief is **not** "a typed DSL for AI workflows."  
Brief is **the permission, verification, and audit layer for MCP-based AI agents.**

The analog: what SELinux/AppArmor does for processes — Brief does for AI agents.

| Problem | Brief's answer |
|---------|---------------|
| AI uses wrong tools | `uses[]` + `forbids{}` (done) |
| AI misuses allowed tools (wrong path, wrong repo) | `allow{}/deny{}` — **the new killer feature** |
| Skills don't exist / wrong interface | `brief verify` + `brief skillsync` |
| Can't see what AI actually did | `brief serve --record` + `brief audit` |
| Slow feedback loop when AI fails | `brief suggest` |

---

## The Core Contradiction (Phase 2 — TRIZ final pass)

Phase 1 solved: "verified vs. immediately usable" → draft mode (separation in time).

**New PC (rubber duck critique):** The constraint mechanism must be:
- **STATIC** — declared in the spec file, verified by checker at compile time
- **DYNAMIC** — enforced at runtime against actual JSON argument values

These oppose each other because argument values are unknown at compile time.

**TRIZ resolution:** Separation in time (P17) + Parameter Changes (P35)
- Compile time: checker validates patterns reference real functions with real argument names → E423
- Runtime: proxy enforcer matches JSON call arguments against compiled `allow{}/deny{}` patterns → E422

IFR: "The spec file itself enforces what the AI can do — with no code changes and no runtime configuration."

---

## The Killer Demo (what makes Brief irreplaceable)

```brief
task "review-pr" {
  uses [GitHub, FileSystem]
  forbids { skill "Shell" }          # hidden from tools/list entirely

  allow {                            # whitelist: ONLY these argument patterns permitted
    GitHub.get_pull_request(owner="acme", repo="brief", number=*)
    GitHub.get_file_contents(owner="acme", repo="brief")
    FileSystem.write_file(path="/tmp/review.md")
    FileSystem.read_file(path="/tmp/**")
  }
  deny {                             # always blocks, even if allow matches
    GitHub.merge_pull_request
    FileSystem.write_file(path="./src/**")
    FileSystem.read_file(path="**/.env")
  }
}
```

```
$ brief serve review-pr --record trace.jsonl
$ brief audit trace.jsonl

PASS  GitHub.get_pull_request(owner="acme", repo="brief", number=123)
PASS  GitHub.get_file_contents — matched allow pattern
PASS  FileSystem.write_file(path="/tmp/review.md")
DENY  FileSystem.write_file(path="./src/main.rs") — matched deny: path="./src/**"
DENY  GitHub.merge_pull_request — not in allow{} patterns
```

This is impossible to build with "give the AI a list of tools." This requires Brief.

---

## TRIZ Analysis — Second Pass (completed phases: 1–3 ✅)

### New Core Contradiction

Phase 1 solved the adoption contradiction (verified vs. immediately usable → draft mode).

**New PC revealed:** The skill verification mechanism must be:
- **PASSIVE** (zero-cost, zero-network, fast) for CI-safe compile-time checks
- **ACTIVE** (connects to real systems, behavioral) to verify AI can actually complete the task

**Matrix lookups (exact):**
- TC1: Improving Reliability (#27) × Worsening Ease of Operation (#33) → R27×W33 → **P27, P17, P40**
- TC2: Improving Reliability (#27) × Worsening Productivity (#39) → R27×W39 → **P1, P35, P38**

**Vepol:** Insufficient vepol — S1(checker) → weak info field → S2(task spec)
- Standard 1.3.1 (Increase Field Intensity) + Standard 2.1.1 (Transition to Bi-System)
- Resolution: add a second parallel verification substance (semantic/behavioral field) to complement the existing structural field

**IFR-1:** Brief's existing verification system verifies behavioral correctness by itself, without adding new external services.
**IFR-2:** Skill verification must be PASSIVE at spec time (static, instant) AND ACTIVE at sync time (behavioral, from truth source), separated in time.

**Evolution Law 1 (Completeness):** Missing "control" element — no feedback loop from AI execution back to spec.
**Evolutionary forecast:** Brief must complete the control loop. Without it, Brief remains one-directional.

### Semantic Verification Sub-Contradiction — FINAL TRIZ resolution (fourth pass)

**The contradiction:**
- The semantic checker must be RELIABLE (CI-safe, no false positives, reproducible)
- AND INTELLIGENT (understands natural language goal intent)

These are fundamentally opposed because semantic understanding is probabilistic — any LLM-based check will have false positives and will be ignored once it cries wolf.

**Rubber duck critique (Round 2) confirmed:** W401 as a compiler warning is wrong because "semantic matching between free-text goal and free-text descriptions is unreliable." The rubber duck was right — about that specific form.

**But the user is right too:** LLM + language collaboration CAN help reach the goal. The error was WHERE to put the LLM, not WHETHER to use it.

**TRIZ — Separation by Condition (P28 + Standard 2.1.2):**

| When | What Brief uses | Why |
|------|----------------|-----|
| `brief check` (CI, every run) | Deterministic rules only — zero LLM | Reliable, fast, no surprises |
| `brief policy suggest` (one-shot, developer reviews) | LLM — generates allow{}/deny{} spec from goal | Developer reviews + compiles result into spec |
| `brief skillsync --enrich` (one-shot, developer reviews) | LLM — enriches @desc from MCP descriptions | Richer metadata for better policy suggestion |

**The key insight:** LLM is used for GENERATION, not VERIFICATION. LLM writes code once. Compiler verifies it every build. This is how AI code generation already works — Brief just applies it to policy generation.

**IFR-2 (revised):** "Brief's policy files contain LLM-generated intelligence, compiled into deterministic rules, verified by the checker — without any LLM at check time."

**Level 4 resolution:**
1. `brief skillsync --enrich`: LLM (SmolLM2-135M, in-process, no server) generates richer `@desc` tags
2. `brief policy suggest`: LLM reads goal + skill interfaces (including @desc) → suggests allow{}/deny{} patterns
3. Developer reviews suggestion → applies → `brief check` validates it deterministically
4. **Result:** Intelligence is compiled into the spec. Verification is always rule-based.

This is the correct form of "merging LLM with language": the LLM enriches the language's CONTENT (generated code), not its verification rules.

### 6 Concepts (final)

| # | Concept | TRIZ | Level | Priority |
|---|---------|------|-------|----------|
| A | `allow{}/deny{}` — argument-level scoping | P1, P35, separation in space | **4** | **#1** |
| B | `brief serve --record` + `brief audit` — behavioral trace + policy violations | P20, P23 | 3 | **#2** |
| C | `brief skillsync` — auto-generate .briefskill from live server | P27, P35 | 3 | **#3** |
| D | `brief policy suggest` — LLM generates allow{}/deny{} from goal | S2.1.2, P35, separation by condition | **4** | **#4** |
| E | `brief models install` + candle — SmolLM2-135M in-process | P35 (super-system resource) | 3 | **#5** |
| F | `brief suggest` — spec self-improves from trace | S2.2.5, P23 | 3 | **#6** |

**W401 removed from `brief check`.** Replaced by `brief policy suggest` — which gives the developer a COMPLETE suggested spec, not just a "this might be missing" warning.

**Stopping criterion:** Concepts A+D together score Level 4 AND resolve both stated PCs without adding external services (LLM is in-process via candle). ✅


---

# Brief — Phase 1 (DONE ✅)

## Honest Assessment (TRIZ-derived)

## Honest Assessment (TRIZ-derived)

**The project has genuine potential. The core idea is sound and timely.**

MCP is becoming the standard. Typed, bounded, verified AI agent contexts are a real problem.
The enforcement chain (check → verify → serve) is elegant and principled.
The `needs{}` / `forbids{}` blocks are genuinely unique. The skill interface typing is novel.

**But the project has never been used on a real task. That is the only real problem.**

---

## The Core Contradiction (TRIZ Step 2 — PC)

> Brief must be **fully verified** (complete, typed, locked) to be trustworthy  
> AND **immediately usable** (zero steps, no DSL, no npm) to be adopted.

These are in direct opposition. No feature addition resolves this — only architectural separation does.

**TRIZ Resolution: Separation in time (P17 + P27)**  
The contract can be incomplete and provisional early, then harden at the risk boundary.  
Exploratory AI use ≠ production AI use. Both are valid. Brief should serve both.

---

## What Is Actually Broken (in priority order)

1. **The verifier ecosystem doesn't exist.**  
   `@local-path`, `@github-repo`, `@shell-command` all require npm packages that aren't published.  
   Without these, `brief verify` with any dynamic annotation fails immediately.  
   This is the #1 gap. `brief verify` is the heart of the value proposition.

2. **There is no way to start without going through verify first.**  
   The current enforcement chain is: check → verify → serve (requires lock).  
   If you skip verify, you can't serve. This means users must debug the full chain before seeing any value.  
   This is the adoption killer the TRIZ contradiction matrix predicted.

3. **54 examples create the illusion of a working ecosystem.**  
   They all compile. None of them can run end-to-end (no real skill servers, no verifier npm packages).  
   This is a false signal of completeness.

---

## What Is Already Good (keep, don't touch)

- The compiler core (lexer, parser, checker, HIR, typeck) — solid, tested, keep exactly as-is
- The MCP server (`brief serve`) — this IS the product
- The enforcement logic (`uses[]`, `forbids{}`, `needs{}`) — genuinely unique
- The lock gate concept — keep, just make reaching it easier
- The LSP — good, working, don't add to it but don't deprecate
- The self-hosting compiler descriptions (Stage 1) — keep frozen, it's elegant as-is
- `brief check` — fast, correct, CI-safe — core tool, keep

---

## What to Freeze (don't add features, don't advertise as roadmap)

- **LLVM/WASM backend** — fascinating engineering, zero user value today. Freeze.
- **Self-hosting Stage 2** — the Rust-in-Brief replacement of backends. Academic. Freeze.
- **REPL** — wrong interaction model for current adoption stage. Freeze.
- **Registry** — empty ecosystem + empty registry = theater. Freeze until verifiers exist.
- **`brief build`** — remove from roadmap table until LLVM backend is real.

---

## Plan

### Phase 0 — Reality Test (do this TODAY, before any code)

**Goal:** Discover what actually breaks in the real workflow.

- Pick ONE real task from your daily AI workflow (e.g., "review a GitHub PR with Claude Code")
- Write a `.brief` file for it
- Try to go all the way: `brief check` → `brief verify` → `brief serve` → AI agent connects
- Document every friction point on paper
- If you can't get to `brief serve` working with a real skill, that IS the plan input

**Expected finding:** `brief verify` fails because the dynamic annotation verifiers (npm packages) don't exist.

---

### Phase 1 — Native Verifiers (the #1 unblock)

**Goal:** Make `brief verify` work without any npm dependencies.

**Approach:** Add builtin Rust verifiers directly to `verifier.rs` dispatch, alongside the existing `builtin:url`.

**Three verifiers to build (in Rust, zero npm):**

#### `builtin:local-path`
```rust
fn builtin_local_path(value: &str) -> VerificationResult {
    if std::path::Path::new(value).exists() {
        VerificationResult::ok()
    } else {
        VerificationResult::fail(&format!("path does not exist: {value}"))
    }
}
```
Config in `brief.toml`:
```toml
[verifiers."@local-path"]
skill = "builtin:local-path"
```

#### `builtin:shell-command`
```rust
fn builtin_shell_command(value: &str) -> VerificationResult {
    let cmd = value.split_whitespace().next().unwrap_or(value);
    if which::which(cmd).is_ok() {
        VerificationResult::ok()
    } else {
        VerificationResult::fail(&format!("command not found in PATH: {cmd}"))
    }
}
```

#### `builtin:github-repo`
HTTP GET to `https://api.github.com/repos/{owner}/{repo}` with `Accept: application/vnd.github+json`.
Uses `GITHUB_TOKEN` env var if set. No new dependencies beyond what `builtin:url` already uses.

**After Phase 1:** `brief verify` works without npm for the 5 reference skills.
Update `examples/skills/brief.toml` to use `builtin:local-path` etc. instead of npm commands.

---

### Phase 2 — Draft Mode (`brief serve --draft`)

**Goal:** Resolve the core TRIZ contradiction. Let users get value before they've sealed the contract.

**What `--draft` does:**
1. Runs `brief check` (full static type check — fast, no network)
2. Enforces `uses[]` scope (AI only sees declared tools — same as production)
3. Enforces `forbids{}` (forbidden skills/functions hidden from tools/list — same as production)
4. Checks `needs{}` env vars at startup (E411 if missing — same as production)
5. Skips lock gate — does NOT require `.brief.lock`
6. Emits a visible warning banner on stderr: `⚠ Draft mode — dynamic annotations not verified`

**What `--draft` does NOT do:**
- Does not verify dynamic annotations (`@local-path`, `@github-repo`, `@url`, etc.)
- Does not write or read `.brief.lock`

**Effect:** Users can start working with AI immediately. The scope enforcement and needs/forbids still protect them. They graduate to `brief verify` + `brief serve` (production mode) when they need the full trust guarantee.

**TRIZ resolution:** Separation in time. Exploratory mode (draft) vs. production mode (locked).

**Implementation:** Add `--draft` flag to `Commands::Serve`. In `serve::run_serve()`, accept an additional `bool` for draft mode. If draft, skip the lock check. Keep all other enforcement.

---

### Phase 3 — 5 Canonical Examples (replace 54 theoretical ones)

**Goal:** Replace the illusion of completeness with 5 examples that actually work end-to-end.

**Keep:**
1. `hello.brief` — zero-setup, shows the minimum
2. `review-pr.brief` — GitHub + FileSystem, common real workflow
3. `ai-pipeline.brief` — LLM + VectorStore, the AI-native use case
4. `checkout-flow.brief` — needs{} + forbids{} showcase (the unique Brief features)
5. `compiler-pipeline.brief` — self-hosting demo (shows depth)

**Remove or move to `examples/archive/`:** the other 49 examples.

**Requirement:** Each of the 5 canonical examples must be runnable with `brief check` AND have a clear path to `brief verify` using only builtin verifiers.

---

### Phase 4 — One Real Workflow Story

**Goal:** Prove Brief works for real by using it yourself for one week.

- Use `review-pr.brief` (or equivalent) in your actual PR review workflow
- Run it with `brief serve --draft` first
- Graduate to `brief serve` (locked) when it's stable
- Document what Brief caught, what it prevented, what it simplified
- This story becomes the README introduction

**This is not optional.** A tool that has never been used is a prototype, not a product.

---

## Frozen Scope (do not add features here)

| Area | Status | Reason |
|------|--------|--------|
| LLVM/WASM backend | Frozen | No user benefit until ecosystem exists |
| Self-hosting Stage 2 | Frozen | Elegant but inward-facing |
| REPL | Frozen | Wrong interaction model at this stage |
| Registry | Frozen | Empty registry adds confusion |
| LSP | Keep as-is | Works, don't expand |
| `brief gen` | Keep as-is | Useful, don't expand |
| `brief skillgen` | Keep as-is | Useful, don't expand |

---

## Definition of Done

Brief is valuable when this is true:

> A developer on your team can clone a repo, install `brief`, write one `.brief` file for a real task, run `brief check` → `brief verify` → `brief serve`, connect Claude Code, and have the AI work with ONLY the tools declared in `uses[]` — in under 10 minutes, without installing any npm packages.

That is the test. Everything in this plan serves that test.

---

## Summary: The Three Things

1. **Build the native verifiers** (Phase 1) — makes `brief verify` actually work
2. **Add `--draft` mode** (Phase 2) — resolves the core adoption contradiction
3. **Use it on a real task** (Phase 0 + Phase 4) — turns a prototype into a product

The rest is noise until these three things are true.

---

# Brief — Phase 2: Implementation Plan

## Architecture — The Closed Loop

```
MCP Server ──→ brief skillsync ──→ .briefskill (auto-generated from live server)
                                         ↓
Developer writes .brief → brief check (E423: invalid allow/deny patterns)
                                         ↓
      brief serve --record → .brief.trace (ALL calls: allowed + BLOCKED)
                                         ↓
           brief suggest → spec diff (what to add/fix based on real behavior)
                                         ↓
     Developer reviews → brief verify → .brief.lock → production
```

Evolution Law 1 (Completeness): This closes the missing control loop. Brief was one-directional; now it's self-correcting.

---

## Concept A — `allow{}/deny{}` Argument-Level Scoping (Priority #1 — THE killer feature)

**The gap the rubber duck found:** `uses[GitHub, FileSystem]` gives AI access to ALL GitHub + FileSystem tools with ANY arguments. Brief can stop "AI uses Shell" but not "AI writes to ./src/main.rs using the allowed FileSystem skill."

**TRIZ resolution:** P1 (Segmentation) + P35 (Parameter Changes): segment the permission model from tool-level to argument-level. Change the permission unit from "tool" to "argument value pattern."

### Semantics

- **`allow {}`** = whitelist mode: ONLY listed call patterns are permitted (any other call → E422)
- **`deny {}`** = blacklist additions: always blocks even if an `allow` pattern also matches
- Without `allow {}`: existing `uses[]/forbids{}` behavior unchanged (backward compatible)
- `forbids{}` operates at tool-discovery level (hidden from `tools/list`); `deny{}` operates at invocation level (visible but blocked with reason)

### Language syntax

```brief
task "review-pr" {
  uses [GitHub, FileSystem]
  forbids { skill "Shell" }            # hidden from tools/list

  allow {
    GitHub.get_pull_request(owner="acme", repo="brief", number=*)
    GitHub.get_file_contents(owner="acme", repo="brief")
    FileSystem.write_file(path="/tmp/review.md")
    FileSystem.read_file(path="/tmp/**")
  }
  deny {
    GitHub.merge_pull_request
    FileSystem.write_file(path="./src/**")
    FileSystem.read_file(path="**/.env")
  }
}
```

### Pattern syntax (MVP — no regex, just globs on string args)

- `GitHub.*` — any function in GitHub (allows all calls to GitHub)
- `GitHub.get_file_contents` — specific function, any arguments
- `GitHub.get_file_contents(owner="acme", repo="brief")` — specific function + specific arg values
- `FileSystem.write_file(path="/tmp/**")` — glob on path argument (`**` = any sub-path)
- Wildcard arg value: `number=*` — any value accepted for that argument
- Missing args in pattern = not constrained (any value accepted for unlisted args)

### AST changes

New structs in `ast.rs`:
```rust
pub enum ArgPattern {
    /// owner="acme" — string: exact match; number: JSON-equal
    Exact(serde_json::Value),   // NOT String — avoids number="1" vs 1 mismatch
    /// path="/tmp/**" — glob on string values only (not valid for non-string args)
    Glob(String),
    /// number=* — any value accepted for this argument
    Any,
}
pub struct CallPattern {
    pub skill: String,                        // "GitHub"
    pub func: Option<String>,                 // None = wildcard (GitHub.*)
    pub args: Vec<(String, ArgPattern)>,      // [("owner", Exact("acme")), ...]
    pub span: Span,
}
pub struct Task {
    // ... existing fields ...
    pub allow: Vec<CallPattern>,
    pub deny: Vec<CallPattern>,
}
```

**Critical design decision — ArgPattern::Exact uses `serde_json::Value`** (not `String`). This avoids the mismatch between `number="1"` (string) and the actual JSON `{"number": 1}` (integer). The parser infers type: quoted values → `Value::String`, unquoted digits → `Value::Number`, `true`/`false` → `Value::Bool`.

**Path normalization policy:** Before applying glob matching, ALL path string values are normalized:
1. Remove trailing slashes
2. Resolve `..` segments (relative to the pattern's own base, not filesystem)
3. Normalize `/./` → `/`
4. NOT resolving symlinks — Brief does not have filesystem access during enforcement
5. NOT resolving absolute paths — Brief does not know the MCP server's CWD

**Note on nested arg matching:** Out of scope for MVP. Only top-level JSON object keys are matched. `labels[*]="bug"` syntax is not supported in Phase 2.

### Parser changes

Parse `allow { ... }` and `deny { ... }` blocks in task body. Each item is:
- `Skill.*` → `CallPattern { skill, func: None, args: [] }`
- `Skill.func` → `CallPattern { skill, func: Some(func), args: [] }`
- `Skill.func(key="val", key2=42, key3=*)` → full pattern with typed args
  - Quoted `"val"` → `ArgPattern::Exact(Value::String("val"))`
  - Unquoted digits → `ArgPattern::Exact(Value::Number(...))`
  - `true`/`false` → `ArgPattern::Exact(Value::Bool(...))`
  - Pattern with `**` → `ArgPattern::Glob(...)` (string args only)
  - `*` alone → `ArgPattern::Any`

### Checker changes (E423, W408, W409)

New in `checker.rs`:
- **E423** — `AllowDenyPatternInvalid`: func/skill in `allow{}/deny{}` not found in any declared skill's interface
- **E424** — `EmptyAllowBlock`: `allow {}` with no patterns (blocks ALL tool calls — almost always a mistake)
- **W408** — `AllowDenyContradiction`: deny pattern subsumes (shadows) an allow pattern  
  - covers exact duplicates AND subsumption (e.g., deny `path="/tmp/**"` shadows allow `path="/tmp/review.md"`)
- **W409** — `UnconstrainedSensitiveArg`: allow pattern for a function with sensitive arg names (`path`, `file`, `content`, `body`, `ref`, `branch`, `command`) leaves those args unconstrained
  - Example: `GitHub.get_file_contents(owner="acme", repo="brief")` → W409 because `path` is unconstrained
- Validate arg names exist in the function's `.briefskill` signature

### Runtime enforcement (new `briefc/src/enforcer.rs`)

```rust
pub enum CallDecision {
    Permitted,
    Blocked { reason: String },
}

pub fn check_call(
    skill: &str,
    func: &str,
    args: &serde_json::Value,
    allow: &[CallPattern],
    deny: &[CallPattern],
) -> CallDecision
```
- **Step 1:** Check deny patterns. If any deny pattern matches → `Blocked { reason }` immediately.
- **Step 2:** If `allow` is non-empty (whitelist mode): if no allow pattern matches → `Blocked { reason: "not in allow{} patterns" }`.
- **Step 3:** → `Permitted`.

**Matching logic:**
- `func: None` (wildcard) matches any function in that skill
- `args` empty in pattern matches any args
- `ArgPattern::Exact(v)` — JSON equality between `v` and the actual arg value
- `ArgPattern::Glob(g)` — only for string arg values: normalize path, then `glob::Pattern::matches()`
- `ArgPattern::Any` — always matches

**E422 MCP error format** (machine-readable for AI agent consumption):
```json
{
  "code": -32022,
  "message": "Brief policy blocked call: FileSystem.write_file — matched deny: path='./src/**'",
  "data": {
    "brief_code": "E422",
    "skill": "FileSystem",
    "function": "write_file",
    "reason": "matched deny",
    "pattern": "FileSystem.write_file(path='./src/**')"
  }
}
```

Use a dedicated JSON-RPC code (-32022) not the generic -32001 to allow agents to distinguish Brief policy errors from server errors.

### Tests

- `allow` whitelist: permitted call → passes; non-listed call → E422
- `deny` blacklist: denied call → E422 with reason; non-denied call → passes
- Both: deny overrides allow when same pattern matches
- Glob paths: `/tmp/foo/bar.md` matches `/tmp/**`; `./src/main.rs` matches `./src/**`
- Path normalization: `/tmp/../src/main.rs` normalized to `/src/main.rs`, matches deny `/src/**`
- JSON type matching: `{"number": 1}` matches `Exact(Value::Number(1))`, NOT `Exact(Value::String("1"))`
- E423: pattern references `GitHub.nonexistent_func` → error at check time
- E424: empty `allow {}` block → error at check time
- W408: deny pattern subsumes allow pattern → warning
- W409: allow `GitHub.get_file_contents(owner="acme")` → W409 for unconstrained `path`
- Backward compat: task without `allow{}/deny{}` blocks → no behavior change

---

## Concept B — `brief skillsync` (Priority #2)

**What it does:** Connects to each configured MCP server, calls `tools/list`, auto-generates a `.briefskill` interface file. Name-mismatch bugs become impossible.

**Drops from previous plan:** No `@desc`, no candle/SmolLM2 integration — the rubber duck confirmed natural language semantic matching is premature and unreliable. `skillsync` does structural generation only.

**Implementation:**
1. New command `Commands::Skillsync { yes: bool }` in `main.rs`
2. New `briefc/src/skillsync.rs`:
   - Load `brief.toml` → collect all `[skills.X]` with `mcp_command` or `mcp_url`
   - For each skill: reuse `run_mcp_list_session()` from `verifier.rs` (already parses tools/list)
   - Need full metadata: name, description, inputSchema — `run_mcp_list_session` may need enhancement to return full JSON (not just `Vec<String>`)
   - Parse `inputSchema.properties` → Brief types: `"string"` → `String`, `"integer"|"number"` → `Int`, `"boolean"` → `Bool`, `"object"|"array"` → `String`
   - Output path: `SkillConfig.path` if set, else `.claude/skills/<Name>/`
   - If `.briefskill` already exists: print unified diff; `--yes` to skip confirmation
3. Make `FnSig` and `render_briefskill` `pub(crate)` in `skillgen.rs`

**Enhancement to `run_mcp_list_session()`:** Return `Vec<ToolMeta>` instead of `Vec<String>` where `ToolMeta { name, description, input_schema }`. Parse from the `tools/list` response's `result.tools` array.

**Tests:**
- Unit: mock `tools/list` JSON → correct `.briefskill` content (all 4 type mappings)
- Unit: output path when `path` set vs. not set
- Idempotency: two runs produce identical output
- Diff shown when file differs; no-op when identical

---

## Concept C — `brief serve --record` + `brief audit` (Priority #3)

**What it does:** JSONL trace of EVERY tool call through Brief's proxy — including BLOCKED ones. `brief audit` summarizes policy violations.

### `--record` implementation

**Decouple from allow/deny:** `--record` is implemented independently of the allow/deny enforcer. It wraps the EXISTING `uses[]/forbids{}` decision logic too. This allows delivering observability value immediately and validating the allow/deny design with real traces before enforcement is complete.

1. Add `--record <file>` to `Commands::Serve`
2. In `proxy_tool_call()` AFTER all enforcement decisions: append one JSONL line per call

**Session header (first line written at startup):**
```jsonl
{"event":"session_start","task":"review-pr","brief_version":"0.x.x","policy_hash":"sha256:abc123","ts":"2024-01-15T10:23:40Z"}
```
`policy_hash` = SHA-256 of the serialized allow/deny/uses/forbids policy. Used by `brief audit` to warn if the current spec differs from recorded spec.

**Call entries:**
```jsonl
{"event":"call","ts":"2024-01-15T10:23:45Z","skill":"GitHub","fn":"get_file_contents","args_schema":{"owner":"acme","repo":"brief","path":"src/main.rs"},"allowed":true,"ms":234,"result_size":1024,"result_error":false}
{"event":"call","ts":"2024-01-15T10:23:47Z","skill":"FileSystem","fn":"write_file","args_schema":{"path":"./src/main.rs","content":"[REDACTED:4521b]"},"allowed":false,"blocked_reason":"matched deny: path='./src/**'","ms":0}
```

**Trace privacy defaults:**
- Default mode: `--record-args=schema` — record arg names + value TYPES + lengths, no values
- Opt-in: `--record-args=full` — record full arg values (shows warning: "trace may contain sensitive data")
- Secret redaction (both modes): values equal to any `needs { env "X" }` value → `"[REDACTED:<hash>]"`
- Large strings (>512 bytes): replaced with `"[TRUNCATED:<len>:<hash>]"` unless `--record-args=full`
- Sensitive key names (`token`, `secret`, `password`, `authorization`, `cookie`, `key`, `content`) → value replaced with `"[REDACTED:<hash>]"` in schema mode

**Result recording:** Record `result_size` (bytes) and `result_error` (bool) for allowed calls. Never record result content.

### `brief audit` command

```
$ brief audit trace.jsonl [--fail-on-deny]

PASS  GitHub.get_pull_request(owner="acme", repo="brief", number=123)
PASS  GitHub.get_file_contents(owner="acme", repo="brief", path="src/main.rs")  
DENY  FileSystem.write_file(path="./src/main.rs") — matched deny: path="./src/**"
DENY  GitHub.merge_pull_request — not in allow{} patterns

Summary: 2 allowed, 2 blocked
```

1. New command `Commands::Audit { trace: PathBuf, fail_on_deny: bool }`
2. New `briefc/src/audit.rs`: parse JSONL, warn if policy_hash differs from current spec, print table
3. **Exit codes:** Default exit 0 (report only). `--fail-on-deny` → exit 1 if any blocked call.
4. Policy staleness warning: "⚠ Policy has changed since trace was recorded (hash mismatch)"

**Tests:** trace with blocked call + `--fail-on-deny` → exit 1; same trace without flag → exit 0; policy hash mismatch → warning printed; secret values redacted in output.

---

## Concept D — `brief suggest` (Priority #4 — requires Concept C)

**TRIZ:** Standard 2.2.5 (Self-Regulation) + P23 (Feedback). The spec improves from observed AI behavior. This is Brief's "control element" (Evolution Law 1). With the `allow{}/deny{}` model in place, suggestions are precise and actionable (not NLP guesswork).

**What it does:**
```
$ brief suggest .brief.trace
```

Reads trace + current .brief spec → outputs:
- Calls BLOCKED by `allow{}/deny{}` → "AI needed this, but Brief blocked it — review and decide"
- Skills in trace not in `uses[]` → W501
- Skills in `uses[]` but never called in any trace → W502 (may be intentional)
- Trace arg values matching known secret env vars → W503 (reframed: NOT "env vars accessed by blocked calls" since Brief can't observe that — instead: "values in trace args match known secret env var values")

**`brief suggest --apply` safety constraint (critical):**
- `--apply` ONLY ever adds conservative, restrictive additions:
  - Add missing `needs { env "X" }` entries found in W503
  - Add missing `uses [NewSkill]` entries found in W501
- `--apply` NEVER auto-applies:
  - Expansion of `allow{}` patterns (requires human review)
  - Removal of `deny{}` entries (requires human review)
  - Any change that loosens existing policy
- Suggestions that would loosen policy are shown in output with `REVIEW REQUIRED:` prefix but never auto-applied

**W503 reframing:** Detect when trace args contain values equal to known `needs { env "X" }` values. This signals the env var is being passed as an explicit arg — suggest declaring it in `needs{}` for documentation and startup validation.

**Output (unified diff style):**
```diff
--- task "review-pr" (current)
+++ task "review-pr" (suggested)
+  needs { env "GITHUB_TOKEN" }       // accessed in 3 trace entries
   allow {
     GitHub.get_pull_request(...)
+    FileSystem.read_file(path="/tmp/**")   // AI attempted this; currently blocked
   }
```

**W5xx diagnostics:**
- W501 — MissingSkillInTrace: AI called skill not in uses[]
- W502 — UnusedSkillInTrace: skill in uses[] never called across recorded traces
- W503 — MissingNeedInTrace: env var accessed but not in needs{}

**`brief suggest --apply`:** Applies suggestions in-place (developer reviews with `git diff`).

**Tests:** trace with blocked call → suggestion to expand allow{}; trace with extra skill → W501; `--apply` patches .brief correctly.

---

## Concept E — `brief policy check` (Priority #5 — low complexity, high debugging value)

**Problem:** Debugging `allow{}/deny{}` rules requires running a real AI agent. Developers need a way to test patterns without spinning up a full MCP session.

**Command:**
```
$ brief policy check --task review-pr --tool FileSystem.write_file --args '{"path":"./src/main.rs"}'

BLOCKED  FileSystem.write_file — matched deny: FileSystem.write_file(path='./src/**')
```

1. New command `Commands::PolicyCheck { task: String, tool: String, args: String }`
2. Load `.brief` file, find task, parse `args` as JSON
3. Run `AllowDenyEnforcer::check_call()` — same code path as `brief serve`
4. Print decision + matching pattern

No MCP server needed. Pure static simulation against the spec. This makes debugging `allow{}/deny{}` rules trivial.

**Tests:** permitted call → "PERMITTED"; blocked call → "BLOCKED" + pattern; malformed args JSON → parse error.

---

## What's Removed / Deferred

| Feature | Decision | Reason |
|---------|----------|--------|
| `@desc` as W401 compiler warning | **Removed** | Rubber duck: NL matching in CI = unreliable → noise. Replaced by `brief policy suggest` |
| SmolLM2/candle | **Restored in Phase 2** | Right context: GENERATION (`brief policy suggest`, `brief skillsync --enrich`), not VERIFICATION |
| LLVM/WASM backend | Frozen | No user benefit until ecosystem exists |
| Self-hosting Stage 2 | Frozen | Elegant but inward-facing |
| REPL | Frozen | Wrong interaction model at this stage |
| Registry | Frozen | Empty registry adds confusion |
| LSP | Keep as-is | Works, don't expand |
| `brief gen` / `brief skillgen` | Keep as-is | Useful, don't expand |

---

## Concept F — `brief models install` + SmolLM2 in-process (Priority #6)

**Why candle + SmolLM2-135M is right for Brief:**
- Rust-native (fits the toolchain, no Python runtime)
- ~80MB GGUF, no server, no API keys, no internet at inference time
- SmolLM2-135M: good enough for short text generation (descriptions, policy suggestions)
- Downloads once to `~/.brief/models/`

**Used by:**
1. `brief skillsync --enrich` — enriches @desc from raw MCP description → richer patterns for policy suggestion
2. `brief policy suggest` — generates allow{}/deny{} patterns from goal + skill interfaces

**`brief models install [smollm2]`:**
- New command `Commands::Models(ModelsCmd::Install { model: Option<String> })`
- Default model: smollm2 (SmolLM2-135M GGUF)
- Downloads from Hugging Face Hub to `~/.brief/models/smollm2-135m.gguf`
- Progress bar via `indicatif`
- Verifies SHA256 checksum after download

**`brief models list`:** Shows installed models and their sizes.

**Graceful fallback everywhere:** If model not installed, commands that need it print: "💡 Run `brief models install` to enable AI-powered policy generation" and continue with degraded output.

---

## Concept G — `brief policy suggest` (Priority #7 — the LLM-language synthesis)

**What it does:** Reads a task's `goal` and available skill interfaces → generates a starting `allow{}/deny{}` spec as a diff. Developer reviews once → compiles into `.brief` → deterministic checker validates it forever.

**This is the correct way to merge LLM with the Brief language:**
> LLM writes code once. Compiler verifies it every build.

```
$ brief policy suggest --task review-pr

Analyzing goal: "Review a GitHub PR and write feedback"
Available skills: GitHub (26 functions), FileSystem (14 functions)

Suggested allow/deny spec:

  allow {
    GitHub.get_pull_request(owner=*, repo=*, number=*)
    GitHub.get_file_contents(owner=*, repo=*, ref=*, path=*)
    GitHub.list_pull_requests(owner=*, repo=*)
    FileSystem.write_file(path="/tmp/**")
    FileSystem.read_file(path="/tmp/**")
  }
  deny {
    GitHub.merge_pull_request
    GitHub.create_pull_request
    GitHub.delete_branch
    FileSystem.write_file(path="**/.env")
    FileSystem.write_file(path="./src/**")
  }

💡 Add this to your task block. Run `brief check` to validate.
```

**Implementation:**
1. New command `Commands::PolicySuggest { task: String }`
2. New `briefc/src/policy_suggest.rs`:
   - Load `.brief`, find task by name
   - Load all skill interfaces from `uses[]`
   - Build prompt: goal + function list with descriptions
   - Run SmolLM2-135M via candle (or degrade gracefully if absent)
   - Parse LLM output → CallPattern list
   - Validate patterns through E423 checker (filter invalid ones)
   - Output as diff to apply
3. `--apply` flag: patches allow{}/deny{} blocks into .brief file
4. All generated patterns go through E423 validation before output (LLM can't generate invalid patterns)

**Prompt structure (carefully engineered for SmolLM2-135M):**
```
Task goal: "Review a GitHub PR and write feedback"
Available functions:
- GitHub.get_pull_request: Get details of a pull request
- GitHub.get_file_contents: Get file contents from a repository
- GitHub.merge_pull_request: Merge a pull request into the base branch
- FileSystem.write_file: Write content to a file
[...]
Generate: allow list (needed), deny list (risky/unnecessary for this goal)
```

**Fallback without model:** Suggest minimal allow based on keyword heuristics (read → get_, write → write_, review → PR-related functions). Lower quality but always works.

**Tests:** goal "review PR" → allow includes get_pull_request, deny includes merge; E423 filters invalid function names from LLM output; fallback without model → heuristic output.

---

## Implementation Order

These todos must be executed in dependency order:

**Core policy enforcement (ship first):**
1. `ast-allow-deny` — AST types (CallPattern, ArgPattern with serde_json::Value)
2. `parser-allow-deny` — parse allow{}/deny{} blocks (depends on 1)
3. `checker-allow-deny` — E423 + E424 + W408 + W409 (depends on 1 + 2)
4. `enforcer-allow-deny` — AllowDenyEnforcer in enforcer.rs (depends on 1)
5. `policy-check-command` — `brief policy check` dry-run (depends on 4)
6. `serve-allow-deny` — wire enforcer into proxy_tool_call (depends on 4 + 5)

**Observability (ship second):**
7. `serve-record` — `--record` + JSONL trace with policy_hash + privacy defaults (depends on 6)
8. `audit-command` — `brief audit` with `--fail-on-deny` + hash check (depends on 7)

**Skillsync (independent, run in parallel with 1-8):**
9. `skillsync-meta` — enhance run_mcp_list_session to return Vec<ToolMeta>
10. `skillsync-command` — `brief skillsync` + `--enrich` flag (depends on 9)

**LLM integration (depends on skillsync + enforcer):**
11. `models-install` — `brief models install` + candle SmolLM2-135M (depends on 10)
12. `policy-suggest-command` — `brief policy suggest` + LLM integration (depends on 4 + 11)

**Feedback loop (last):**
13. `suggest-command` — `brief suggest` with safe-only `--apply` (depends on 7 + 8)

