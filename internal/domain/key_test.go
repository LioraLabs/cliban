package domain

import "testing"

func TestIssueKey(t *testing.T) {
	k := IssueKey{Project: "CLI", Seq: 42}
	if k.String() != "CLI-42" {
		t.Errorf("String() = %q, want CLI-42", k.String())
	}
}

func TestParseIssueKey(t *testing.T) {
	cases := []struct {
		in      string
		project string
		seq     int64
		ok      bool
	}{
		{"CLI-42", "CLI", 42, true},
		{"cli-7", "CLI", 7, true},
		{"FOO-1", "FOO", 1, true},
		{"CLI", "", 0, false},
		{"CLI-", "", 0, false},
		{"-42", "", 0, false},
		{"CLI-abc", "", 0, false},
		{"", "", 0, false},
	}
	for _, tc := range cases {
		got, err := ParseIssueKey(tc.in)
		if tc.ok && err != nil {
			t.Errorf("ParseIssueKey(%q) err: %v", tc.in, err)
		}
		if !tc.ok && err == nil {
			t.Errorf("ParseIssueKey(%q) want err", tc.in)
		}
		if tc.ok && (got.Project != tc.project || got.Seq != tc.seq) {
			t.Errorf("ParseIssueKey(%q) = %+v, want {%s %d}", tc.in, got, tc.project, tc.seq)
		}
	}
}
