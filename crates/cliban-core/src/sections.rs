//! Locating H2 sections in the issue/milestone description markdown.
//!
//! The description is a contract, not free text: `## Spec`, `## Plan`,
//! `## Activity Log`, `## Notes`, and `## Files` each have an owner, and every
//! tool that edits one must leave the others byte-identical. This module is the
//! single definition of where a section starts and ends, and of how the one
//! machine-read section, `## Files`, parses.
//!
//! It lives in core rather than in the CLI's `descmd` because the Linear bridge
//! needs the same boundaries: a re-import replaces `## Spec` and must not
//! disturb the `## Plan` an agent has been ticking. Two implementations of
//! "where does this section end" would eventually disagree, and the symptom
//! would be a silently eaten plan.
//!
//! # Why a real markdown parser
//!
//! Boundaries used to be found by scanning lines for a `## ` prefix. That
//! cannot see block structure, so a column-zero `##` inside a fenced code
//! block read as a section boundary: it truncated `## Spec`, hid the real
//! `## Plan` behind a fake one, and `lint` reported the issue as clean while
//! `tick` failed with "no Task 1 in ## Plan". Whether a line is a heading is a
//! grammar question, so pulldown-cmark answers it.
//!
//! The parser is used only to *classify* — the `[start, end)` byte offsets it
//! yields still slice out of the original string, so every mutation here stays
//! a splice and round-trips byte-identically. The storage layer is unchanged.
//!
//! # What counts as a section heading
//!
//! An ATX `## Anchor` at column zero, at the top level of the document.
//! pulldown-cmark decides whether such a line is a heading *at all* (inside a
//! fence, an indented code block, or link text it is not). The three extra
//! restrictions are deliberate, and each one keeps a construct that parses
//! fine today from silently becoming a new boundary:
//!
//!   * **ATX only.** `Plan\n----` is an H2 in CommonMark, but the writers here
//!     only ever emit `## Plan`, and promoting setext would split any
//!     description with a `---` rule under a line of prose.
//!   * **Column zero only.** CommonMark allows up to three spaces of
//!     indentation. The writers emit none.
//!   * **Top level only.** A heading nested in a list item or blockquote
//!     belongs to that block, not to the document's section sequence.
//!
//! Net effect: this module is strictly better at rejecting things that were
//! never headings, and recognizes exactly the same set of real ones as before.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

/// Extensions to parse with. Deliberately none: every construct these bugs
/// turn on — fenced code, indented code, lists, lazy continuation — is core
/// CommonMark, and an extension can only add new ways for a block to swallow a
/// heading.
fn options() -> Options {
    Options::empty()
}

/// A heading: its text (the part after the `##` marker) and the byte range of
/// the heading line, including its trailing newline when it has one.
pub struct Heading {
    pub text: String,
    pub range: Range<usize>,
}

