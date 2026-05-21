# Brief Mermaid Diagrams

## 1. System Architecture Overview

```mermaid
graph TD
    Dev[Developer]
    BriefSrc[".brief workflow file"]
    SkillIface[".briefskill interfaces"]
    Manifest["brief.toml manifest"]

    subgraph CLI["Brief CLI / briefc"]
        Check["brief check\nstatic analysis"]
        Verify["brief verify\nseal contract"]
        Serve["brief serve\nMCP server"]
    end

    Lock[".brief.lock\nverified contract"]
    AI["MCP AI Client\n(Claude or other agent)"]

    subgraph Skills["MCP Skill Servers"]
        GitHub[GitHub]
        FileSystem[FileSystem]
        Other["Other skill servers"]
    end

    Dev --> BriefSrc
    Dev --> SkillIface
    Dev --> Manifest
    BriefSrc --> Check
    SkillIface --> Check
    Manifest --> Check
    Check --> Verify
    Verify --> Lock
    Lock --> Serve
    Serve --> AI
    AI --> GitHub
    AI --> FileSystem
    AI --> Other
    Manifest -. declares backends .-> Skills
    SkillIface -. typed tool surface .-> AI
```

This overview shows Brief as the control plane between a developer, an AI client, and real MCP skill servers. The `.brief`, `.briefskill`, and `brief.toml` inputs are compiled and verified before `brief serve` exposes a constrained tool surface to the AI.

## 2. Enforcement Chain Flowchart

```mermaid
flowchart TD
    Start["Write task.brief + .briefskill + brief.toml"] --> Check["brief check\nfast static analysis\n< 1s, no network"]
    Check --> Decision{"Any E-codes?"}
    Decision -- Yes --> Errors["E107 / E301 / E302 / E303 / E309 / E420 / E421"]
    Errors --> Fix["Edit workflow, tests, skills, or manifest"]
    Fix --> Check
    Decision -- No --> Verify["brief verify\nrun configured verifiers\n~5-30s when needed"]
    Verify --> VerifyDecision{"Verification passes?"}
    VerifyDecision -- No --> VErrors["E401 / E402 / E411\nor verifier failure"]
    VErrors --> Fix
    VerifyDecision -- Yes --> Lock["Write .brief.lock\ncommit to git"]
    Lock --> Serve["brief serve\nrequires valid lock"]
    Serve --> Surface["Expose ONLY tools in uses[]"]
    Surface --> Once["Enforce @once and protocol-level boundaries"]
    Once --> AI["AI client connects over MCP"]
    AI --> Skills["Calls real MCP skill servers"]
    Lock -. invalidated when source changes .-> Check
```

This flowchart captures Brief's no-shortcuts workflow: static checking first, then verification, then serving. It also highlights the feedback loop where E-codes force iteration until the contract is both statically clean and dynamically sealed.

## 3. Compiler Pipeline

```mermaid
graph LR
    Source[".brief source"] --> Lex["lexer.rs\nTokenize with logos"]
    Lex --> Parse["parser.rs\nBuild syntax tree"]
    Parse --> AST["ast.rs\nAST types"]
    AST --> HIR["hir.rs\nHigh-level IR"]
    HIR --> TypeCheck["typeck.rs\nType checking"]
    TypeCheck --> Check["checker.rs\nStatic analysis + E-codes"]
    Check --> Verify["verifier.rs + verify.rs\nDynamic annotation verification"]
    Verify --> Lock["lock.rs\nWrite/read .brief.lock"]
    Lock --> Serve["serve.rs\nMCP server"]

    Parse -. manifest and skills loaded during analysis .-> Manifest["manifest.rs\nbrief.toml"]
    Parse -. interfaces loaded .-> SkillLoader["skill_loader.rs\n.briefskill files"]
    Serve -. runtime backend comms .-> Backends["skill_backends.rs\nMCP subprocesses"]
```

This pipeline shows how Brief moves from raw source text to a served MCP contract. Supporting modules such as `manifest.rs`, `skill_loader.rs`, and `skill_backends.rs` feed configuration, interface loading, and runtime communication into the main compile-and-serve path.

