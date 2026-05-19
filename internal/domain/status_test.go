package domain

import "testing"

func TestParseStatus(t *testing.T) {
	cases := []struct {
		in   string
		want Status
		ok   bool
	}{
		{"backlog", StatusBacklog, true},
		{"in-progress", StatusInProgress, true},
		{"blocked", StatusBlocked, true},
		{"in-review", StatusInReview, true},
		{"done", StatusDone, true},
		{"BACKLOG", StatusBacklog, true},
		{"", "", false},
		{"todo", "", false},
	}
	for _, tc := range cases {
		got, err := ParseStatus(tc.in)
		if tc.ok && err != nil {
			t.Errorf("ParseStatus(%q) unexpected err: %v", tc.in, err)
		}
		if !tc.ok && err == nil {
			t.Errorf("ParseStatus(%q) want err, got %q", tc.in, got)
		}
		if tc.ok && got != tc.want {
			t.Errorf("ParseStatus(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}

func TestAllStatuses(t *testing.T) {
	got := AllStatuses()
	if len(got) != 5 {
		t.Fatalf("AllStatuses len = %d, want 5", len(got))
	}
	if got[0] != StatusBacklog || got[4] != StatusDone {
		t.Errorf("AllStatuses ordering wrong: %v", got)
	}
}
