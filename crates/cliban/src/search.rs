//! Fuzzy search over issues, ported to match the Go oracle
//! (`internal/search`). The scorer is a verbatim port of
//! `github.com/sahilm/fuzzy` v0.1.2 (`fuzzy.go`) so that per-field scores —
//! and therefore the summed `score` and the result ORDER — match byte-for-byte.

use cliban_core::contexts::issues::{self, ListOpts};
use cliban_core::contexts::milestones;
use cliban_core::schema::Issue;
use cliban_core::Store;

use crate::errors::{CliError, CliResult};

// ---- sahilm/fuzzy port (v0.1.2 fuzzy.go) ----

const FIRST_CHAR_MATCH_BONUS: i64 = 10;
const MATCH_FOLLOWING_SEPARATOR_BONUS: i64 = 20;
const CAMEL_CASE_MATCH_BONUS: i64 = 20;
const ADJACENT_MATCH_BONUS: i64 = 5;
const UNMATCHED_LEADING_CHAR_PENALTY: i64 = -5;
const MAX_UNMATCHED_LEADING_CHAR_PENALTY: i64 = -15;

const SEPARATORS: &[char] = &['/', '-', '_', ' ', '.', '\\'];

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

/// `unicode.SimpleFold`-free ASCII-leaning equalFold, matching Go's
/// `strings.EqualFold` for the rune pairs this code compares. For ASCII it is
/// exact; for non-ASCII it falls back to Rust's `char::eq_ignore` semantics via
/// simple case folding, which matches Go for the common cases.
fn equal_fold(tr: char, sr: char) -> bool {
    if tr == sr {
        return true;
    }
    // Order so that `tr >= sr` like the Go code.
    let (tr, sr) = if tr < sr { (sr, tr) } else { (tr, sr) };
    if (tr as u32) < 0x80 {
        // ASCII, sr is upper case → tr must be lower case.
        if ('A'..='Z').contains(&sr) && tr == ((sr as u8) + b'a' - b'A') as char {
            return true;
        }
        return false;
    }
    // General (non-ASCII) case: compare simple-folded forms. Go uses
    // unicode.SimpleFold iteration; for the inputs cliban deals with this
    // lowercase-comparison is equivalent for matching purposes.
    tr.to_lowercase().eq(sr.to_lowercase())
}

fn adjacent_char_bonus(i: usize, last_match: usize, current_bonus: i64) -> i64 {
    if last_match == i {
        current_bonus * 2 + ADJACENT_MATCH_BONUS
    } else {
        0
    }
}