## 4. CLI Commands Mind Map

```mermaid
mindmap
  root((Brief CLI))
    Static Analysis
      brief check
      brief watch
      brief ci
    Contract Sealing
      brief verify
    Execution
      brief serve
      brief serve --draft
      brief run
      brief test
      brief test --live
    Generation
      brief gen
      brief skillgen
      brief init
    Tooling
      brief fmt
      brief doc
      brief hir
      brief lsp
      brief build
      shell completions
```

This mind map groups Brief commands by the job they do instead of alphabetically. It makes the product shape clear: some commands analyze and seal contracts, some execute workflows, and others help generate or inspect Brief projects.

## 5. Skill Architecture

```mermaid
graph TD
    Dev[Developer]

    subgraph Interface["Interface authoring"]
        Readme["README.md\nskill behavior description"]
        Skillgen["brief skillgen"]
        InterfaceFile["GitHub.briefskill\ntyped interface"]
    end

    subgraph Runtime["Runtime binding"]
        Manifest["brief.toml\n[skills.<Name>] mcp_command"]
        Serve["brief serve"]
        Spawn["Spawn MCP subprocess"]
        MCP["Skill MCP server process"]
    end

    AI["AI client sees typed tools only"]

    Dev --> Readme
    Readme --> Skillgen
    Skillgen --> InterfaceFile
    Dev --> Manifest
    InterfaceFile --> Serve
    Manifest --> Serve
    Serve --> AI
    Serve --> Spawn
    Spawn --> MCP
    MCP --> Serve
    Manifest -. declares command and transport .-> MCP
```

This diagram separates the human-authored interface side from the runtime implementation side. In Brief, the AI reads the `.briefskill` contract, while the actual work is executed by an MCP server process declared in `brief.toml` and spawned by `brief serve`.

## 6. .brief File Anatomy

```mermaid
graph LR
    File[".brief file"] --> ImportSection["imports"]
    File --> TaskSection["task declaration"]
    File --> TestSection["test blocks"]

    subgraph ImportDetails["Import section"]
        Import1["import skill \"GitHub\""]
        Import2["import skill \"FileSystem\""]
    end

    subgraph TaskDetails["Task declaration body"]
        Header["task ReviewPR : TaskBrief"]
        Uses["uses [GitHub, FileSystem]"]
        Goal["goal = \"Fetch, review, summarize\""]
        Needs["needs { env / feature / config }"]
        Forbids["forbids { skill / func }"]
        StepSection["step blocks"]
        Perform["perform Skill.fn(...)? calls"]
    end

    subgraph ExecutionDetails["Execution body"]
        Step1["step FetchChangelog"]
        Step2["step WriteReport"]
        Once["@once bindings"]
        Values["let bindings + results"]
    end

    subgraph TestDetails["Test coverage"]
        TestBlock["test \"boundary coverage\""]
        EnumCases["@enum literals exercised"]
        RangeCases["@range boundaries exercised"]
    end

    ImportSection --> Import1
    ImportSection --> Import2
    TaskSection --> Header
    Header --> Uses
    Header --> Goal
    Header --> Needs
    Header --> Forbids
    Header --> StepSection
    StepSection --> Step1
    StepSection --> Step2
    StepSection --> Perform
    StepSection --> Once
    StepSection --> Values
    TestSection --> TestBlock
    TestBlock --> EnumCases
    TestBlock --> RangeCases
```

This anatomy diagram shows the major structural parts of a `.brief` file: imports, a typed task declaration, governance blocks, executable steps, and tests. It emphasizes that a Brief file is not just imperative code; it also contains boundary rules and test coverage requirements.

## 7. Brief File to Serve Sequence Diagram

