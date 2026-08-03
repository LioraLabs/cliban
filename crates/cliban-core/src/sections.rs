//! Locating H2 sections in the issue/milestone description markdown.
//!
//! The description is a contract, not free text: `## Spec`, `## Plan`,
//! `## Activity Log`, and `## Notes` each have an owner, and every tool that
//! edits one must leave the others byte-identical. This module is the single
//! definition of where a section starts and ends.
//!
//! It lives in core rather than in the CLI's `descmd` because the Linear bridge
//! needs the same boundaries: a re-import replaces `## Spec` and must not
//! disturb the `## Plan` an agent has been ticking. Two implementations of
//! "where does this section end" would eventually disagree, and the symptom
//! would be a silently eaten plan.

/// Locates a top-level H2 section by its exact anchor text (the part after
/// "## "). Returns the `[start, end)` byte offsets of the section's *content* —
/// everything after the heading line up to (but not including) the next H2
/// heading or end of string — plus whether it was found at all.
///
/// Matching rules:
///   - Anchor match is case-sensitive and exact (no leading/trailing spaces).
///   - The heading must appear at the start of a line.
///   - Content includes the leading newline after the heading and the trailing
///     newlines up to the next `## ` heading.
pub fn find_section(desc: &str, anchor: &str) -> (usize, usize, bool) {
    if anchor.is_empty() {
        return (0, 0, false);
    }
    let needle = format!("## {anchor}");
    let mut offset = 0usize;
    let mut section_content_start: Option<usize> = None;
    for line in desc.split_inclusive('\n') {
        let line_len = line.len();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        match section_content_start {
            None => {
                if trimmed == needle {
                    section_content_start = Some(offset + line_len);
                }
            }
            Some(start) => {
                if trimmed.starts_with("## ") {
                    return (start, offset, true);
                }
            }
        }
        offset += line_len;
    }
    match section_content_start {
        None => (0, 0, false),
        Some(start) => (start, desc.len(), true),
    }
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
pub fn sanitize_section_body(anchor: &str, body: &str) -> Result<String, String> {
    let mut lines = body.lines();
    let mut kept: Vec<&str> = Vec::new();
    let mut first_content_seen = false;
    for line in &mut lines {
        let trimmed = line.trim_end();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            if !first_content_seen && heading.trim().eq_ignore_ascii_case(anchor) {
                // The payload restated its own heading; drop it.
                continue;
            }
            return Err(format!(
                "section payload contains an H2 heading {trimmed:?} — a section holds body \
                 text only; an embedded H2 would terminate it and the content after would \
                 silently leave the section"
            ));
        }
        if !trimmed.is_empty() {
            first_content_seen = true;
        }
        kept.push(line);
    }
    Ok(kept.join("\n").trim_start_matches('\n').to_string())
}

/// Every H2 anchor in the description, in order. What "sections: ..." error
/// messages list so a caller can see what actually exists.
pub fn h2_anchors(desc: &str) -> Vec<String> {
    desc.lines()
        .filter_map(|l| l.trim_end().strip_prefix("## "))
        .map(str::to_string)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let out = sanitize_section_body("Plan", "## Plan\n\n### Task 1: x\n\n- [ ] step\n").unwrap();
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
}
