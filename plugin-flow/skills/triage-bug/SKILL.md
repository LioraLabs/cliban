---
name: triage-bug
description: "Turn a bug report into a board ticket that someone can actually act on — search for duplicates, try to reproduce, decide whether it's real, then file it with a reproduction, label, and priority. Use when something is reported broken, throwing, failing, or slow and there is no ticket for it yet."
requires_skills: [cliban-workflow]
---

# Triage Bug

A report arrives. Decide whether it is real, then leave the board holding something the next person can act on without re-asking the reporter anything.

**Not this skill:** a ticket that already exists and needs a root cause — that's `diagnose-issue`. Feature work — that's `explore-feature`.

## 1. Search the board before anything else

```bash
cliban issue ls --search "<symptom terms>" --json    # fuzzy: title, key, labels, description
cliban issue ls --search "<the error string>" --json
```

Search twice — once for how the *user* described it, once for the literal error text. They rarely match the same tickets.

- **A ticket already covers it** → don't file a second. Add what's new to the existing one (`issue log`, or `append-section` for a new reproduction) and say which key it landed on.
- **Something adjacent** → note the key; you will link it with `--related-to` when you file.

Duplicate bug tickets are worse than duplicate feature tickets: two people debug the same thing from different halves of the evidence.

## 2. Try to reproduce

Get the exact invocation, input, environment, and expected-vs-actual from the report. What's missing, go find — the reporter is the last resort, not the first.

The bar is **one command someone else can run**. Not "click around the settings page"; a command, a script, a test invocation, a curl. If the report only supports a manual sequence, write the sequence down as numbered steps and say plainly that it isn't automated yet.

Reproducing is not diagnosing. Stop when the symptom appears — do not start forming theories about why. That's the next skill, and doing it here is how triage turns into an afternoon.

## 3. Decide

Exactly one of:

**Real, reproduced.** File it (step 4) with the reproduction.

**Real, not yet reproducible.** File it anyway — a bug you can't yet trigger is still a bug, and losing it is worse than holding it. Record precisely what you tried and what you'd need (environment access, a captured trace, their exact version). Put `**Cannot reproduce yet:**` at the top of the spec so nobody assumes the repro line works.

**Not a bug.** Say so and don't file. Expected behavior, a usage error, a duplicate, or something already fixed on `main`. Explain which, and where the behavior is specified. Filing "just in case" spends someone else's triage time twice.

Genuinely can't tell? That is *Real, not yet reproducible* with a note on what would settle it — not a fourth category, and not a reason to stall.

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

**Environment:** version, OS, config that matters. Omit what doesn't.

**First seen / last known good:** a commit, release, or date, when known —
this is what makes a bisect possible later.
EOF
````

- **Title the symptom, never the cause.** "Ordering collapses after ~50 reorders" survives being wrong; "Fix f64 position drift" becomes a lie the moment the cause turns out to be elsewhere, and it's the first thing the next search matches on.
- **Priority is impact, not annoyance.** `urgent` = data loss, corruption, or everyone blocked. `high` = a core path broken with no workaround. `medium` = the default, and most bugs. Reach past `medium` only when you can name who is blocked.
- **Redact secrets** from anything you paste — tokens, keys, customer data. `<REDACTED>` in their place. A pasted log with a live credential is a second incident.
- No hypotheses in the spec. If you formed one anyway, `issue log` it as a lead — it belongs in the timeline where it's marked as a guess, not in the spec where it reads as a finding.

## 5. Hand off

Report the key, the priority, and which it was — reproduced, or not yet.

Then offer the next step: `diagnose-issue` to find the root cause, or `complete-issue` directly when the cause is already obvious from the reproduction and the fix is small. Filing is where this skill ends; starting the work is the user's call.