/// Port of sahilm/fuzzy's per-string match (the body of `FindFromNoSort`'s
/// loop, specialized to a single candidate string). Returns `(score,
/// matched_byte_offsets)` when every pattern rune is matched in order, else
/// `None`.
///
/// Indexes are byte offsets into `text` (matching the Go code, which records
/// `j`, a byte index, in `MatchedIndexes`).
pub fn fuzzy_find(pattern: &str, text: &str) -> Option<(i64, Vec<usize>)> {
    if pattern.is_empty() {
        return None;
    }
    let runes: Vec<char> = pattern.chars().collect();

    // Limit matching to the first NUL rune, if any.
    let clean: &str = match text.find('\0') {
        Some(i) => &text[..i],
        None => text,
    };

    // Pre-decode candidate runes with their byte offsets so we can do the
    // `next` look-ahead the Go code performs by byte-indexing.
    let chars: Vec<(usize, char)> = clean.char_indices().collect();
    let n = chars.len();

    let mut matched_indexes: Vec<usize> = Vec::with_capacity(runes.len());
    let mut total_score: i64 = 0;

    let mut score: i64;
    let mut pattern_index = 0usize;
    let mut best_score: i64 = -1;
    let mut matched_index: i64 = -1;
    let mut curr_adjacent_match_bonus: i64 = 0;
    let mut last: char = '\0';
    let mut have_last = false;
    let mut last_index: usize = 0;

    for idx in 0..n {
        let (j, candidate) = chars[idx];

        if equal_fold(candidate, runes[pattern_index]) {
            score = 0;
            if j == 0 {
                score += FIRST_CHAR_MATCH_BONUS;
            }
            if have_last && last.is_lowercase() && candidate.is_uppercase() {
                score += CAMEL_CASE_MATCH_BONUS;
            }
            if j != 0 && have_last && is_separator(last) {
                score += MATCH_FOLLOWING_SEPARATOR_BONUS;
            }
            if let Some(&last_match) = matched_indexes.last() {
                let bonus = adjacent_char_bonus(last_index, last_match, curr_adjacent_match_bonus);
                score += bonus;
                curr_adjacent_match_bonus += bonus;
            }
            if score > best_score {
                best_score = score;
                matched_index = j as i64;
            }
        }

        // Determine the next pattern rune and the next candidate rune (byte
        // value-wise look-ahead mirrors the Go code; for matching purposes the
        // decoded rune is what matters).
        let nextp: char = if pattern_index < runes.len() - 1 {
            runes[pattern_index + 1]
        } else {
            '\0'
        };
        let nextc: char = if idx + 1 < n { chars[idx + 1].1 } else { '\0' };

        if equal_fold(nextp, nextc) || nextc == '\0' {
            if matched_index > -1 {
                if matched_indexes.is_empty() {
                    let penalty = matched_index * UNMATCHED_LEADING_CHAR_PENALTY;
                    best_score += penalty.max(MAX_UNMATCHED_LEADING_CHAR_PENALTY);
                }
                total_score += best_score;
                matched_indexes.push(matched_index as usize);
                best_score = -1;
                matched_index = -1;
                pattern_index += 1;
                if pattern_index >= runes.len() {
                    // All pattern runes consumed. The Go loop continues to the
                    // end of the string but never matches again (pattern_index
                    // out of range); replicate by breaking after the final
                    // per-char penalty accounting below.
                    // We still must apply the trailing unmatched-char penalty,
                    // handled after the loop.
                }
            }
        }

        last_index = j;
        last = candidate;
        have_last = true;

        if pattern_index >= runes.len() {
            break;
        }
    }

    // Apply penalty for each unmatched character: len(matched) - len(clean
    // runes).
    let penalty = matched_indexes.len() as i64 - n as i64;
    total_score += penalty;

    if matched_indexes.len() == runes.len() {
        Some((total_score, matched_indexes))
    } else {
        None
    }
}

// ---- description stripping (internal/search/strip.go) ----

const MAX_DESC_BYTES: usize = 4096;

