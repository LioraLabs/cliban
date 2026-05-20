# cliban

Self-hosted, AI-agent-first kanban board for the terminal.

- Single Go binary, no daemon, no CGO.
- SQLite (WAL mode) at `$XDG_DATA_HOME/cliban/cliban.db` by default.
- Three front doors: Bubble Tea TUI, flat Cobra CLI, and a Claude Code skill bundle.

## Quickstart

```bash
go build -o cliban ./cmd/cliban
./cliban init
./cliban project add CLI --name "Cliban"
./cliban issue add --project CLI --title "First issue" --priority high
./cliban           # opens the TUI
```

## Editor integration

By default `cliban issue add` and `cliban issue edit` never open an editor —
they fail fast if no content flags are supplied, which is the right behavior
for agents. Pass `--editor` to opt in to the YAML-frontmatter + markdown
buffer in `$EDITOR` (`$VISUAL` first, falls back to `vi`). Inside the TUI,
select a card and press `e`.

## Shell completion

cliban prints completion scripts via the standard `completion` subcommand. To
install, pick the one-liner for your shell:

```bash
# bash (system-wide)
cliban completion bash | sudo tee /etc/bash_completion.d/cliban > /dev/null

# zsh (user)
mkdir -p ~/.zsh/completions
cliban completion zsh > ~/.zsh/completions/_cliban
# then add `fpath=(~/.zsh/completions $fpath)` and `autoload -U compinit && compinit` to ~/.zshrc

# fish
cliban completion fish > ~/.config/fish/completions/cliban.fish

# PowerShell
cliban completion powershell | Out-String | Invoke-Expression
```

## Documentation

- Design spec: `docs/specs/2026-05-19-cliban-design.md`
- Implementation plan: `docs/plans/2026-05-19-cliban.md`
- Skill bundle for Claude Code: `skill/cliban-skill/SKILL.md`

## Test

```bash
go test ./...
./scripts/smoke.sh
```
