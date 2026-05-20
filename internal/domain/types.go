package domain

import "time"

type Project struct {
	ID          int64
	Key         string
	Name        string
	Description string
	Archived    bool
	IssueSeq    int64
	// AutoArchiveDoneAfterDays, when non-nil, instructs cleanup operations
	// (e.g. `archive-done --auto`) to archive done issues older than this
	// many days since they were completed.
	AutoArchiveDoneAfterDays *int64
	CreatedAt                time.Time
	UpdatedAt                time.Time
}

type MilestoneStatus string

const (
	MilestoneOpen      MilestoneStatus = "open"
	MilestoneCompleted MilestoneStatus = "completed"
	MilestoneCancelled MilestoneStatus = "cancelled"
)

type Milestone struct {
	ID          int64
	ProjectID   int64
	Name        string
	Description string
	TargetDate  *time.Time
	Status      MilestoneStatus
	CreatedAt   time.Time
	UpdatedAt   time.Time
}

type Issue struct {
	ID          int64
	ProjectID   int64
	MilestoneID *int64
	ParentID    *int64
	Seq         int64
	Title       string
	Description string
	Status      Status
	Priority    Priority
	Position    float64
	Archived    bool
	DueDate     *time.Time
	CreatedAt   time.Time
	UpdatedAt   time.Time
	CompletedAt *time.Time
}
