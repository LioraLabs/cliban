use cliban_core::contexts::{issues, labels, milestones, projects, relations};
use cliban_core::Store;

async fn store_with_project() -> Store {
    let s = Store::open_in_memory().expect("open in-memory store");
    s.call(|c| {
        projects::create(c, projects::CreateProject {
            key: "CLI".into(),
            name: "Cliban".into(),
            ..Default::default()
        })
    })
    .await
    .unwrap();
    s
}

async fn new_issue(s: &Store, title: &str) -> cliban_core::schema::Issue {
    let title = title.to_string();
    s.call(move |c| {
        issues::create(c, "CLI", issues::CreateIssue { title, ..Default::default() })
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn project_round_trips() {
    let s = Store::open_in_memory().unwrap();
    let p = s
        .call(|c| {
            projects::create(c, projects::CreateProject {
                key: "cli".into(),
                name: "Cliban".into(),
                ..Default::default()
            })
        })
        .await
        .unwrap();
    assert_eq!(p.key, "CLI"); // upcased
    let got = s.call(|c| projects::get_by_key(c, "cli")).await.unwrap();
    assert_eq!(got.unwrap().name, "Cliban");
    let all = s.call(|c| projects::list(c)).await.unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn milestone_and_label_round_trip() {
    let s = store_with_project().await;
    let m = s
        .call(|c| {
            milestones::create(c, milestones::CreateMilestone {
                project: "CLI".into(),
                name: "M1".into(),
                description: None,
                target_date: None,
                status: None,
            })
        })
        .await
        .unwrap();
    assert_eq!(m.status, "open");
    let ms = s.call(|c| milestones::list(c, Some("CLI"))).await.unwrap();
    assert_eq!(ms.len(), 1);
    let l = s.call(|c| labels::create(c, "CLI", "bug")).await.unwrap();
    assert_eq!(l.name, "bug");
    let ls = s.call(|c| labels::list(c, "CLI")).await.unwrap();
    assert_eq!(ls.len(), 1);
}

#[tokio::test]
async fn issue_create_defaults_and_key() {
    let s = store_with_project().await;
    let i = new_issue(&s, "first").await;
    assert_eq!(i.key, "CLI-1");
    assert_eq!(i.status, "backlog");
    assert_eq!(i.priority, "none");
    assert!(i.completed_at.is_none());
}

#[tokio::test]
async fn issue_move_sets_and_clears_completed_at() {
    let s = store_with_project().await;
    let i = new_issue(&s, "movable").await;
    let done = {
        let i = i.clone();
        s.call(move |c| issues::move_issue(c, &i, "done")).await.unwrap()
    };
    assert_eq!(done.status, "done");
    assert!(done.completed_at.is_some());
    let back = s
        .call(move |c| issues::move_issue(c, &done, "backlog"))
        .await
        .unwrap();
    assert!(back.completed_at.is_none());
}

#[tokio::test]
async fn issue_update_title_and_priority() {
    let s = store_with_project().await;
    let i = new_issue(&s, "edit me").await;
    let updated = s
        .call(move |c| {
            issues::update(c, &i, issues::UpdateIssue {
                title: Some("edited".into()),
                priority: Some("high".into()),
                ..Default::default()
            })
        })
        .await
        .unwrap();
    assert_eq!(updated.title, "edited");
    assert_eq!(updated.priority, "high");
}

#[tokio::test]
async fn issue_labels_round_trip() {
    let s = store_with_project().await;
    let i = new_issue(&s, "labelled").await;
    let i = s
        .call(move |c| issues::set_labels(c, &i, &["a".into(), "b".into()]))
        .await
        .unwrap();
    let names = {
        let id = i.id;
        s.call(move |c| issues::label_names(c, id)).await.unwrap()
    };
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}

#[tokio::test]
async fn relations_blocks_related_and_blocked_list() {
    let s = store_with_project().await;
    let a = new_issue(&s, "blocker").await;   // CLI-1
    let b = new_issue(&s, "blocked").await;   // CLI-2
    s.call(|c| relations::add(c, "CLI-1", "CLI-2", "blocks")).await.unwrap();
    s.call(|c| relations::add(c, "CLI-1", "CLI-2", "related_to")).await.unwrap();

    let b_rels = {
        let bid = b.id;
        s.call(move |c| relations::for_issue(c, bid)).await.unwrap()
    };
    assert!(b_rels.iter().any(|r| r.kind == "blocked_by" && r.target_key == "CLI-1"));
    assert!(b_rels.iter().any(|r| r.kind == "related_to" && r.target_key == "CLI-1"));

    let blocked = s.call(|c| relations::list_blocked(c, Some("CLI"))).await.unwrap();
    assert!(blocked.iter().any(|i| i.key == "CLI-2"));

    let a2 = a.clone();
    s.call(move |c| issues::move_issue(c, &a2, "done")).await.unwrap();
    let blocked2 = s.call(|c| relations::list_blocked(c, Some("CLI"))).await.unwrap();
    assert!(!blocked2.iter().any(|i| i.key == "CLI-2"));
}
