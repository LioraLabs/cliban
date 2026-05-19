package domain

import (
	"fmt"
	"strings"
)

type Status string

const (
	StatusBacklog    Status = "backlog"
	StatusInProgress Status = "in-progress"
	StatusBlocked    Status = "blocked"
	StatusInReview   Status = "in-review"
	StatusDone       Status = "done"
)

func AllStatuses() []Status {
	return []Status{StatusBacklog, StatusInProgress, StatusBlocked, StatusInReview, StatusDone}
}

func ParseStatus(s string) (Status, error) {
	norm := Status(strings.ToLower(strings.TrimSpace(s)))
	for _, v := range AllStatuses() {
		if v == norm {
			return v, nil
		}
	}
	return "", fmt.Errorf("invalid status %q (valid: backlog, in-progress, blocked, in-review, done)", s)
}
