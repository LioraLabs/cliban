//! On-disk editor buffers for the e/E/n/N $EDITOR flows. Mirrors cliban's Go
//! `internal/issuebuf` format: `---` frontmatter + markdown body.

#[derive(Debug, Default, Clone, PartialEq)]
pub struct IssueBuffer {
    pub header: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub milestone: String,
    pub parent: String,
    pub description: String,
}

impl IssueBuffer {
    pub fn serialize(&self) -> String {
        let mut s = String::new();
        if !self.header.is_empty() {
            s.push_str(&self.header);
            if !self.header.ends_with('\n') {
                s.push('\n');
            }
        }
        s.push_str(&format!(
            "---\ntitle:     {}\nstatus:    {}\npriority:  {}\nmilestone: {}\nparent:    {}\n---\n",
            self.title, self.status, self.priority, self.milestone, self.parent
        ));
        s.push_str(&self.description);
        if !self.description.ends_with('\n') {
            s.push('\n');
        }
        s
    }
}

pub fn split_frontmatter(src: &str) -> Result<(String, String), String> {
    let mut lines = src.lines();
    let mut found = false;
    for l in lines.by_ref() {
        if l.trim() == "---" {
            found = true;
            break;
        }
    }
    if !found {
        return Err("missing opening ---".into());
    }
    let mut front = String::new();
    let mut closed = false;
    for l in lines.by_ref() {
        if l.trim() == "---" {
            closed = true;
            break;
        }
        front.push_str(l);
        front.push('\n');
    }
    if !closed {
        return Err("missing closing ---".into());
    }
    Ok((front, lines.collect::<Vec<&str>>().join("\n")))
}

pub fn parse_issue(src: &str) -> Result<IssueBuffer, String> {
    let (front, body) = split_frontmatter(src)?;
    let mut b = IssueBuffer {
        description: body.trim().to_string(),
        ..Default::default()
    };
    if !b.description.is_empty() {
        b.description.push('\n');
    }
    for line in front.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().to_string();
            match k.trim() {
                "title" => b.title = v,
                "status" => b.status = v,
                "priority" => b.priority = v,
                "milestone" => b.milestone = v,
                "parent" => b.parent = v,
                _ => {}
            }
        }
    }
    if b.title.is_empty() {
        return Err("title is required".into());
    }
    Ok(b)
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct MilestoneBuffer {
    pub header: String,
    pub name: String,
    pub target: String,
    pub status: String,
    pub description: String,
}

impl MilestoneBuffer {
    pub fn serialize(&self) -> String {
        let mut s = String::new();
        if !self.header.is_empty() {
            s.push_str(&self.header);
            if !self.header.ends_with('\n') {
                s.push('\n');
            }
        }
        s.push_str(&format!(
            "---\nname:    {}\ntarget:  {}\nstatus:  {}\n---\n",
            self.name, self.target, self.status
        ));
        s.push_str(&self.description);
        if !self.description.ends_with('\n') {
            s.push('\n');
        }
        s
    }
}

pub fn parse_milestone(src: &str) -> Result<MilestoneBuffer, String> {
    let (front, body) = split_frontmatter(src)?;
    let mut b = MilestoneBuffer {
        description: body.trim().to_string(),
        ..Default::default()
    };
    if !b.description.is_empty() {
        b.description.push('\n');
    }
    for line in front.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().to_string();
            match k.trim() {
                "name" => b.name = v,
                "target" => b.target = v,
                "status" => b.status = v,
                _ => {}
            }
        }
    }
    if b.name.is_empty() {
        return Err("name is required".into());
    }
    Ok(b)
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ProjectBuffer {
    pub header: String,
    pub name: String,
    pub description: String,
}

impl ProjectBuffer {
    pub fn serialize(&self) -> String {
        let mut s = String::new();
        if !self.header.is_empty() {
            s.push_str(&self.header);
            if !self.header.ends_with('\n') {
                s.push('\n');
            }
        }
        s.push_str(&format!("---\nname: {}\n---\n", self.name));
        s.push_str(&self.description);
        if !self.description.ends_with('\n') {
            s.push('\n');
        }
        s
    }
}

pub fn parse_project(src: &str) -> Result<ProjectBuffer, String> {
    let (front, body) = split_frontmatter(src)?;
    let mut b = ProjectBuffer {
        description: body.trim().to_string(),
        ..Default::default()
    };
    if !b.description.is_empty() {
        b.description.push('\n');
    }
    for line in front.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == "name" {
                b.name = v.trim().to_string();
            }
        }
    }
    if b.name.is_empty() {
        return Err("name is required".into());
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_buffer_round_trips() {
        let b = IssueBuffer {
            header: "# hi".into(),
            title: "T".into(),
            status: "backlog".into(),
            priority: "high".into(),
            milestone: "M1".into(),
            parent: "".into(),
            description: "Body\n".into(),
        };
        let p = parse_issue(&b.serialize()).unwrap();
        assert_eq!(p.title, "T");
        assert_eq!(p.status, "backlog");
        assert_eq!(p.priority, "high");
        assert_eq!(p.milestone, "M1");
        assert_eq!(p.description, "Body\n");
    }

    #[test]
    fn parse_issue_requires_title() {
        assert!(parse_issue("---\ntitle:\n---\n").is_err());
    }

    #[test]
    fn milestone_buffer_round_trips() {
        let b = MilestoneBuffer {
            header: "".into(),
            name: "M1".into(),
            target: "2026-01-01".into(),
            status: "open".into(),
            description: "".into(),
        };
        let p = parse_milestone(&b.serialize()).unwrap();
        assert_eq!(p.name, "M1");
        assert_eq!(p.target, "2026-01-01");
        assert_eq!(p.status, "open");
    }

    #[test]
    fn project_buffer_round_trips() {
        let b = ProjectBuffer {
            header: "".into(),
            name: "Cliban".into(),
            description: "Desc\n".into(),
        };
        let p = parse_project(&b.serialize()).unwrap();
        assert_eq!(p.name, "Cliban");
        assert_eq!(p.description, "Desc\n");
    }
}