/// Every top-level ATX heading of `level` in `src`, in document order.
///
/// H2 delimits sections; H3 delimits `### Task N:` inside a plan. Both need
/// the same "is this really a heading" answer, so both come from here.
fn atx_headings(src: &str, level: HeadingLevel) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    for (ev, range) in Parser::new_ext(src, options()).into_offset_iter() {
        match ev {
            Event::Start(Tag::Heading { level: got, .. }) => {
                // `depth` is the nesting level *before* this block opens, so
                // zero means the heading is a direct child of the document.
                if got == level && depth == 0 && at_line_start(src, range.start) {
                    if let Some(text) = atx_text(&src[range.clone()], level) {
                        out.push(Heading { text, range });
                    }
                }
                depth += 1;
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    out
}

/// Every top-level ATX H2 in `desc`, in document order.
fn headings(desc: &str) -> Vec<Heading> {
    atx_headings(desc, HeadingLevel::H2)
}

/// Every top-level ATX H3 in `src`, in document order — the `### Task N:`
/// headings of a plan body. Exposed so the CLI's plan parsing and `lint` share
/// this module's answer instead of re-deriving it from line prefixes.
pub fn h3_headings(src: &str) -> Vec<Heading> {
    atx_headings(src, HeadingLevel::H3)
}

/// Whether `offset` sits at the beginning of a line — the column-zero rule.
fn at_line_start(desc: &str, offset: usize) -> bool {
    offset == 0 || desc.as_bytes()[offset - 1] == b'\n'
}

/// The text of an ATX heading, or `None` for a setext heading
/// (`Anchor\n----`), which carries no `#` marker to strip.
fn atx_text(heading_src: &str, level: HeadingLevel) -> Option<String> {
    let marker = "#".repeat(level as usize);
    let rest = heading_src.strip_prefix(&marker)?;
    // CommonMark requires whitespace (or end of line) after the marker.
    if !rest.is_empty() && !rest.starts_with([' ', '\t', '\n', '\r']) {
        return None;
    }
    let text = rest.trim_end_matches(['\n', '\r']).trim();
    // An optional closing sequence: `## Spec ##` names the same section as
    // `## Spec`. It only closes when preceded by whitespace.
    let closed = text.trim_end_matches('#');
    let text = if closed.len() != text.len() && (closed.is_empty() || closed.ends_with([' ', '\t']))
    {
        closed.trim_end()
    } else {
        text
    };
    Some(text.to_string())
}

/// Locates a top-level H2 section by its exact anchor text (the part after
/// "## "). Returns the `[start, end)` byte offsets of the section's *content* —
/// everything after the heading line up to (but not including) the next H2
/// heading or end of string — plus whether it was found at all.
///
/// Matching rules:
///   - Anchor match is case-sensitive and exact (no leading/trailing spaces).
///   - The heading must be a real markdown heading — see the module docs.
///   - Content includes the leading newline after the heading and the trailing
///     newlines up to the next `## ` heading.
pub fn find_section(desc: &str, anchor: &str) -> (usize, usize, bool) {
    if anchor.is_empty() {
        return (0, 0, false);
    }
    let headings = headings(desc);
    for (i, h) in headings.iter().enumerate() {
        if h.text == anchor {
            let start = h.range.end;
            let end = headings
                .get(i + 1)
                .map_or(desc.len(), |next| next.range.start);
            return (start, end, true);
        }
    }
    (0, 0, false)
}

/// Replace the body of `anchor`'s section with `body`, leaving every other
/// section byte-identical. When the section is absent, the whole
/// `## <anchor>\n\n<body>\n` block is appended.
///
/// `body` is written verbatim between the heading and the next section, with
/// exactly one blank line on each side.
pub fn replace_section(desc: &str, anchor: &str, body: &str) -> String {
    let body = body.trim_end();
    let (start, end, found) = find_section(desc, anchor);
    if !found {
        let base = desc.trim_end();
        if base.is_empty() {
            return format!("## {anchor}\n\n{body}\n");
        }
        return format!("{base}\n\n## {anchor}\n\n{body}\n");
    }
    let mut out = String::with_capacity(desc.len() + body.len());
    out.push_str(&desc[..start]);
    out.push('\n');
    out.push_str(body);
    out.push_str("\n\n");
    out.push_str(&desc[end..]);
    // The tail may already have started with blank lines; collapse the seam so
    // repeated replacement does not accumulate them.
    collapse_blank_runs(&out)
}

/// Append `text` to the end of `anchor`'s section body as its own block,
/// separated by one blank line, leaving everything outside the section
/// byte-identical. When the section is absent it is created at the end —
/// callers wanting stricter create semantics check [`find_section`] first.
pub fn append_section(desc: &str, anchor: &str, text: &str) -> String {
    let text = text.trim_end();
    let (start, end, found) = find_section(desc, anchor);
    if !found {
        return replace_section(desc, anchor, text);
    }
    let existing = desc[start..end].trim_matches('\n');
    let body = if existing.is_empty() {
        text.to_string()
    } else {
        format!("{existing}\n\n{text}")
    };
    replace_section(desc, anchor, &body)
}

/// Prepare a caller-supplied section payload for writing under `anchor`.
///
/// Agents naturally include the heading in the file they pass (`## Plan\n...`);
/// written verbatim into the section body, that embedded H2 *terminates* the
/// section on the next parse and the content silently reads as empty. So: a
/// leading H2 matching the target anchor (case-insensitive) is stripped, and
/// any other H2 anywhere in the payload is an error naming the line — body
/// text cannot contain section boundaries.
///
/// Only H2s that would actually become boundaries count. A `## Plan` inside a
/// fenced code block is a payload an agent legitimately wants to store (a spec
/// quoting the plan format), and rejecting it was the same over-recognition
/// bug wearing a different hat.
pub fn sanitize_section_body(anchor: &str, body: &str) -> Result<String, String> {
    let headings = headings(body);
    let Some(first) = headings.first() else {
        return Ok(body.trim_start_matches('\n').trim_end().to_string());
    };

    // Is the first heading a restatement of the target anchor, before any
    // other content? Then drop it and keep what follows.
    let leading = body[..first.range.start].trim().is_empty();
    if leading && first.text.eq_ignore_ascii_case(anchor) {
        let rest = &body[first.range.end..];
        return match headings.get(1) {
            Some(next) => Err(embedded_h2(&body[next.range.clone()])),
            None => Ok(rest.trim_start_matches('\n').trim_end().to_string()),
        };
    }
    Err(embedded_h2(&body[first.range.clone()]))
}

fn embedded_h2(heading_src: &str) -> String {
    let trimmed = heading_src.trim_end();
    format!(
        "section payload contains an H2 heading {trimmed:?} — a section holds body \
         text only; an embedded H2 would terminate it and the content after would \
         silently leave the section"
    )
}

/// Every H2 anchor in the description, in order. What "sections: ..." error
/// messages list so a caller can see what actually exists.
pub fn h2_anchors(desc: &str) -> Vec<String> {
    headings(desc).into_iter().map(|h| h.text).collect()
}

/// One list item: how deeply nested it is (0 = top level) and its byte range.
pub struct ListItem {
    pub depth: usize,
    pub range: Range<usize>,
}

/// Every list item in `body`, in document order, with its nesting depth.
///
/// The ranges are what a markdown parser considers one item, which is the
/// whole point: an indented `- ` under an entry is a *nested list item* and a
/// non-indented prose line following one is a *lazy continuation*. Both belong
/// to the item that opened, and both used to be scanned as separate top-level
/// lines — the first producing spurious "does not parse" warnings, the second
/// silently dropping the text.
pub fn list_items(body: &str) -> Vec<ListItem> {
    let mut out = Vec::new();
    // Nesting depth in *lists*, not in blocks: a list inside a blockquote
    // still has top-level items as far as its own structure goes, and the
    // callers here care about "is this a child bullet of another bullet".
    let mut list_depth = 0usize;
    for (ev, range) in Parser::new_ext(body, options()).into_offset_iter() {
        match ev {
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(pulldown_cmark::TagEnd::List(_)) => list_depth -= 1,
            Event::Start(Tag::Item) => out.push(ListItem {
                depth: list_depth.saturating_sub(1),
                range,
            }),
            _ => {}
        }
    }
    out
}

/// The top-level list items of `body`, as byte ranges into it.
pub fn top_level_list_items(body: &str) -> Vec<Range<usize>> {
    list_items(body)
        .into_iter()
        .filter(|i| i.depth == 0)
        .map(|i| i.range)
        .collect()
}

/// Split one list item's source into its marker and its body, with
/// continuation lines dedented to the item's content column.
///
/// `- 2026-01-01T00:00Z — did a thing\n  - detail\n` yields
/// `"2026-01-01T00:00Z — did a thing\n- detail"`: the entry's own text, with
/// the structure beneath it intact and re-indentable.
pub fn list_item_body(item_src: &str) -> String {
    let mut lines = item_src.lines();
    let Some(first) = lines.next() else {
        return String::new();
    };
    // The marker is leading whitespace, a bullet, then whitespace. Whatever
    // follows starts the content column.
    let after_indent = first.trim_start();
    let indent = first.len() - after_indent.len();
    let after_bullet = match after_indent.strip_prefix(['-', '*', '+']) {
        Some(rest) => rest,
        // An ordered item (`1.`): take digits, then the delimiter.
        None => {
            let digits = after_indent
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_indent.len());
            after_indent[digits..]
                .strip_prefix(['.', ')'])
                .unwrap_or(&after_indent[digits..])
        }
    };
    let content = after_bullet.trim_start();
    let content_col = indent + (after_indent.len() - content.len());

    let mut out = String::with_capacity(item_src.len());
    out.push_str(content);
    for line in lines {
        out.push('\n');
        // Strip exactly the item's content indent; anything deeper is real
        // nesting and survives.
        let stripped = strip_indent(line, content_col);
        out.push_str(stripped);
    }
    out.trim_end().to_string()
}

/// Remove up to `n` leading spaces (tabs count as one) from `line`.
fn strip_indent(line: &str, n: usize) -> &str {
    for (cut, (i, ch)) in line.char_indices().enumerate() {
        if cut >= n || (ch != ' ' && ch != '\t') {
            return &line[i..];
        }
    }
    ""
}

/// `Task 3: rewire the parser` → `3`. The colon is required: it is what keeps
/// a search for Task 1 from matching "Task 10".
pub fn task_number(heading_text: &str) -> Option<i32> {
    heading_text
        .strip_prefix("Task ")?
        .split_once(':')
        .and_then(|(n, _)| n.trim().parse::<i32>().ok())
}

/// Give a headingless `## Plan` its implicit `### Task 1:` heading.
///
/// A flat plan — checkbox steps with no `### Task N:` above them — used to be
/// accepted at write time and reported by `lint` afterwards, with `tick`
/// refusing to address any of it ("no `### Task N:` headings in ## Plan"). The
/// contract is not relaxed to admit such plans; they are made canonical on the
/// way in, so what is stored is always what `tick` can drive.
///
/// Only a plan that *has steps to adopt* is rewritten. A plan holding just
/// prose or a placeholder gains nothing from an empty task heading — it would
/// merely trade one lint finding ("steps outside any task") for another
/// ("Task 1 has no steps").
pub fn canonicalize_plan(desc: &str, task_title: &str) -> String {
    let (start, end, ok) = find_section(desc, "Plan");
    if !ok {
        return desc.to_string();
    }
    let plan = &desc[start..end];
    if h3_headings(plan)
        .iter()
        .any(|h| task_number(&h.text).is_some())
    {
        return desc.to_string();
    }
    // Insert above the first step rather than at the top of the section, so
    // any prose introducing the plan stays plan-level commentary.
    let Some(first_step) = top_level_list_items(plan)
        .into_iter()
        .find(|r| is_step_line(&plan[r.clone()]))
    else {
        return desc.to_string();
    };

    let at = start + first_step.start;
    let prefix = &desc[..at];
    let sep = if prefix.is_empty() || prefix.ends_with("\n\n") {
        ""
    } else if prefix.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!(
        "{prefix}{sep}### Task 1: {}\n\n{}",
        heading_title(task_title),
        &desc[at..]
    )
}

/// Whether a list item's first line is a top-level GFM checkbox step.
fn is_step_line(item_src: &str) -> bool {
    let line = item_src.split_once('\n').map_or(item_src, |(l, _)| l);
    line.starts_with("- [ ] ") || line.starts_with("- [x] ")
}

/// A heading is one line, so the task inherits the issue's title only as far
/// as its first line. An issue with no usable title still needs *some* task
/// name for the contract to hold.
fn heading_title(title: &str) -> &str {
    let first = title.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        "Implementation"
    } else {
        first
    }
}

