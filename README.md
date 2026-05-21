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

## Description contract

Some cliban commands (`issue tick`, `issue promote`, `issue log`, `issue show --section`) parse the markdown structure of an issue's `description` field. They expect a small, well-defined contract:

### Top-level sections

The following H2 anchors are reserved. The exact heading text matters; cliban looks up sections by exact match.

- `## Spec` — the design/brainstorm output for this issue
- `## Plan` — the implementation plan
- `## Activity Log` — chronological events
- `## Notes` — long-lived notes (mostly for project-level descriptions)

Anything else in the description is preserved untouched.

### Plan tasks and steps

Within `## Plan`, tasks are numbered H3 headings:

```markdown
## Plan

### Task 1: short title

- [ ] **Step 1: ...**
- [ ] **Step 2: ...**

### Task 2: another short title

- [ ] **Step 1: ...**
```

- Tasks are numbered (`### Task <N>:`). Numbers must be unique within the section.
- Steps are GFM checkbox lines at column zero: `- [ ] ...` or `- [x] ...`. Indented child bullets are not parsed as steps.

### Promotion suffix

A step that has been split into its own issue is suffixed with ` → KEY`:

```markdown
- [ ] **Step 3: CSRF middleware** → CLI-18
```

This is produced by `cliban issue promote` and consumed by readers (humans, and any tooling that walks plans).

### Failure mode

If the description structure is violated (missing `## Plan` anchor, renamed `### Task N`, etc.), the workflow commands exit with code 2 and a clear error pointing at the structural problem. No best-effort recovery — fix the description and retry.

## Fuzzy-find tickets

Three coordinated surfaces share one matcher (powered by `github.com/sahilm/fuzzy`):

- `cliban issue ls --search QUERY` — pipeable. Adds a `score` field in `--json` output; respects every existing `ls` filter (`--project`, `--label`, `--milestone`, `--status`, `--priority`, `--archived`, `--no-subs`, `--parent`). `--limit N` caps results (default 50 when `--search` is set).
- `cliban fff [QUERY]` — interactive Bubble Tea picker; prints the selected key to stdout so you can compose: `cliban issue show $(cliban fff)`. Same filter flags as `ls`. Falls back to batch NDJSON mode when stdin is not a TTY (great for `cliban fff foo | jq`).
- `/` inside `cliban tui` — fuzzy filter overlay; selecting a card snaps the board cursor onto it.

The matcher weights matches across title (×3.0), key (×2.5), labels (×2.0), and description (×1.0). Default scope is all non-archived issues across all projects; narrow with `--project`, `--label`, etc.

## Test

```bash
go test ./...
./scripts/smoke.sh
```
