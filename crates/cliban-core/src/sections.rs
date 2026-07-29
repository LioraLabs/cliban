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
    fn repeated_replacement_does_not_accumulate_blank_lines() {
        let mut d = "## Spec\n\none\n\n## Plan\n\np\n".to_string();
        for i in 0..5 {
            d = replace_section(&d, "Spec", &format!("body {i}"));
        }
        assert!(!d.contains("\n\n\n"), "blank-line drift: {d:?}");
        assert!(d.contains("## Plan\n\np\n"));
    }
}
