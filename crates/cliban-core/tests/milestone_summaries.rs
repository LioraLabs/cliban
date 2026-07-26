//! `milestones::summaries` — the rollup query behind the milestone page and
//! `cliban milestone ls --stats`.

use cliban_core::contexts::milestones::{Sort, SummaryOpts};
use cliban_core::contexts::{activity_log, issues, milestones, projects};
use cliban_core::Store;

async fn store() -> Store {
    let s = Store::open_in_memory().expect("open in-memory store");
    s.call(|c| {
        for key in ["CLI", "LM"] {
            projects::create(
                c,
                projects::CreateProject {
                    key: key.into(),
                    name: key.into(),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    s
}

async fn milestone(s: &Store, project: &str, name: &str, target: Option<&str>) {
    let (project, name) = (project.to_string(), name.to_string());
    let target_date = target.and_then(cliban_core::time::parse_date);
    s.call(move |c| {
        milestones::create(
            c,
            milestones::CreateMilestone {
                project,
                name,
                description: None,
                target_date,
                status: None,
            },
        )
    })
    .await
    .unwrap();
}

async fn issue(s: &Store, project: &str, title: &str, ms: Option<&str>, status: Option<&str>) {
    let (project, title) = (project.to_string(), title.to_string());
    let (ms, status) = (ms.map(String::from), status.map(String::from));
    s.call(move |c| {
        issues::create(
            c,
            &project,
            issues::CreateIssue {
                title,
                milestone: ms,
                status,
                ..Default::default()
            },
        )
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn summaries_count_only_non_archived_issues() {
    let s = store().await;
    milestone(&s, "CLI", "v1", None).await;
    issue(&s, "CLI", "a", Some("v1"), Some("done")).await;
    issue(&s, "CLI", "b", Some("v1"), None).await;
    issue(&s, "CLI", "c", Some("v1"), None).await;
    issue(&s, "CLI", "unmilestoned", None, None).await;

    // Archiving one drops it out of both counts.
    s.call(|c| {
        let i = issues::get_by_key(c, "CLI-3")?.unwrap();
        issues::update(
            c,
            &i,
            issues::UpdateIssue {
                archived: Some(true),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let out = s
        .call(|c| milestones::summaries(c, SummaryOpts::default()))
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].total, 2);
    assert_eq!(out[0].done, 1);
    assert_eq!(out[0].project_key, "CLI");
    assert!((out[0].progress() - 0.5).abs() < f64::EPSILON);
}

#[tokio::test]
async fn activity_sort_puts_the_recently_worked_milestone_first() {
    let s = store().await;
    milestone(&s, "CLI", "aaa-stale", None).await;
    milestone(&s, "CLI", "zzz-busy", None).await;
    issue(&s, "CLI", "quiet", Some("aaa-stale"), None).await;
    issue(&s, "CLI", "busy", Some("zzz-busy"), None).await;

    // Name order would put the stale one first; an activity entry on the
    // other milestone's issue must flip that.
    let by_name = s
        .call(|c| {
            milestones::summaries(
                c,
                SummaryOpts {
                    sort: Sort::Name,
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    assert_eq!(names(&by_name), vec!["aaa-stale", "zzz-busy"]);

    s.call(|c| {
        let i = issues::get_by_key(c, "CLI-2")?.unwrap();
        activity_log::append(
            c,
            &i,
            "status",
            "moved to in-progress",
            &serde_json::json!({}),
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let by_activity = s
        .call(|c| milestones::summaries(c, SummaryOpts::default()))
        .await
        .unwrap();
    assert_eq!(names(&by_activity), vec!["zzz-busy", "aaa-stale"]);
    assert!(by_activity[0].last_activity > by_activity[1].last_activity);
}

/// The `activity_log_entries` table is only written by callers that use the
/// log API; ordinary issue moves are the signal that actually exists in the
/// wild, so they must drive the activity sort too.
#[tokio::test]
async fn moving_an_issue_counts_as_milestone_activity() {
    let s = store().await;
    milestone(&s, "CLI", "aaa-stale", None).await;
    milestone(&s, "CLI", "zzz-busy", None).await;
    issue(&s, "CLI", "quiet", Some("aaa-stale"), None).await;
    issue(&s, "CLI", "busy", Some("zzz-busy"), None).await;

    s.call(|c| {
        let i = issues::get_by_key(c, "CLI-2")?.unwrap();
        issues::move_issue(c, &i, "in-progress")?;
        Ok(())
    })
    .await
    .unwrap();

    let out = s
        .call(|c| milestones::summaries(c, SummaryOpts::default()))
        .await
        .unwrap();
    assert_eq!(
        names(&out),
        vec!["zzz-busy", "aaa-stale"],
        "the milestone whose issue just moved sorts first"
    );
    assert!(out[0].last_activity > out[0].milestone.updated_at);
}

#[tokio::test]
async fn empty_milestone_falls_back_to_its_own_updated_at() {
    let s = store().await;
    milestone(&s, "CLI", "v1", None).await;
    let out = s
        .call(|c| milestones::summaries(c, SummaryOpts::default()))
        .await
        .unwrap();
    assert_eq!(out[0].last_activity, out[0].milestone.updated_at);
}

#[tokio::test]
async fn target_sort_puts_undated_milestones_last() {
    let s = store().await;
    milestone(&s, "CLI", "undated", None).await;
    milestone(&s, "CLI", "later", Some("2026-12-01")).await;
    milestone(&s, "CLI", "sooner", Some("2026-08-01")).await;
    let out = s
        .call(|c| {
            milestones::summaries(
                c,
                SummaryOpts {
                    sort: Sort::Target,
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    assert_eq!(names(&out), vec!["sooner", "later", "undated"]);
}

#[tokio::test]
async fn project_and_status_filters_narrow_the_list() {
    let s = store().await;
    milestone(&s, "CLI", "cli-open", None).await;
    milestone(&s, "CLI", "cli-done", None).await;
    milestone(&s, "LM", "lm-open", None).await;
    s.call(|c| {
        let m = milestones::get(c, "CLI", "cli-done")?.unwrap();
        milestones::update(
            c,
            &m,
            milestones::UpdateMilestone {
                status: Some("completed".into()),
                ..Default::default()
            },
        )?;
        Ok(())
    })
    .await
    .unwrap();

    let all = s
        .call(|c| milestones::summaries(c, SummaryOpts::default()))
        .await
        .unwrap();
    assert_eq!(all.len(), 3, "no project filter spans every project");

    let cli_open = s
        .call(|c| {
            milestones::summaries(
                c,
                SummaryOpts {
                    project: Some("CLI"),
                    status: Some("open"),
                    sort: Sort::Name,
                },
            )
        })
        .await
        .unwrap();
    assert_eq!(names(&cli_open), vec!["cli-open"]);

    let unknown = s
        .call(|c| {
            milestones::summaries(
                c,
                SummaryOpts {
                    project: Some("NOPE"),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    assert!(unknown.is_empty(), "unknown project key yields nothing");
}

fn names(v: &[milestones::MilestoneSummary]) -> Vec<&str> {
    v.iter().map(|s| s.milestone.name.as_str()).collect()
}
