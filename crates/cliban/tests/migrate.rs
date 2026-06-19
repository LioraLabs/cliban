//! Round-trip integrity test for the legacy->core migration (CLI-7).

use std::path::Path;

use cliban::migrate;
use rusqlite::Connection;

const GO_SCHEMA: &str = include_str!("../../../internal/store/schema.sql");

fn build_fixture(path: &Path) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(GO_SCHEMA).unwrap();
    c.execute_batch(
        "INSERT INTO project (id,key,name,description,archived,issue_seq,created_at,updated_at) \
           VALUES (1,'AAA','Alpha','d',0,2,'2026-01-01T00:00:00.123456789Z','2026-01-02T00:00:00Z');\
         INSERT INTO milestone (id,project_id,name,description,target_date,status,created_at,updated_at) \
           VALUES (1,1,'M1','md','2026-03-01','open','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');\
         INSERT INTO label (id,project_id,name,created_at) VALUES (1,1,'bug','2026-01-01T00:00:00Z');\
         INSERT INTO issue (id,project_id,milestone_id,parent_id,seq,title,description,status,priority,position,archived,due_date,created_at,updated_at,completed_at) \
           VALUES (1,1,1,NULL,1,'Parent','pd','done','high',1000.0,0,NULL,'2026-01-01T00:00:00.987654321Z','2026-01-01T00:00:00Z','2026-01-05T00:00:00.111222333Z');\
         INSERT INTO issue (id,project_id,milestone_id,parent_id,seq,title,description,status,priority,position,archived,due_date,created_at,updated_at,completed_at) \
           VALUES (2,1,NULL,1,2,'Child','cd','backlog','none',2000.0,0,'2026-02-01','2026-01-02T00:00:00Z','2026-01-02T00:00:00Z',NULL);\
         INSERT INTO issue_label (issue_id,label_id) VALUES (1,1);\
         INSERT INTO issue_relation (id,from_issue_id,to_issue_id,type,created_at) VALUES (1,1,2,'blocks','2026-01-03T00:00:00Z');\
         INSERT INTO issue_relation (id,from_issue_id,to_issue_id,type,created_at) VALUES (2,1,2,'related_to','2026-01-03T00:00:00Z');\
         INSERT INTO issue_relation (id,from_issue_id,to_issue_id,type,created_at) VALUES (3,2,1,'related_to','2026-01-03T00:00:00Z');",
    )
    .unwrap();
}

#[test]
fn round_trip_fixture() {
    let dir = std::env::temp_dir().join(format!("cli7_rt_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let from = dir.join("go.db");
    let to = dir.join("core.db");
    let _ = std::fs::remove_file(&from);
    let _ = std::fs::remove_file(&to);
    build_fixture(&from);

    let rep = migrate::migrate(&from, &to).expect("migrate ok");
    assert_eq!(rep.projects, 1);
    assert_eq!(rep.milestones, 1);
    assert_eq!(rep.issues, 2);
    assert_eq!(rep.labels, 1);
    assert_eq!(rep.issues_labels, 1);
    assert_eq!(rep.relations, 3);

    let c = Connection::open(&to).unwrap();
    let key: String = c
        .query_row("SELECT key FROM issues WHERE id=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(key, "AAA-1");
    let parent: i64 = c
        .query_row("SELECT parent_id FROM issues WHERE id=2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(parent, 1);
    let (mid, status, prio, completed): (i64, String, String, String) = c
        .query_row(
            "SELECT milestone_id, status, priority, completed_at FROM issues WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(mid, 1);
    assert_eq!(status, "done");
    assert_eq!(prio, "high");
    assert_eq!(completed, "2026-01-05T00:00:00.111222Z");
    let blocks: i64 = c
        .query_row("SELECT count(*) FROM issue_relation WHERE type='blocks'", [], |r| r.get(0))
        .unwrap();
    let related: i64 = c
        .query_row("SELECT count(*) FROM issue_relation WHERE type='related_to'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(blocks, 1);
    assert_eq!(related, 2);

    assert!(!cliban_core::migrations::run(&c).unwrap());

    let _ = std::fs::remove_dir_all(&dir);
}
