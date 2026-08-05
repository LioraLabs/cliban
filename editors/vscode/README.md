# Cliban Board for VS Code

The [cliban](https://github.com/LioraLabs/cliban) kanban board as a VS Code
webview: watch the cards your agents move, and move them yourself.

Everything goes through the `cliban` CLI's JSON contract — the extension has
no database coupling, so it stays correct across cliban's schema migrations.
Mutations run the same atomic, timeline-recorded code paths agents use.

## Features

- **Board** — the five cliban columns (backlog · in progress · blocked ·
  in review · done), cards with priority, labels, milestone, claim badge, and
  parent links. Drag a card between columns to move it (`cliban issue mv`).
- **Edit as a document** — click a card and the issue's full description
  opens as a real markdown editor tab (`cliban:/CLI-42.md`), the equivalent
  of the TUI's `e`. Type, Ctrl+S, and the description is written back through
  the CLI — compare-and-swap guarded, so if an agent edited the issue since
  you opened it, the save fails with a "revert to load the latest" error
  instead of overwriting.
- **Issue drawer** — the ▤ button on a card opens a quick-view drawer: spec,
  plan, notes, and the merged activity timeline. Tick plan steps live
  (`issue tick`), append log notes (`issue log`), edit
  priority/labels/milestone, and edit sections raw — all CAS-guarded too.
- **Create issues** — title, status, priority, labels, milestone, markdown
  description.
- **Live board** — the extension watches the SQLite files and refreshes when
  any other session (an agent, the TUI, another terminal) mutates the board;
  a collapsible activity feed shows who did what, when.

## Requirements

The `cliban` binary, v0.6.0 or newer:

```
cargo install cliban        # or: brew install lioralabs/tap/cliban · AUR: cliban
```

## Commands

| Command | Action |
|---|---|
| `Cliban: Open Board` | open (or focus) the board panel |
| `Cliban: Switch Project` | pick the project the board shows |
| `Cliban: Switch Milestone` | filter the board to one milestone (or none/all) |
| `Cliban: New Issue` | open the create form |
| `Cliban: Refresh Board` | re-read everything now |
| `Cliban: Archive Done Issues` | sweep the Done column (reversible) |
| `Cliban: Open Issue as Document` | open any issue key as an editable markdown tab |

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `cliban.executablePath` | `cliban` | binary path or PATH name |
| `cliban.dbPath` | – | passed as `--db`; empty uses cliban's default chain (`$CLIBAN_DB`, XDG) |
| `cliban.defaultProject` | – | project key opened on first launch |
| `cliban.watch.mode` | `auto` | `auto` (file watch, poll fallback) · `poll` · `off` |
| `cliban.watch.pollIntervalSeconds` | `15` | poll cadence |

## Known limits

- Cards reorder **between** columns, not within one — within-column position
  is not reachable through the cliban CLI yet. A same-column drop snaps back.
- Section editors appear only on sections that already exist on the issue.
- Distributed as a local `.vsix` for now (no marketplace listing).

## Development

```
npm install
npm run check       # typecheck + build + tests
npx vsce package    # produce the .vsix
```

Launch for hacking: open this folder's repo in VS Code and run the
Extension Development Host with
`code --extensionDevelopmentPath=$PWD/editors/vscode`.
