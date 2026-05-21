## Research Report: Projects Similar to Brief

### Summary

Brief occupies a genuinely novel intersection: it is a **standalone compiled DSL** (with its own syntax and file format), not a library embedded in Python/TypeScript, that produces **sealed, verifiable contracts** (`.brief.lock`) serving as **MCP proxy gateways with enforced scope**. No single project combines all three of those properties simultaneously. However, across five categories, I found **~20 meaningfully comparable projects**, several of which share 1-2 of Brief's core properties. The closest conceptual relatives are **TypeChat** (type-based contract enforcement for LLM I/O), **AvaKill** (YAML policy enforcement as MCP proxy), and **mcp-proxy** (Rust-built capability-filtering MCP reverse proxy) — all significantly less opinionated than Brief but sharing its "boundary layer" stance.

---

## Category 1: AI Agent Frameworks with Typed/Structured Task Definitions

### 1. LangGraph
- **URL**: https://github.com/langchain-ai/langgraph
- **Stars**: ~15K+ (PyPI: millions of downloads)
- **Tech stack**: Python (primary), JS/TS (langgraphjs)
- **Problem it solves**: Low-level orchestration of stateful, long-running agents using directed graphs. Nodes = functions, edges = conditional transitions. Provides durable execution (resume from failure), human-in-the-loop via interrupt points, and persistent memory.
- **Similar to Brief**: Both model agent workflows as structured, composable units. Both support human-in-the-loop control points. LangGraph is the dominant framework for "agentic" multi-step work.
- **Key differences**: No DSL — workflows are Python code using `StateGraph`. No concept of a sealed contract, no lock file, no static type-checking of the workflow definition, no `uses[]` scope enforcement. Tool access is unrestricted — any LangChain tool is callable. MCP is supported via `langchain-mcp-adapters`, but it loads all tools from the MCP server — no allowlisting per task. The AI can discover and call any exposed tool.
- **Citation**: `langchain-ai/langgraph:README.md:1-45`

---