/// Port of `stripDescription`: collapse fenced code blocks to a single space,
/// drop heading markers, unwrap `[text](url)` to `text`, cap at 4096 bytes.
pub fn strip_description(s: &str) -> String {
    use std::sync::OnceLock;
    static RE_FENCE: OnceLock<regex::Regex> = OnceLock::new();
    static RE_HEADING: OnceLock<regex::Regex> = OnceLock::new();
    static RE_LINK: OnceLock<regex::Regex> = OnceLock::new();

    // (?s)```.*?``` — DOTALL, non-greedy fenced block.
    let re_fence = RE_FENCE.get_or_init(|| regex::Regex::new(r"(?s)```.*?```").unwrap());
    // (?m)^#+\s* — heading markers at line start.
    let re_heading = RE_HEADING.get_or_init(|| regex::Regex::new(r"(?m)^#+\s*").unwrap());
    // \[([^\]]+)\]\([^)]+\) — markdown link.
    let re_link = RE_LINK.get_or_init(|| regex::Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap());

    let s = re_fence.replace_all(s, " ");
    let s = re_heading.replace_all(&s, "");
    let s = re_link.replace_all(&s, "$1");
    let mut s = s.into_owned();
    if s.len() > MAX_DESC_BYTES {
        // Byte-truncate, then back off to a char boundary (Go slices raw bytes;
        // for ASCII these agree, and truncating to a boundary is the safe Rust
        // equivalent — fuzzy matching is unaffected by a dropped partial rune).
        let mut cut = MAX_DESC_BYTES;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s
}

// ---- weighted multi-field search (internal/search/search.go) ----

const WEIGHT_TITLE: i64 = 30;
const WEIGHT_KEY: i64 = 25;
const WEIGHT_LABEL: i64 = 20;
const WEIGHT_DESC: i64 = 10;

/// A single search hit: the matched issue plus its summed weighted score.
pub struct Match {
    pub issue: Issue,
    pub score: i64,
}

/// Options for [`search`], mirroring Go `search.Options`. Each filter field is
/// the resolved (parsed/normalized) value, an empty option meaning "no filter".
pub struct Options {
    pub query: String,
    pub project: Option<String>,
    pub label: Vec<String>,
    pub milestone: Option<String>,
    pub status: Option<String>,
    pub priority: Option<String>,
    pub parent: Option<String>,
    pub include_archived: bool,
    pub exclude_subs: bool,
    pub limit: i64,
}

/// Project-key prefix of an issue key (`CLI-12` → `CLI`).
fn project_prefix(key: &str) -> &str {
    match key.rfind('-') {
        Some(idx) => &key[..idx],
        None => key,
    }
}

/// Base ordering matching the Go store query (`ORDER BY p.key, i.status,
/// i.position`). Establishes the stable-sort input order so score/updated_at
/// tiebreaks resolve identically to the oracle.
fn base_order(issues: &mut [Issue]) {
    issues.sort_by(|a, b| {
        project_prefix(&a.key)
            .cmp(project_prefix(&b.key))
            .then_with(|| a.status.cmp(&b.status))
            .then_with(|| {
                a.position
                    .partial_cmp(&b.position)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
}

/// Run a fuzzy search according to `opts`. Fetches the candidate set with the
/// same project/status/priority/milestone/label/parent/no-subs/archived
/// filters as `issue ls`, scores each candidate's title/key/labels/description
/// against the (trimmed) query, keeps any issue with ≥1 matched field, and
/// sorts by score desc with updated_at desc as the tiebreak. An empty query
/// returns every candidate sorted by updated_at desc with score 0.
pub async fn search(store: &Store, opts: Options) -> CliResult<Vec<Match>> {
    // ---- candidate fetch + filtering (mirrors `issue ls`) ----
    let project = opts.project.clone().filter(|p| !p.is_empty());
    let status = opts.status.clone().filter(|s| !s.is_empty());
    let milestone = opts.milestone.clone().filter(|m| !m.is_empty());
    let priority = opts.priority.clone().filter(|p| !p.is_empty());
    let parent_key = opts.parent.clone().filter(|p| !p.is_empty());

    let list_project = project.clone();
    let list_status = status.clone();
    let list_milestone = milestone.clone();
    let include_archived = opts.include_archived;
    let mut issues = store
        .call(move |conn| {
            let mut out = issues::list(
                conn,
                ListOpts {
                    project: list_project.as_deref(),
                    status: list_status.as_deref(),
                    milestone: list_milestone.as_deref(),
                    archived: false,
                },
            )?;
            if include_archived {
                let archived = issues::list(
                    conn,
                    ListOpts {
                        project: list_project.as_deref(),
                        status: list_status.as_deref(),
                        milestone: list_milestone.as_deref(),
                        archived: true,
                    },
                )?;
                out.extend(archived);
            }
            Ok(out)
        })
        .await?;

    if let Some(pr) = &priority {
        issues.retain(|i| &i.priority == pr);
    }
    if let Some(pk) = &parent_key {
        // Go's search.Search parses the parent key up front and errors on a
        // malformed key.
        let parsed = crate::cmd::issue::parse_issue_key_pub(pk)
            .map_err(|e| CliError::other(format!("parent key {pk:?}: {}", e.message())))?;
        let lookup = parsed.clone();
        let parent_id = store
            .call(move |conn| issues::get_by_key(conn, &lookup).map(|o| o.map(|i| i.id)))
            .await?;
        match parent_id {
            Some(pid) => issues.retain(|i| i.parent_id == Some(pid)),
            None => issues.clear(),
        }
    }
    if opts.exclude_subs {
        issues.retain(|i| i.parent_id.is_none());
    }
    if !opts.label.is_empty() {
        let want = opts.label.clone();
        let mut kept = Vec::with_capacity(issues.len());
        for i in issues.into_iter() {
            let id = i.id;
            let names = store.call(move |conn| issues::label_names(conn, id)).await?;
            if want.iter().all(|w| names.iter().any(|n| n == w)) {
                kept.push(i);
            }
        }
        issues = kept;
    }

    // Establish the Go store's input ordering before the stable result sort.
    base_order(&mut issues);

    let q = opts.query.trim();

    let mut matches: Vec<Match>;
    if q.is_empty() {
        matches = issues.into_iter().map(|issue| Match { issue, score: 0 }).collect();
        // stable sort by updated_at desc
        matches.sort_by(|a, b| b.issue.updated_at.cmp(&a.issue.updated_at));
    } else {
        // Resolve labels per issue (one lookup each — matches the bulk Go path
        // result, just N round-trips on our writer thread).
        matches = Vec::with_capacity(issues.len());
        for issue in issues.into_iter() {
            let id = issue.id;
            let labels = store.call(move |conn| issues::label_names(conn, id)).await?;
            let label_str = labels.join(" ");
            let desc = strip_description(&issue.description);

            let fields: [(&str, i64); 4] = [
                (issue.title.as_str(), WEIGHT_TITLE),
                (issue.key.as_str(), WEIGHT_KEY),
                (label_str.as_str(), WEIGHT_LABEL),
                (desc.as_str(), WEIGHT_DESC),
            ];

            let mut total = 0i64;
            let mut matched = false;
            for (text, weight) in fields {
                if text.is_empty() {
                    continue;
                }
                if let Some((s, _idx)) = fuzzy_find(q, text) {
                    total += s * weight;
                    matched = true;
                }
            }
            if matched {
                matches.push(Match { issue, score: total });
            }
        }
        // stable sort: score desc, updated_at desc tiebreak
        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.issue.updated_at.cmp(&a.issue.updated_at))
        });
    }

    if opts.limit > 0 && matches.len() > opts.limit as usize {
        matches.truncate(opts.limit as usize);
    }
    Ok(matches)
}

/// Resolve milestone name + parent key for an issue (empty when unset). Local
/// copy so `search` does not depend on the `cmd::issue` module's private
/// helper.
pub async fn resolve_refs(store: &Store, issue: &Issue) -> CliResult<(String, String)> {
    let milestone_id = issue.milestone_id;
    let parent_id = issue.parent_id;
    let pair = store
        .call(move |conn| {
            let milestone = match milestone_id {
                Some(mid) => milestones::get_by_id(conn, mid)?.map(|m| m.name),
                None => None,
            };
            let parent = match parent_id {
                Some(pid) => issues::get_by_id(conn, pid)?.map(|i| i.key),
                None => None,
            };
            Ok((milestone.unwrap_or_default(), parent.unwrap_or_default()))
        })
        .await?;
    Ok(pair)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_no_match_returns_none() {
        assert!(fuzzy_find("xyz", "abc").is_none());
        assert!(fuzzy_find("abcd", "abc").is_none()); // pattern longer
        assert!(fuzzy_find("", "abc").is_none());
    }

    #[test]
    fn fuzzy_exhaustive_picks_best_k() {
        // From the sahilm/fuzzy doc: "tk" against "The Black Knight" should
        // match the second k (the K after a separator) for a higher score.
        let (score, idx) = fuzzy_find("tk", "The Black Knight").unwrap();
        // 't' at 0 (first-char bonus 10, leading penalty 0); 'K' at index 10
        // follows a separator (bonus 20). Verify the K chosen is the Knight K.
        assert_eq!(idx[0], 0);
        assert_eq!(idx[1], 10);
        assert!(score > 0, "score {score}");
    }

    #[test]
    fn fuzzy_first_char_and_full_word() {
        // "search" fully matching "search" → strong positive score.
        let (full, _) = fuzzy_find("search", "search").unwrap();
        let (partial, _) = fuzzy_find("srch", "search").unwrap();
        assert!(full > partial, "full {full} partial {partial}");
    }

    #[test]
    fn strip_collapses_fences_headings_links() {
        let s = strip_description("## Spec\nhello [text](http://x)\n```\ncode\n```\nbye");
        assert!(!s.contains("##"));
        assert!(!s.contains("```"));
        assert!(!s.contains("http"));
        assert!(s.contains("text"));
        assert!(s.contains("hello") && s.contains("bye"));
    }

    #[test]
    fn strip_caps_at_4096_bytes() {
        let big = "a".repeat(5000);
        assert_eq!(strip_description(&big).len(), 4096);
    }
}