/// Squeeze runs of 3+ newlines down to 2. Markdown treats them the same, and
/// without this a section replaced N times grows N blank lines at its seam.
fn collapse_blank_runs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0usize;
    for ch in s.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines > 2 {
                continue;
            }
        } else {
            newlines = 0;
        }
        out.push(ch);
    }
    out
}

/// One entry of a `## Files` section: a ticket's prediction that it will add,
/// modify, or delete `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredictedChange {
    /// `A` add, `M` modify, `D` delete — git's own vocabulary.
    pub status: char,
    pub path: String,
}

/// A list item in a `## Files` section: either a parsed prediction or the raw
/// text of one that does not parse.
///
/// Readers skip `Invalid`; `lint` reports it. They are one enum because a
/// dropped entry and a reported entry must be the same set: an entry silently
/// ignored by the reader but accepted by the linter is a collision nobody
/// sees, which is the failure this section exists to prevent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileLine {
    Change(PredictedChange),
    Invalid(String),
}

/// Parse the `## Files` section. Prose lines are ignored, so the section can
/// carry a sentence of context; only list items are entries, and every list
/// item must parse. An absent section yields an empty vector.
pub fn file_lines(desc: &str) -> Vec<FileLine> {
    let (start, end, found) = find_section(desc, "Files");
    if !found {
        return Vec::new();
    }
    let body = &desc[start..end];
    let mut out = Vec::new();
    // The list parser, not a line scan: it is fence-aware and nesting-aware,
    // so an entry-shaped line quoted inside a code fence is content rather
    // than a prediction, and an item written with any valid bullet still
    // reaches the reader instead of vanishing before it can be reported.
    for range in top_level_list_items(body) {
        let item = list_item_body(&body[range]);
        let item = item.trim();
        if item.is_empty() {
            out.push(FileLine::Invalid(String::new()));
            continue;
        }
        let (status, path) = match item.split_once(char::is_whitespace) {
            Some((s, p)) => (s, p.trim()),
            None => (item, ""),
        };
        let mut chars = status.chars();
        match (chars.next(), chars.next(), path.is_empty()) {
            (Some(s @ ('A' | 'M' | 'D')), None, false) => {
                out.push(FileLine::Change(PredictedChange {
                    status: s,
                    path: path.to_string(),
                }));
            }
            _ => out.push(FileLine::Invalid(item.to_string())),
        }
    }
    out
}

