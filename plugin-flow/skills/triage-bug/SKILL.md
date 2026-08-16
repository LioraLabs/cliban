---
name: triage-bug
description: "Turn a bug report into an actionable board ticket: search duplicates, try to reproduce, decide whether it's real, file with reproduction, label, and priority. Use when something is reported broken, throwing, failing, or slow and no ticket exists yet."
requires_skills: [cliban-workflow]
---

# Triage Bug

Decide whether the report is real, then leave the board holding something the
next person can act on without re-asking the reporter. Not this skill: an
existing ticket needing a root cause (`diagnose-issue`); feature work
(`explore-feature`).

**Load first:** `cliban-flow:cliban-workflow` and `cliban:cliban` — neither
loads on its own.

## 1. Search the board first

```bash
cliban issue ls --search "<symptom terms>" --json
cliban issue ls --search "<the error string>" --json
```

Search twice — the user's words and the literal error text rarely match the
same tickets. A ticket already covers it → add what's new there (`issue log`,
or `append-section` for a new reproduction) and say which key. Something
adjacent → note the key for `--related-to`. A duplicate bug ticket makes two
people debug the same thing from different halves of the evidence.

## 2. Try to reproduce

Get the exact invocation, input, environment, and expected-vs-actual. What's
missing, go find — the reporter is the last resort. The bar is one command
someone else can run; when only a manual sequence works, write it as numbered
steps and say plainly it isn't automated yet. Stop when the symptom appears —
theories about why are `diagnose-issue`'s job, and forming them here is how
triage becomes an afternoon.

## 3. Decide

Exactly one of:

- **Real, reproduced** — file it (step 4) with the reproduction.
- **Real, not yet reproducible** — file it anyway, `**Cannot reproduce
  yet:**` atop the spec, with what you tried and what would settle it.
  "Genuinely can't tell" is this category, not a reason to stall.
- **Not a bug** — expected behavior, a usage error, a duplicate, or already
  fixed; say which and where the behavior is specified, and don't file.

## 4. File it

````bash
cliban issue add "<symptom, not guessed cause>" --project <KEY> \
  --label bug --priority <medium|high|urgent> \
  --related-to <ADJACENT-KEY> \
  --description-file - --json <<'EOF'
## Spec

**Symptom:** what the user sees, in their terms.

**Reproduction:**
```
<the one command, or numbered manual steps>
```

**Expected:** …
**Actual:** …

**Environment:** version, OS, config that matters.

**First seen / last known good:** commit, release, or date — what makes a
bisect possible later.
EOF
````

- **Title the symptom, never the cause** — a cause-title becomes a lie the
  moment the cause moves, and the title is what the next search matches.
- **Priority is impact:** `urgent` = data loss, corruption, or everyone
  blocked; `high` = a core path with no workaround; `medium` = the default and
  most bugs. Reach higher only when you can name who is blocked.
- **Redact secrets** from anything pasted — a log with a live credential is a
  second incident.
- Hypotheses go to `issue log` as marked guesses, never into the spec where
  they read as findings.

## 5. Hand off

Report the key, the priority, and reproduced-or-not. Offer `diagnose-issue`,
or `complete-issue` directly when the cause is obvious from the reproduction
and the fix is small. Starting the work is the user's call.
