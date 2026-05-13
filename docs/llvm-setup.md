# LLVM Backend Setup

`brief build` compiles `.brief` files to native binaries via LLVM IR using the [`inkwell`](https://github.com/TheDan64/inkwell) crate. This document walks through enabling the backend.

---

## Requirements

| Component | Minimum version |
|-----------|----------------|
| LLVM      | 18.x           |
| Rust      | 1.75+          |
| `briefc`  | built with `--features llvm-backend` |

---

## Step 1 — Install LLVM 18

### macOS (Homebrew)

```bash
brew install llvm@18
```

### Ubuntu / Debian

```bash
wget -qO- https://apt.llvm.org/llvm-snapshot.gpg.key | sudo apt-key add -
sudo add-apt-repository "deb http://apt.llvm.org/$(lsb_release -cs)/ llvm-toolchain-$(lsb_release -cs)-18 main"
sudo apt-get update
sudo apt-get install llvm-18 llvm-18-dev clang-18
```

### Windows (Chocolatey)

```powershell
choco install llvm --version 18.1.0
```

---

## Step 2 — Add `inkwell` to `Cargo.toml`

Open `briefc/Cargo.toml` and add the following to `[dependencies]` and `[features]`:

```toml
# Add to [dependencies]:
inkwell = { git = "https://github.com/TheDan64/inkwell", branch = "master",
            features = ["llvm18-0"], optional = true }

# Update [features]:
llvm-backend = ["inkwell"]
```

---

## Step 3 — Set `LLVM_SYS_180_PREFIX`

The `llvm-sys` crate (a transitive dependency of `inkwell`) needs to know where LLVM is installed.

### macOS

```bash
export LLVM_SYS_180_PREFIX=$(brew --prefix llvm@18)
```

Add this to your shell profile (`~/.zshrc` / `~/.bash_profile`) to persist it.

### Ubuntu

```bash
export LLVM_SYS_180_PREFIX=/usr/lib/llvm-18
```

### Windows

Set the environment variable to the LLVM installation directory:

```powershell
$env:LLVM_SYS_180_PREFIX = "C:\Program Files\LLVM"
```

---

## Step 4 — Build with the feature

```bash
cargo build --features llvm-backend

# Or install globally:
cargo install --path briefc --features llvm-backend
```

---

## Usage

```bash
# Compile to a native binary:
brief build hello.brief

# Custom output path:
brief build hello.brief -o myapp

# Emit LLVM IR for inspection:
brief build hello.brief --emit-ir
cat hello.ll
```

---

## How it works

The Brief LLVM backend follows this pipeline:

```
.brief source
    ↓  (lex + parse)
AST (Program)
    ↓  (check + typeck)
Type-checked Program
    ↓  (codegen)
LLVM IR Module (inkwell)
    ↓  (LLVM optimization passes)
Object file (.o)
    ↓  (cc / lld linker)
Native binary
```

### Task → LLVM function

Each `task` declaration becomes an LLVM function:

```brief
task Hello : TaskBrief {
    goal = "Say hello"
    step Greet {
        perform IO.print("Hello, world!")
    }
}
```

Compiles to roughly:

```llvm
define void @Hello() {
entry:
  call void @brief_rt_print(ptr @str.task)
  call void @brief_rt_print(ptr @str.step.Greet)
  call void @brief_rt_perform(ptr @str.IO, ptr @str.print, i32 1)
  ret void
}
```

### Brief runtime stubs

The compiler emits calls to a minimal runtime:

| Function | Signature | Description |
|----------|-----------|-------------|
| `brief_rt_print` | `(msg: *i8) -> void` | Print to stdout |
| `brief_rt_perform` | `(skill: *i8, fn: *i8, argc: i32, ...) -> void` | Dispatch skill call |
| `brief_rt_exit` | `(code: i32) -> void` | Exit process |

The runtime library (`brief_rt.c`) will be implemented in Phase 2 (v0.2).

---

## Troubleshooting

### `llvm-sys` cannot find LLVM

```
error: No suitable version of LLVM was found system-wide or pointed
       to by LLVM_SYS_180_PREFIX.
```

**Fix:** Ensure `LLVM_SYS_180_PREFIX` is set and points to a valid LLVM 18 installation:

```bash
ls $LLVM_SYS_180_PREFIX/bin/llvm-config
```

### Feature not found: `llvm18-0`

You may have an old version of `inkwell`. Use the `master` branch from GitHub:

```toml
inkwell = { git = "https://github.com/TheDan64/inkwell", branch = "master",
            features = ["llvm18-0"], optional = true }
```

### `compile_error!` when enabling the feature

This means `inkwell` hasn't been added to `Cargo.toml` yet — the current `Cargo.toml` ships with `inkwell` commented out. Follow Step 2 above.
