package domain

import (
	"fmt"
	"strconv"
	"strings"
)

type IssueKey struct {
	Project string
	Seq     int64
}

func (k IssueKey) String() string {
	return fmt.Sprintf("%s-%d", k.Project, k.Seq)
}

func ParseIssueKey(s string) (IssueKey, error) {
	s = strings.TrimSpace(s)
	idx := strings.LastIndex(s, "-")
	if idx <= 0 || idx == len(s)-1 {
		return IssueKey{}, fmt.Errorf("invalid issue key %q (want PROJECT-N)", s)
	}
	project := strings.ToUpper(s[:idx])
	seq, err := strconv.ParseInt(s[idx+1:], 10, 64)
	if err != nil || seq <= 0 {
		return IssueKey{}, fmt.Errorf("invalid issue key %q (sequence must be positive integer)", s)
	}
	return IssueKey{Project: project, Seq: seq}, nil
}
