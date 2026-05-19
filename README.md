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

`cliban issue add --project CLI` with no `--title` opens `$EDITOR` (`$VISUAL` first, fall back to `vi`) with a YAML-frontmatter + markdown buffer. Same flow inside the TUI: select a card and press `e`. Pass `--no-editor` or set `$CLIBAN_NO_EDITOR=1` to disable the editor (useful for agents that should never block).

## Documentation

- Design spec: `docs/specs/2026-05-19-cliban-design.md`
- Implementation plan: `docs/plans/2026-05-19-cliban.md`
- Skill bundle for Claude Code: `skill/cliban-skill/SKILL.md`

## Test

```bash
go test ./...
./scripts/smoke.sh
```