### 2. Haystack (by deepset)
- **URL**: https://github.com/deepset-ai/haystack
- **Stars**: ~22K
- **Tech stack**: Python
- **Problem it solves**: Modular pipeline-based AI framework for RAG, search, and agentic workflows. Pipelines are Python component graphs; supports MCP servers via [Hayhooks](https://github.com/deepset-ai/hayhooks).
- **Similar to Brief**: Components have explicit typed inputs/outputs (validated by Pydantic at connection time). Explicit "context engineering" philosophy — you control exactly what information reaches the model. Supports MCP servers as backends.
- **Key differences**: Typed connections are at the Python level, not a standalone DSL. No static pre-execution verification step (no equivalent to `brief check`). No lock file or sealed contract concept. Components can call any tool they're programmed with — no per-task `uses[]` scope. MCP is exposed as a server endpoint via Hayhooks, not as a typed proxy per task.
- **Citation**: `deepset-ai/haystack:README.md:44-90`

---

### 3. Dify
- **URL**: https://github.com/langgenius/dify
- **Stars**: ~100K+
- **Tech stack**: Python (backend), TypeScript (frontend)
- **Problem it solves**: Visual-canvas LLM app platform with workflow builder, RAG pipelines, agent capabilities, and LLMOps observability. Drag-and-drop.
- **Similar to Brief**: Visual workflow definitions can be serialized (exported as YAML/JSON). Agent tools are explicitly configured per app. Has a concept of "what tools this agent can use" within each app definition.
- **Key differences**: Fundamentally GUI-first, not a text DSL. No compiler or static type system. No `.lock` file concept. Tool scope is loose — agents can be given many tools without constraint enforcement. No MCP proxy gateway model. Not code-first.
- **Citation**: `langgenius/dify:README.md:54-95`

---

### 4. CrewAI
- **URL**: https://github.com/crewAIInc/crewAI
- **Stars**: ~35K+
- **Tech stack**: Python
- **Problem it solves**: Multi-agent framework where you define `Agent` objects (with roles, goals, tools) and `Task` objects (with description, expected output, assigned agent). Agents collaborate in sequential or hierarchical crews.
- **Similar to Brief**: Tasks have explicit `expected_output` type and `tools` list. Agents are scoped to specific tools. `@before_kickoff` hooks act like `needs{}` prerequisites. Supports YAML-based crew/task definitions (`crew.yaml`, `tasks.yaml`).
- **Key differences**: The YAML file is a data config, not a compiled DSL. No static type checker. No verifier phase. No lock file. Tool scope can be overridden at runtime. No MCP proxy — tools are Python callables, not protocol-backed. "Task" is more of a natural-language directive than a typed contract.
- **YAML definition example**:
  ```yaml
  # tasks.yaml (CrewAI)
  research_task:
    description: "Research {topic} thoroughly"
    expected_output: "Comprehensive report"
    agent: researcher
  ```
- **Citation**: `crewAIInc/crewAI:README.md` (preview only due to size)

---

### 5. Marvin (PrefectHQ)
- **URL**: https://github.com/PrefectHQ/marvin
- **Stars**: ~5K
- **Tech stack**: Python (built on PydanticAI)
- **Problem it solves**: Task-centric AI workflow framework. `Task` objects have typed `result_type`, `instructions`, and `tools`. Agents are specialized with instructions and reusable tool sets. `Thread` manages shared context across tasks. Supersedes ControlFlow.
- **Similar to Brief**: Strongly task-centric (`marvin.Task`). Type-safe results via Pydantic. "Mise en place" philosophy (similar to Brief's preflight concept) — tasks declare what they need. Tools are attached per-agent.
- **Key differences**: Embedded Python library, not a standalone DSL. No static compile step. No lock file. Tool scope is advisory (Python objects), not enforced at a protocol boundary. No MCP proxy model. No `forbids{}` concept.
- **Citation**: `PrefectHQ/marvin:README.md:1-80`

---

### 6. Semantic Kernel / Microsoft Agent Framework
- **URL**: https://github.com/microsoft/semantic-kernel → now https://github.com/microsoft/agent-framework
- **Stars**: ~23K
- **Tech stack**: Python, .NET, Java
- **Problem it solves**: Enterprise AI orchestration SDK with plugins (tools), agents, multi-agent systems. Supports MCP as a plugin source. `Process Framework` models complex business workflows. YAML/JSON agent definitions.
- **Similar to Brief**: Explicit `Plugin` model (tools bundled into named capability sets — like Brief's skills). MCP integration as a first-class plugin source. YAML-defined agents in newer versions.
- **Key differences**: Plugins are additive — agents can have many plugins without scope enforcement. No concept of a sealed contract/lock. No `forbids{}`. The Process Framework uses state machine semantics, not a typed DSL. Plugin tools are discovered from MCP servers wholesale, not filtered by task scope.
- **Citation**: `microsoft/semantic-kernel:README.md:1-80`

---

### 7. AutoGen (Microsoft)
- **URL**: https://github.com/microsoft/autogen
- **Stars**: ~45K
- **Tech stack**: Python (.NET partial)
- **Problem it solves**: Multi-agent conversational frameworks where agents communicate via messages. `AssistantAgent`, `UserProxyAgent`, etc. Supports tool use via function calling.
- **Similar to Brief**: Agents can have bounded tool sets. Supports human-in-the-loop approval. v0.4+ "AgentChat" has typed message protocols between agents.
- **Key differences**: Fundamentally conversational/message-passing model vs. Brief's contract DSL model. No standalone file format. No static analysis. No lock file. Tool availability is unrestricted by default.
- **Citation**: `microsoft/autogen:README.md` (preview only)

---

### 8. Temporal
- **URL**: https://github.com/temporalio/temporal
- **Stars**: ~13K
- **Tech stack**: Go (server), SDKs in Go/Java/Python/TypeScript
- **Problem it solves**: Durable workflow execution platform. Workflows are code (functions) that persist through failures. Activities = typed units of work. Workflow definitions are strongly typed with explicit I/O.
- **Similar to Brief**: **Strong structural similarity**: workflows are typed, statically analyzed by the compiler, and Activities (like Brief's `step {}` blocks) have explicit pre/postconditions. Activity timeouts and retries are declarative. The Temporal type system is enforced — if your Activity interface doesn't match the implementation, the compiler rejects it.
- **Key differences**: General-purpose durable execution engine, not AI-specific. No concept of MCP, no skill interfaces, no LLM scope enforcement. The typing is language-level (Go/Java/TypeScript types), not a new DSL. No `@enum`, `@range`-style annotations. No "boundary between human and AI" framing.
- **Citation**: `temporalio/temporal:README.md:1-60`

---

### 9. Prefect
- **URL**: https://github.com/PrefectHQ/prefect
- **Stars**: ~18K
- **Tech stack**: Python
- **Problem it solves**: Python workflow orchestration with `@flow` and `@task` decorators. Typed Python. Scheduling, retries, caching, event-based triggers.
- **Similar to Brief**: `@task` creates discrete, typed units of work. Flow parameters are type-validated. Deployment manifests declare what a flow needs (infrastructure, parameters).
- **Key differences**: Not AI-agent-specific. No DSL, not a contract boundary, no scope enforcement, no MCP. The "task" is a Python function with retries, not a behavioral spec for an AI.
- **Citation**: `PrefectHQ/prefect:README.md:48-90`

---

### 10. OpenAI Agents SDK
- **URL**: https://github.com/openai/openai-agents-python
- **Stars**: ~8K
- **Tech stack**: Python (TypeScript variant: openai-agents-js)
- **Problem it solves**: Multi-agent workflow SDK with guardrails, handoffs, MCP integration, and tracing.
- **Similar to Brief**: Has a **Guardrails** system for input/output validation. Tools are explicitly declared per agent. MCP servers are natively supported as tool sources. Sandbox agents work within isolated environments with a `Manifest` specifying workspace contents (closest Brief analog in spirit).
- **Key differences**: Python library, not a DSL. Guardrails are LLM-based (another model validates calls), not static type-checked. No lock file. MCP tools are loaded wholesale from a server — no per-task allowlist. `Manifest` describes workspace files, not tool scope.
- **Key differentiating feature found**:
  ```python
  # OpenAI Agents SDK sandbox manifest
  agent = SandboxAgent(
      default_manifest=Manifest(entries={"repo": GitRepo(...)})
  )
  ```
- **Citation**: `openai/openai-agents-python:README.md:1-80`

---

### 11. PydanticAI
- **URL**: https://github.com/pydantic/pydantic-ai
- **Stars**: ~15K+
- **Tech stack**: Python
- **Problem it solves**: Type-safe, production-grade agent framework. Agents have typed dependency injection, typed output (`output_type`), and tool functions with validated parameters. Full static type inference via mypy/Pyright.
- **Similar to Brief**: **Strongest type-safety story** of any Python agent framework. Explicit `deps_type` and `output_type` on agents. Tool parameter validation via Pydantic. "If it passes type checking, it works" philosophy — explicitly echoes Rust's "if it compiles" framing (same language Brief uses). Supports MCP and YAML/JSON agent definitions.
- **Key differences**: Library-level typing, not a standalone DSL. No `brief check` equivalent — you get type errors from mypy/Pyright, not a dedicated compiler. No lock file or sealed contract. No scope boundary enforcement at protocol level. No `@range`, `@enum` annotations on tool parameters.
- **Key similar quote** from README: *"Fully Type-safe: moving entire classes of errors from runtime to write-time for a bit of that Rust 'if it compiles, it works' feel."*
- **Citation**: `pydantic/pydantic-ai:README.md:23-40`

---

## Category 2: DSLs for AI/LLM Workflows

### 12. Microsoft Guidance
- **URL**: https://github.com/guidance-ai/guidance
- **Stars**: ~21K
- **Tech stack**: Python
- **Problem it solves**: A programming paradigm (not just a library) for steering LLMs. You write Python-like programs that interleave generation (`gen()`) and control flow. Supports regex constraints, context-free grammars, JSON schema enforcement. Can "fast-forward" tokens that are structurally predetermined.
- **Similar to Brief**: **Closest analogy to Brief's annotation system** (`@regex`, `@range`, `@enum` ≈ Guidance's `gen(regex=...)`, `select(["a","b"])`, `gen_json(schema=...)`). Both are about constraining what the AI can produce. Guidance has a `Mock` model for offline grammar validation (≈ `brief check`). Composable grammar functions (≈ Brief's skills).
- **Key differences**: Guidance constrains **LLM output generation** — it's a generation-time constraint tool. Brief constrains **what tools an AI agent can call** — it's an invocation-time scope tool. Guidance is embedded Python code, not a standalone file format. No lock file. No MCP integration. No skill interface concept.
- **Citation**: `guidance-ai/guidance:README.md:1-60`

---

### 13. LMQL
- **URL**: https://github.com/eth-sri/lmql (research) and https://lmql.ai
- **Stars**: ~3.4K
- **Tech stack**: Python/custom syntax (own `.lmql` file format!)
- **Problem it solves**: A SQL-like query language for LLMs. Programs are `@lmql.query` functions mixing prompts and Python. Hard constraints (`where THING in [...]`) are enforced at generation time. Variables have types (`[NUM: int]`). Runs its own tokenizer-level enforcement engine.
- **Similar to Brief**: **Most similar in architecture**: `.lmql` is a standalone DSL file format (like `.brief`). Typed variable declarations. Constraint enforcement that's proven at compile time. Has its own IDE playground. Composable query functions.
- **Key differences**: LMQL constrains **LLM text generation** (token-level), not **agent tool calls**. No concept of MCP, skill interfaces, or scope enforcement. No lock file. The "contract" is between the programmer and the generation output, not between human and agent. Research artifact (PLDI'23), not production-maintained.
- **Citation**: `lmql-lang/lmql:README.md:1-50`

---

### 14. DSPy
- **URL**: https://github.com/stanfordnlp/dspy
- **Stars**: ~30K+
- **Tech stack**: Python
- **Problem it solves**: "Programming — not prompting — LMs." Declarative Python modules (`dspy.Signature`, `dspy.Module`, `dspy.ChainOfThought`). Signatures define typed input/output fields. Optimizers automatically tune prompts from examples. "Compiling" LM programs to effective prompts.
- **Similar to Brief**: Uses the word "compile" literally (`dspy.compile()`). Signatures are like Brief's typed step interfaces. Assertions (`dspy.Assert`) and suggestions provide constraint enforcement with retry.
- **Key differences**: DSPy optimizes LLM **prompts** — the "compilation" is prompt optimization, not type verification. No file format (embedded Python classes). No MCP integration. No scope enforcement. The AI's tool use is unconstrained. Focused on prompt engineering, not behavioral contracts.
- **Citation**: `stanfordnlp/dspy:README.md:1-30`

---

### 15. Instructor
- **URL**: https://github.com/567-labs/instructor (also `jxnl/instructor`)
- **Stars**: ~13K
- **Tech stack**: Python, TypeScript, Go, Ruby, Rust, Elixir
- **Problem it solves**: Structured output extraction from LLMs using Pydantic models. Wraps any LLM provider with a `response_model=` parameter. Handles JSON schema generation, validation, and automatic retry on validation failure.
- **Similar to Brief**: Type-safe LLM outputs. Pydantic validators act like Brief's `@enum`, `@range` annotations. Works across providers. The Rust implementation is notable.
- **Key differences**: Instructor is a **response parsing** library — it ensures the LLM *returns* data matching a schema. Brief is a **task behavioral spec** — it ensures the AI *calls tools* within declared scope. No file format, no lock file, no MCP proxy, no scope enforcement.
- **Citation**: `567-labs/instructor:README.md:1-40`

---

### 16. Outlines (dottxt-ai)
- **URL**: https://github.com/dottxt-ai/outlines
- **Stars**: ~15K+
- **Tech stack**: Python (Rust backend via faster-outlines)
- **Problem it solves**: Structured text generation for LLMs — constrain generation to regex, JSON schema, Pydantic models, context-free grammars at the **token sampling level** (not post-hoc validation). Used by vLLM, Cohere, HuggingFace, NVIDIA.
- **Similar to Brief**: Constraint enforcement that's statically definable. Schema-based typing. The Rust backend (faster-outlines) is conceptually parallel to Brief's Rust compiler — both are compile-time enforcement systems.
- **Key differences**: Generation-time constraint on LLM output tokens, not agent tool invocation scope. No file format, no lock files, no MCP, no skill interfaces.
- **Citation**: `dottxt-ai/outlines:README.md` (preview)

---

### 17. TypeChat (Microsoft)
- **URL**: https://github.com/microsoft/TypeChat
- **Stars**: ~11K+
- **Tech stack**: TypeScript (primary), Python, C#
- **Problem it solves**: Replaces prompt engineering with **schema engineering**. You define TypeScript types (or Python/C# types) representing intents, TypeChat constructs a prompt from those types, validates the LLM response against the schema, and repairs non-conforming output through further LLM interaction.
- **Similar to Brief**: **Conceptually closest to Brief's philosophy**: "Types are all you need." The schema IS the contract. TypeScript type definitions act as the spec. Validation with repair is similar to Brief's `verify` phase catching mismatches. "Schema engineering vs. prompt engineering" directly parallels Brief's "contract vs. prompt."
- **Key differences**: TypeChat handles **NL input → structured JSON output**; Brief handles **task scope → MCP tool calls**. TypeChat is a library, not a standalone DSL. No lock file. No scope enforcement at protocol level. No MCP proxy.
- **Key quote from README**: *"TypeChat replaces prompt engineering with schema engineering."* (Echoes Brief's philosophy exactly.)
- **Citation**: `microsoft/TypeChat:README.md:1-40`

---

## Category 3: MCP-Related Tools

### 18. mcp-proxy (joshrotenberg)
- **URL**: https://github.com/joshrotenberg/mcp-proxy
- **Stars**: 2 (very new, March 2026)
- **Tech stack**: **Rust** (built on tower middleware)
- **Problem it solves**: Config-driven MCP reverse proxy. Aggregates multiple MCP backends behind one endpoint with per-backend capability filtering (`expose_tools`, `hide_tools`), argument injection, circuit breakers, rate limiting, JWT/RBAC auth, audit logging.
- **Similar to Brief**: **Architecturally closest to Brief's `brief serve` command**: Rust-built, MCP proxy, capability filtering (allowlist/denylist tools per backend), multi-backend aggregation. `expose_tools` = `uses[]`. `hide_tools` = `forbids{}`. TOML config (like Brief's `brief.toml`).
- **Key differences**: Pure runtime proxy — no static type system, no DSL, no `brief check`, no annotations, no lock file. No concept of a human-readable task spec. Closer to infrastructure middleware than a workflow definition language.
- **Example config**:
  ```toml
  [[backends]]
  name = "files"
  expose_tools = ["read_file", "list_directory"]  # ≈ Brief's uses[]
  hide_tools = ["write_file", "delete_file"]       # ≈ Brief's forbids{}
  ```
- **Citation**: `joshrotenberg/mcp-proxy:README.md:1-80`

---

### 19. LangChain MCP Adapters
- **URL**: https://github.com/langchain-ai/langchain-mcp-adapters
- **Stars**: ~2K
- **Tech stack**: Python
- **Problem it solves**: Makes MCP server tools compatible with LangChain/LangGraph agents. `MultiServerMCPClient` connects to multiple MCP backends and loads tools from them.
- **Similar to Brief**: Multi-MCP-backend connectivity pattern. Tools from multiple servers are aggregated and forwarded to the agent.
- **Key differences**: No filtering or scope enforcement — all tools from connected MCP servers are exposed to the agent. No types, no annotations, no lock files. Acts as a bridge, not a gateway.
- **Citation**: `langchain-ai/langchain-mcp-adapters:README.md:1-60`

---

## Category 4: Contract/Schema Enforcement for AI Agents

### 20. AvaKill
- **URL**: https://github.com/log-bell/avakill
- **Stars**: 10 (new, Feb 2026)
- **Tech stack**: Python (with Go binary for MCP shim), AGPL-3.0
- **Problem it solves**: Safety firewall for AI agents. Intercepts tool calls before execution and evaluates them against YAML policies. Three enforcement paths: native agent hooks, MCP proxy wrapper, OS sandbox. 81 built-in rules across 14 categories (file paths, shell safety, SQL, secrets, PII).
- **Similar to Brief**: **Most operationally similar**: YAML policy file as contract (`avakill.yaml` ≈ `task.brief`). MCP proxy mode (`avakill mcp-wrap`) = `brief serve`. Tool name filtering with glob patterns ≈ `uses[]`/`forbids{}`. Human-in-the-loop approval gates. Audit logging. Policy signing. "One policy, enforced at every level" philosophy.
- **Key differences**: AvaKill is **reactive** (blocks bad calls at runtime); Brief is **proactive** (prevents bad calls from being possible via scope). AvaKill is a security firewall — it allows all tools but blocks dangerous ones based on arguments. Brief is a contract layer — it only exposes declared tools. No static type system, no DSL, no lock file, no `needs{}` prerequisites, no skill interfaces. Python library, not a standalone language.
- **Citation**: `log-bell/avakill:README.md:1-100`

---

## Category 5: Task Workflow DSLs (non-AI-specific)

### 21. Taskfile (go-task/task)
- **URL**: https://github.com/go-task/task
- **Stars**: ~14K
- **Tech stack**: Go
- **Problem it solves**: YAML-based task runner (Make alternative). Tasks declare `cmds`, `deps`, `vars`, `preconditions`. YAML schema-validated. Cross-platform.
- **Similar to Brief**: `preconditions` in Taskfile ≈ `needs{}` in Brief. `deps` ≈ task ordering in Brief. YAML task definitions as a declarative spec.
- **Key differences**: Not AI-specific. No types, no annotations, no scope enforcement. "Tasks" are shell commands, not AI agent behavioral specs. No MCP integration.
- **Citation**: `go-task/task:README.md`

---

## Unique Properties of Brief (Gap Analysis)

After surveying 21+ comparable projects, Brief's unique combination is:

| Feature | Brief | Closest rival |
|---|---|---|
| Standalone compiled DSL (own syntax/file format) | ✅ `.brief` files | LMQL (`.lmql`), Taskfile (`.yaml`) |
| Static type checking of agent workflows (no network) | ✅ `brief check` | PydanticAI (mypy/Pyright), Temporal (language compiler) |
| Verifier protocol → sealed lock file | ✅ `.brief.lock` | Nothing comparable found |
| MCP proxy with per-task tool scope enforcement | ✅ `brief serve` + `uses[]` | mcp-proxy (`expose_tools`), AvaKill (deny rules) |
| Skill interface files (`.briefskill` like `.d.ts`) | ✅ | Semantic Kernel plugins (partial) |
| `forbids{}` static + runtime scope boundary | ✅ | AvaKill (runtime deny rules only) |
| `needs{}` prerequisite verification before AI starts | ✅ | CrewAI `@before_kickoff` (partial) |
| Linear types (`@once`) in workflow steps | ✅ | Nothing comparable found |
| Composable verification via MCP protocol | ✅ | Nothing comparable found |
| Human-AI typed contract framing | ✅ | TypeChat (philosophy, not mechanism) |

---

## Gaps and Uncertainties

1. **ControlFlow** (PrefectHQ, now merged into Marvin 3.0) — its task-centric "mise en place" philosophy was very close to Brief; worth deeper inspection of the migration notes.
2. **Anthropic's Claude model context window management tools** — not directly comparable but worth monitoring.
3. **AgentKit / OpenAI Function Calling specs** — low similarity, skipped.
4. **Dify's YAML workflow export format** — could be more similar to Brief if inspected at the raw YAML level; GUI obscures the spec structure.
5. **Rivet (by Ironclad)** — visual node-based LLM programming environment; not searched directly.
6. **GitHub search limitations**: Several broad searches returned no results due to GitHub rate-limiting and search indexing constraints. Searches for "mcp gateway" and "agent contract DSL" returned zero results, which likely reflects indexing gaps rather than absence of projects.
7. **No equivalent to `.brief.lock`**: This appears to be genuinely novel — no project found uses the pattern of committing a sealed, content-addressed verification artifact that gates production execution.___BEGIN___COMMAND_DONE_MARKER___0