/// The parseable entries of a `## Files` section, for readers that only care
/// about what the ticket claims it will touch.
pub fn predicted_changes(desc: &str) -> Vec<PredictedChange> {
    file_lines(desc)
        .into_iter()
        .filter_map(|l| match l {
            FileLine::Change(c) => Some(c),
            FileLine::Invalid(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_lines_parses_entries_and_ignores_prose() {
        let desc = "## Spec\n\nwords\n\n## Files\n\nPredicted, not a contract.\n\n\
                    - M crates/core/src/lib.rs\n- A crates/core/tests/new.rs\n\
                    - D old/gone.rs\n\n## Notes\n\n- M not/in/files.rs\n";
        assert_eq!(
            predicted_changes(desc),
            vec![
                PredictedChange {
                    status: 'M',
                    path: "crates/core/src/lib.rs".into()
                },
                PredictedChange {
                    status: 'A',
                    path: "crates/core/tests/new.rs".into()
                },
                PredictedChange {
                    status: 'D',
                    path: "old/gone.rs".into()
                },
            ]
        );
    }

    #[test]
    fn file_lines_flag_unparseable_items_rather_than_dropping_them() {
        let desc = "## Files\n\n- X bad/status.rs\n- M \n- justapath.rs\n- m lower/case.rs\n";
        let lines = file_lines(desc);
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert!(lines.iter().all(|l| matches!(l, FileLine::Invalid(_))));
        // A path containing spaces is kept verbatim: paths may contain them.
        assert_eq!(
            predicted_changes("## Files\n\n- M a path/with spaces.rs\n"),
            vec![PredictedChange {
                status: 'M',
                path: "a path/with spaces.rs".into()
            }]
        );
    }

    #[test]
    fn no_files_section_is_not_an_error() {
        assert!(file_lines("## Spec\n\nnothing here\n").is_empty());
    }

    #[test]
    fn an_entry_quoted_in_a_fence_is_content_not_a_prediction() {
        // Same bug class this module documents fixing for `##` headings: a
        // fenced example of the format must not become a real entry.
        let desc = "## Files\n\nThe format is:\n\n```markdown\n- M example/only.rs\n```\n\n\
                    - M real/entry.rs\n";
        assert_eq!(
            predicted_changes(desc),
            vec![PredictedChange {
                status: 'M',
                path: "real/entry.rs".into()
            }]
        );
    }

    #[test]
    fn every_valid_bullet_reaches_the_reader() {
        // A dropped entry is invisible to lint AND to collision detection, so
        // an entry written with a tab or a `+` must still be seen, and seen as
        // whatever it is: a change if it parses, an Invalid if it does not.
        let lines = file_lines("## Files\n\n-\tM tab/bullet.rs\n+ M plus/bullet.rs\n- X bad.rs\n");
        assert_eq!(
            lines,
            vec![
                FileLine::Change(PredictedChange {
                    status: 'M',
                    path: "tab/bullet.rs".into()
                }),
                FileLine::Change(PredictedChange {
                    status: 'M',
                    path: "plus/bullet.rs".into()
                }),
                FileLine::Invalid("X bad.rs".into()),
            ]
        );
    }

    #[test]
    fn find_section_returns_the_content_range() {
        let d = "## Spec\n\nhello\n\n## Plan\n\nworld\n";
        let (s, e, ok) = find_section(d, "Spec");
        assert!(ok);
        assert_eq!(&d[s..e], "\nhello\n\n");
    }

    #[test]
    fn find_section_runs_to_end_when_it_is_last() {
        let d = "## Spec\n\nhello\n";
        let (s, e, ok) = find_section(d, "Spec");
        assert!(ok);
        assert_eq!(&d[s..e], "\nhello\n");
    }

    #[test]
    fn missing_section_is_reported_not_guessed() {
        let (_, _, ok) = find_section("## Plan\n\nx\n", "Spec");
        assert!(!ok);
        let (_, _, ok) = find_section("## Spec\n", "");
        assert!(!ok, "an empty anchor matches nothing");
    }

    #[test]
    fn anchor_match_is_exact() {
        // "Spec" must not match "Specification" — a prefix match here would
        // splice the wrong section.
        let (_, _, ok) = find_section("## Specification\n\nx\n", "Spec");
        assert!(!ok);
    }

    #[test]
    fn replace_section_leaves_neighbours_byte_identical() {
        let d = "## Spec\n\nold spec\n\n## Plan\n\n### Task 1: x\n\n- [x] **Step 1: done**\n";
        let out = replace_section(d, "Spec", "new spec");
        assert!(out.contains("new spec"));
        assert!(!out.contains("old spec"));
        // The plan, including its tick state, survives untouched.
        assert!(out.contains("## Plan\n\n### Task 1: x\n\n- [x] **Step 1: done**\n"));
    }

    #[test]
    fn replace_section_appends_when_absent() {
        let out = replace_section("## Plan\n\nkeep me\n", "Spec", "fresh");
        assert!(out.contains("## Plan\n\nkeep me"));
        assert!(out.contains("## Spec\n\nfresh\n"));
    }

    #[test]
    fn replace_section_on_empty_description_yields_just_the_section() {
        assert_eq!(replace_section("", "Spec", "body"), "## Spec\n\nbody\n");
    }

    #[test]
    fn append_section_adds_a_block_and_preserves_neighbours() {
        let d = "## Spec\n\ns\n\n## Decisions so far\n\n- first\n\n## Plan\n\np\n";
        let out = append_section(d, "Decisions so far", "- second");
        assert!(out.contains("- first\n\n- second\n"), "got {out:?}");
        assert!(out.contains("## Spec\n\ns\n"));
        assert!(out.contains("## Plan\n\np\n"));
    }

    #[test]
    fn append_section_into_empty_section_has_no_leading_blank() {
        let d = "## Notes\n";
        let out = append_section(d, "Notes", "first");
        // Same trailing-seam convention as replace_section: at most one
        // blank line, never a run.
        assert_eq!(out.trim_end(), "## Notes\n\nfirst");
        assert!(!out.contains("\n\n\n"));
    }

    #[test]
    fn append_section_creates_when_absent() {
        let out = append_section("## Spec\n\ns\n", "Rollout", "step one");
        assert!(out.contains("## Rollout\n\nstep one\n"));
        assert!(out.contains("## Spec\n\ns\n"));
    }

    #[test]
    fn repeated_append_does_not_accumulate_blank_lines() {
        let mut d = "## Notes\n\nseed\n\n## Plan\n\np\n".to_string();
        for i in 0..5 {
            d = append_section(&d, "Notes", &format!("entry {i}"));
        }
        assert!(!d.contains("\n\n\n"), "blank-line drift: {d:?}");
        assert!(d.contains("entry 0\n\nentry 1"));
        assert!(d.contains("## Plan\n\np\n"));
    }

    #[test]
    fn sanitize_strips_the_restated_heading() {
        let out =
            sanitize_section_body("Plan", "## Plan\n\n### Task 1: x\n\n- [ ] step\n").unwrap();
        assert_eq!(out, "### Task 1: x\n\n- [ ] step");
        // Case-insensitive: agents write "## plan" too.
        assert!(sanitize_section_body("Plan", "## plan\nbody").unwrap() == "body");
    }

    #[test]
    fn sanitize_rejects_foreign_h2s() {
        let err = sanitize_section_body("Spec", "the spec\n\n## Plan\n\nsneaky").unwrap_err();
        assert!(err.contains("## Plan"), "{err}");
        // A matching heading later (after content) is also a boundary, not a restatement.
        assert!(sanitize_section_body("Spec", "text\n## Spec\nmore").is_err());
    }

    #[test]
    fn sanitize_passes_clean_bodies_through() {
        assert_eq!(
            sanitize_section_body("Notes", "### lesson\n\nbody").unwrap(),
            "### lesson\n\nbody"
        );
    }

    #[test]
    fn h2_anchors_lists_in_order() {
        let d = "## Spec\n\nx\n\n## Decisions so far\n\ny\n\n## Plan\n";
        assert_eq!(h2_anchors(d), vec!["Spec", "Decisions so far", "Plan"]);
    }

    #[test]
    fn repeated_replacement_does_not_accumulate_blank_lines() {
        let mut d = "## Spec\n\none\n\n## Plan\n\np\n".to_string();
        for i in 0..5 {
            d = replace_section(&d, "Spec", &format!("body {i}"));
        }
        assert!(!d.contains("\n\n\n"), "blank-line drift: {d:?}");
        assert!(d.contains("## Plan\n\np\n"));
    }

    // ---- block structure: things that look like headings but are not ----

    #[test]
    fn a_heading_inside_a_code_fence_is_not_a_boundary() {
        // The bug this module was rewritten for. The fenced `## Plan` used to
        // truncate Spec and shadow the real plan; tick then failed with
        // "no Task 1 in ## Plan" while lint reported the issue clean.
        let d = "## Spec\n\nFormat:\n\n```markdown\n## Plan\n\nnot real\n```\n\nstill spec.\n\n\
                 ## Plan\n\n### Task 1: t\n\n- [ ] **Step 1: x**\n";
        let (s, e, ok) = find_section(d, "Spec");
        assert!(ok);
        let spec = &d[s..e];
        assert!(
            spec.contains("```markdown"),
            "fence stays in Spec: {spec:?}"
        );
        assert!(spec.contains("still spec."), "Spec not truncated: {spec:?}");

        let (s, e, ok) = find_section(d, "Plan");
        assert!(ok);
        assert!(
            d[s..e].contains("### Task 1: t"),
            "the real plan is the one found: {:?}",
            &d[s..e]
        );
        assert_eq!(h2_anchors(d), vec!["Spec", "Plan"]);
    }

    #[test]
    fn a_tilde_fence_hides_headings_too() {
        let d = "## Spec\n\n~~~\n## Plan\n~~~\n\ntail\n";
        assert_eq!(h2_anchors(d), vec!["Spec"]);
        let (s, e, _) = find_section(d, "Spec");
        assert!(d[s..e].contains("tail"));
    }

    #[test]
    fn a_heading_inside_an_indented_code_block_is_not_a_boundary() {
        let d = "## Spec\n\nExample:\n\n    ## Plan\n\n    indented code\n\nstill spec.\n";
        assert_eq!(h2_anchors(d), vec!["Spec"]);
        let (s, e, ok) = find_section(d, "Spec");
        assert!(ok);
        assert!(d[s..e].contains("still spec."));
    }

    #[test]
    fn a_hash_inside_link_text_is_not_a_boundary() {
        let d = "## Spec\n\nSee [## Plan](http://example.test/x) and [#42](http://y).\n";
        assert_eq!(h2_anchors(d), vec!["Spec"]);
        let (s, e, _) = find_section(d, "Spec");
        assert!(d[s..e].contains("http://y"));
    }

    #[test]
    fn a_heading_nested_in_a_list_or_quote_is_not_a_boundary() {
        // Indented under a list item, or quoted — both belong to that block,
        // not to the document's section sequence.
        let d = "## Spec\n\n- item\n  ## Plan\n\n> ## Notes\n\ntail\n";
        assert_eq!(h2_anchors(d), vec!["Spec"]);
    }

    #[test]
    fn setext_and_indented_headings_stay_out_of_the_contract() {
        // Both are H2s in CommonMark. The writers emit neither, and promoting
        // them would split descriptions that parse fine today — so the
        // contract stays "ATX at column zero". Pinned deliberately.
        assert_eq!(h2_anchors("## Spec\n\nPlan\n----\n\nbody\n"), vec!["Spec"]);
        assert_eq!(h2_anchors("## Spec\n\n   ## Plan\n\nbody\n"), vec!["Spec"]);
    }

    #[test]
    fn a_closing_sequence_names_the_same_section() {
        let d = "## Spec ##\n\nbody\n\n## Plan\n";
        assert_eq!(h2_anchors(d), vec!["Spec", "Plan"]);
        let (s, e, ok) = find_section(d, "Spec");
        assert!(ok);
        assert_eq!(&d[s..e], "\nbody\n\n");
    }

    #[test]
    fn sanitize_allows_a_fenced_heading_in_the_payload() {
        // A spec that quotes the plan format is a legitimate payload; the old
        // line scan rejected it as an embedded boundary.
        let body = "The plan format is:\n\n```markdown\n## Plan\n\n### Task 1: x\n```\n";
        let out = sanitize_section_body("Spec", body).unwrap();
        assert!(out.contains("## Plan"), "fenced heading survives: {out:?}");
    }

    #[test]
    fn replace_section_does_not_eat_a_following_fenced_heading() {
        let d = "## Spec\n\nold\n\n## Notes\n\n```\n## Plan\n```\n";
        let out = replace_section(d, "Spec", "new");
        assert!(out.contains("new"));
        assert!(out.contains("```\n## Plan\n```"), "got {out:?}");
        assert_eq!(h2_anchors(&out), vec!["Spec", "Notes"]);
    }

    // ---- list items ----

    #[test]
    fn a_sublist_belongs_to_the_item_that_opened_it() {
        let body = "- 2026-08-08T10:00Z — did a thing\n  - detail one\n  - detail two\n\
                    - 2026-08-08T11:00Z — next\n";
        let items = top_level_list_items(body);
        assert_eq!(items.len(), 2, "nested bullets are not top-level items");
        assert!(body[items[0].clone()].contains("detail two"));
        assert_eq!(
            list_item_body(&body[items[0].clone()]),
            "2026-08-08T10:00Z — did a thing\n- detail one\n- detail two"
        );
    }

    #[test]
    fn a_lazy_continuation_folds_into_its_item() {
        let body = "- 2026-08-08T11:00Z — wrapped entry that continues\n  onto a prose line\n";
        let items = top_level_list_items(body);
        assert_eq!(items.len(), 1);
        assert_eq!(
            list_item_body(&body[items[0].clone()]),
            "2026-08-08T11:00Z — wrapped entry that continues\nonto a prose line"
        );
    }

    // ---- canonical plans ----

    #[test]
    fn a_flat_plan_gains_task_one() {
        let d = "## Spec\n\ns\n\n## Plan\n\n- [ ] first\n- [ ] second\n";
        let out = canonicalize_plan(d, "Rewire the parser");
        assert_eq!(
            out,
            "## Spec\n\ns\n\n## Plan\n\n### Task 1: Rewire the parser\n\n- [ ] first\n- [ ] second\n"
        );
        // …and what comes out is what `tick` can drive.
        let (s, e, _) = find_section(&out, "Plan");
        assert_eq!(
            h3_headings(&out[s..e])
                .iter()
                .filter_map(|h| task_number(&h.text))
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn a_plan_that_already_has_task_headings_passes_through_untouched() {
        let d = "## Plan\n\n### Task 1: a\n\n- [ ] x\n\n### Task 2: b\n\n- [ ] y\n";
        assert_eq!(canonicalize_plan(d, "T"), d);
        // Even numbered oddly — renumbering is lint's business, not this
        // function's, and rewriting here would hide the finding.
        let odd = "## Plan\n\n### Task 3: a\n\n- [ ] x\n";
        assert_eq!(canonicalize_plan(odd, "T"), odd);
    }

    #[test]
    fn an_issue_with_no_plan_is_untouched() {
        let d = "## Spec\n\njust a spec\n";
        assert_eq!(canonicalize_plan(d, "T"), d);
        assert_eq!(canonicalize_plan("", "T"), "");
        assert_eq!(canonicalize_plan("free text\n", "T"), "free text\n");
    }

    #[test]
    fn a_plan_with_no_steps_is_left_alone() {
        // The Linear bridge seeds an empty `## Plan` placeholder. Giving it a
        // task heading would only trade "steps outside any task" for
        // "Task 1 has no steps".
        let d = "## Plan\n\n_No plan yet. Add tasks as `### Task N: title`._\n";
        assert_eq!(canonicalize_plan(d, "T"), d);
        assert_eq!(canonicalize_plan("## Plan\n", "T"), "## Plan\n");
    }

    #[test]
    fn plan_level_prose_stays_above_the_inserted_task() {
        let d = "## Plan\n\nApproach: smallest change first.\n\n- [ ] first\n";
        let out = canonicalize_plan(d, "T");
        assert_eq!(
            out,
            "## Plan\n\nApproach: smallest change first.\n\n### Task 1: T\n\n- [ ] first\n"
        );
    }

    #[test]
    fn a_prose_line_directly_above_a_step_still_gets_a_blank_line() {
        let d = "## Plan\n\nApproach:\n- [ ] first\n";
        let out = canonicalize_plan(d, "T");
        assert!(
            out.contains("Approach:\n\n### Task 1: T\n\n- [ ] first"),
            "got {out:?}"
        );
    }

    #[test]
    fn a_fenced_checkbox_does_not_look_like_a_plan_to_adopt() {
        let d = "## Plan\n\n```\n- [ ] not a step\n```\n";
        assert_eq!(canonicalize_plan(d, "T"), d);
    }

    #[test]
    fn a_titleless_issue_still_gets_a_contract_shaped_heading() {
        let out = canonicalize_plan("## Plan\n\n- [ ] x\n", "");
        assert!(out.contains("### Task 1: Implementation"), "got {out:?}");
        // A multi-line title cannot become a multi-line heading.
        let out = canonicalize_plan("## Plan\n\n- [ ] x\n", "line one\nline two");
        assert!(out.contains("### Task 1: line one\n"), "got {out:?}");
    }

    #[test]
    fn canonicalizing_is_idempotent() {
        let d = "## Plan\n\n- [ ] first\n";
        let once = canonicalize_plan(d, "T");
        assert_eq!(canonicalize_plan(&once, "T"), once);
    }

    #[test]
    fn list_item_body_handles_the_marker_shapes_markdown_allows() {
        assert_eq!(list_item_body("* starred\n"), "starred");
        assert_eq!(list_item_body("+ plussed\n"), "plussed");
        assert_eq!(list_item_body("1. numbered\n"), "numbered");
        assert_eq!(list_item_body("- \n"), "");
    }
}
