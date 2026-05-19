package domain

import (
	"fmt"
	"strings"
)

type Priority string

const (
	PriorityNone   Priority = "none"
	PriorityLow    Priority = "low"
	PriorityMedium Priority = "medium"
	PriorityHigh   Priority = "high"
	PriorityUrgent Priority = "urgent"
)

func AllPriorities() []Priority {
	return []Priority{PriorityNone, PriorityLow, PriorityMedium, PriorityHigh, PriorityUrgent}
}

func ParsePriority(s string) (Priority, error) {
	norm := Priority(strings.ToLower(strings.TrimSpace(s)))
	for _, v := range AllPriorities() {
		if v == norm {
			return v, nil
		}
	}
	return "", fmt.Errorf("invalid priority %q (valid: none, low, medium, high, urgent)", s)
}