```mermaid
sequenceDiagram
    actor Developer
    participant briefc
    participant Verifier as MCP-Verifier
    participant Lock as .brief.lock
    participant AI as AI-Client
    participant Skill as Skill-MCP-Server

    Developer->>briefc: Write task.brief + interfaces + manifest
    Developer->>briefc: brief check task.brief
    briefc-->>Developer: Static diagnostics / E-codes until clean
    Developer->>briefc: brief verify task.brief
    briefc->>Verifier: Verify dynamic annotations and live skill surface
    Verifier-->>briefc: Pass or E401 / E402 / E411
    briefc->>Lock: Write verified lock file
    Developer->>briefc: brief serve task.brief
    briefc->>Lock: Validate lock freshness
    Lock-->>briefc: Contract is current
    AI->>briefc: tools/list + tool call
    briefc-->>AI: Only uses[] tools exposed; @once tracked; forbids enforced
    briefc->>Skill: Proxy allowed MCP call
    Skill-->>briefc: Tool result
    briefc-->>AI: Typed result
```

This sequence diagram follows the happy path from authoring to live AI execution. It also shows where enforcement actually happens: Brief validates the lock, limits the exposed tool list, and only then proxies permitted calls to the underlying skill MCP server.

## 8. Error Code Map

```mermaid
graph TD
    Root["Brief error codes"]

    subgraph CheckPhase["check phase"]
        E107["E107\nMissing .briefskill interface"]
        E301["E301\n@range boundary literal missing in test"]
        E302["E302\n@enum value literal missing in test"]
        E303["E303\n.brief.lock missing / stale / source changed"]
        E309["E309\nNo configured verifier for dynamic annotation"]
        E420["E420\nForbidden skill used"]
        E421["E421\nForbidden function called"]
    end

    subgraph VerifyPhase["verify phase"]
        E401["E401\nSkill function not found in live MCP server"]
        E402["E402\nSkill MCP server unreachable"]
        E411["E411\nneeds{} prerequisite not met"]
    end

    Root --> CheckPhase
    Root --> VerifyPhase
```

This map groups errors by the phase that emits them, which is how developers usually diagnose Brief failures. The separation makes it clear that some failures are purely local and static, while others only appear when Brief verifies against live prerequisites and skill servers.

## 9. Typical Developer Workflow

```mermaid
stateDiagram-v2
    [*] --> Writing
    Writing --> Checking: save or run check
    Checking --> Fixing: E-codes reported
    Fixing --> Checking: edit .brief / tests / manifest / interfaces
    Checking --> Verifying: static analysis clean
    Verifying --> Fixing: verifier failure or stale inputs
    Verifying --> Serving: .brief.lock written
    Serving --> AIWorking: AI connects over MCP
    AIWorking --> Writing: refine workflow or requirements change
    Serving --> Verifying: source changes invalidate lock
    AIWorking --> Serving: restart or reconnect
```

This state diagram shows the normal operating loop for a Brief project. Developers bounce between writing and fixing until static checks pass, then move into verification and serving, with any source change pushing the workflow back toward verification.

## 10. Annotation System

```mermaid
graph LR
    subgraph Static["Static annotations"]
        Range["@range"]
        Enum["@enum"]
        Matches["@matches"]
        NonEmpty["@nonEmpty"]
    end

    subgraph Dynamic["Dynamic annotations"]
        URL["@url"]
        LocalPath["@local-path"]
        GitHubRepo["@github-repo"]
        ShellCmd["@shell-command"]
        Custom["@custom-*"]
    end

    Static --> Check["brief check\nno network\nfast static validation"]
    Dynamic --> Verify["brief verify\nresolve configured verifiers"]

    Verify --> Builtins["Builtin verifiers\nurl / local-path / github-repo / shell-command"]
    Verify --> MCPVerifiers["Custom MCP verifiers\nconfigured in brief.toml"]

    Check --> Tests["test{} coverage expectations\nE301 / E302"]
    Verify --> Lock["Successful verification\npermits writing .brief.lock"]
```

This diagram shows the split between annotations the compiler can prove locally and annotations that require external validation. Static annotations are handled entirely during `brief check`, while dynamic annotations are routed through configured builtin or custom verifiers during `brief verify`.
