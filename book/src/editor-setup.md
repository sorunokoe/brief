# Editor Setup for Brief

The `brief lsp` command starts a Language Server Protocol server over stdin/stdout,
providing real-time diagnostics and hover information in any LSP-capable editor.

## VS Code

Install the **Brief** extension (coming soon), or configure manually:

1. Install [`briefs-lsp`](https://marketplace.visualstudio.com/items?itemName=brief-lang.brief) — *placeholder until published*

2. Or add this to your `.vscode/settings.json` using the generic
   [LSP client extension](https://marketplace.visualstudio.com/items?itemName=matklad.lsp-client):

```json
{
  "lsp-client.servers": [
    {
      "name": "brief",
      "command": ["brief", "lsp"],
      "filetypes": ["brief"]
    }
  ]
}
```

3. Associate `.brief` files with the language ID:

```json
{
  "files.associations": {
    "*.brief": "brief"
  }
}
```

## Zed

Add to `~/.config/zed/settings.json`:

```json
{
  "lsp": {
    "brief": {
      "binary": {
        "path": "brief",
        "arguments": ["lsp"]
      }
    }
  },
  "languages": {
    "Brief": {
      "language_servers": ["brief"]
    }
  }
}
```

## Neovim (nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.brief then
  configs.brief = {
    default_config = {
      cmd = { 'brief', 'lsp' },
      filetypes = { 'brief' },
      root_dir = lspconfig.util.root_pattern('*.brief', '.git'),
      settings = {},
    },
  }
end

lspconfig.brief.setup {}
```

## Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "brief"
scope = "source.brief"
file-types = ["brief"]
language-servers = ["brief-lsp"]

[language-server.brief-lsp]
command = "brief"
args    = ["lsp"]
```

## What the LSP provides

| Feature      | Status |
|--------------|--------|
| Diagnostics  | ✅ all E/W codes published on open/change/save |
| Hover        | ✅ effect function signatures on `perform` calls |
| Completions  | 🔜 coming in v0.2 |
| Go-to-def    | 🔜 coming in v0.2 |
| Rename       | 🔜 coming in v0.2 |

## Installing `brief`

```sh
cargo install --path briefc
```

After installation, `brief` is available on your `$PATH`.

## Shell Completions

Generate and install completion scripts for your shell:

```sh
# Bash — add to ~/.bashrc
brief completions bash >> ~/.bashrc
source ~/.bashrc

# Zsh — add to ~/.zshrc
brief completions zsh >> ~/.zshrc
source ~/.zshrc

# Fish
brief completions fish > ~/.config/fish/completions/brief.fish

# PowerShell — add to $PROFILE
brief completions powershell >> $PROFILE
```

After installing, typing `brief <TAB>` will complete subcommands and flags.
