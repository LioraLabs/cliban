package domain

import "testing"

func TestParsePriority(t *testing.T) {
	cases := []struct {
		in   string
		want Priority
		ok   bool
	}{
		{"none", PriorityNone, true},
		{"low", PriorityLow, true},
		{"medium", PriorityMedium, true},
		{"high", PriorityHigh, true},
		{"urgent", PriorityUrgent, true},
		{"URGENT", PriorityUrgent, true},
		{"critical", "", false},
	}
	for _, tc := range cases {
		got, err := ParsePriority(tc.in)
		if tc.ok && err != nil {
			t.Errorf("ParsePriority(%q) err: %v", tc.in, err)
		}
		if !tc.ok && err == nil {
			t.Errorf("ParsePriority(%q) want err", tc.in)
		}
		if tc.ok && got != tc.want {
			t.Errorf("ParsePriority(%q) = %q, want %q", tc.in, got, tc.want)
		}
	}
}
