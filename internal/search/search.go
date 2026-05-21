package search

import (
	"context"
	"fmt"
	"sort"
	"strconv"
	"strings"

	"github.com/alex/cliban/internal/domain"
	"github.com/alex/cliban/internal/store"
	"github.com/sahilm/fuzzy"
)

// Weights are the per-field multipliers applied to each fuzzy hit's raw
// score before summation. Values are 10× the ratios in the spec (B.2) so we
// can stay in integer arithmetic — sahilm/fuzzy returns int scores.
const (
	weightTitle = 30
	weightKey   = 25
	weightLabel = 20
	weightDesc  = 10
)

// Match is a single fuzzy search hit, carrying the issue, its denormalized
// project/labels, and per-field scoring details for downstream ranking and
// highlighting.
type Match struct {
	Issue       *domain.Issue
	ProjectKey  string
	Labels      []string
	Score       int
	FieldScores map[string]int
	Positions   []int
	// BestField is the name of the highest-scoring field ("title", "key",
	// "labels", or "description") that Positions indexes into. Empty when no
	// fuzzy match occurred (the empty-query path).
	BestField string
}

// Options configures a Search call: the query string plus optional filters
// over projects, labels, milestones, statuses, priorities, archive state,
// sub-issue inclusion, and parent scoping, with an optional result limit.
type Options struct {
	Query           string
	Projects        []string
	Labels          []string
	Milestones      []string
	Statuses        []string
	Priorities      []string
	IncludeArchived bool
	ExcludeSubs     bool
	ParentKey       string
	Limit           int
}

// Search runs a fuzzy search across issues according to opts. For each
// candidate (post-filter) issue, four field strings — title, key, joined
// labels, stripped description — are scored independently against the query
// via sahilm/fuzzy. Per-field scores are weighted and summed; any issue with
// at least one matched field is returned (raw fuzzy scores can be negative
// for sparse matches, so we don't gate on Score sign). Sorting is Score desc
// with UpdatedAt desc as the tiebreak.
//
// When opts.Query is empty/whitespace, fuzzy matching is skipped entirely
// and every post-filter candidate is returned sorted by UpdatedAt desc with
// Score=0 and nil FieldScores/Positions.
//
// All store fetches are bulk: one ListIssues, one LabelsForIssues, one
// ProjectsByID — never N+1.
func Search(ctx context.Context, s *store.Store, opts Options) ([]Match, error) {
	filter := store.ListIssuesFilter{
		Projects:        opts.Projects,
		Status:          domain.Status(firstOrEmpty(opts.Statuses)),
		Priority:        domain.Priority(firstOrEmpty(opts.Priorities)),
		MilestoneName:   firstOrEmpty(opts.Milestones),
		LabelNames:      opts.Labels,
		IncludeArchived: opts.IncludeArchived,
		NoSubs:          opts.ExcludeSubs,
	}
	if opts.ParentKey != "" {
		k, err := domain.ParseIssueKey(opts.ParentKey)
		if err != nil {
			return nil, fmt.Errorf("parent key %q: %w", opts.ParentKey, err)
		}
		filter.ParentKey = &k
	}

	issues, err := s.ListIssues(filter)
	if err != nil {
		return nil, err
	}
	if len(issues) == 0 {
		return nil, nil
	}

	ids := make([]int64, len(issues))
	for i, iss := range issues {
		ids[i] = iss.ID
	}
	labelsByID, err := s.LabelsForIssues(ids)
	if err != nil {
		return nil, err
	}
	projByID, err := s.ProjectsByID()
	if err != nil {
		return nil, err
	}

	q := strings.TrimSpace(opts.Query)
	matches := make([]Match, 0, len(issues))

	if q == "" {
		for _, iss := range issues {
			matches = append(matches, Match{
				Issue:      iss,
				ProjectKey: projByID[iss.ProjectID],
				Labels:     labelsByID[iss.ID],
			})
		}
		sort.SliceStable(matches, func(i, j int) bool {
			return matches[i].Issue.UpdatedAt.After(matches[j].Issue.UpdatedAt)
		})
	} else {
		for _, iss := range issues {
			pk := projByID[iss.ProjectID]
			lbls := labelsByID[iss.ID]
			key := pk + "-" + strconv.FormatInt(iss.Seq, 10)
			labelStr := strings.Join(lbls, " ")
			desc := stripDescription(iss.Description)

			fields := []struct {
				name   string
				text   string
				weight int
			}{
				{"title", iss.Title, weightTitle},
				{"key", key, weightKey},
				{"labels", labelStr, weightLabel},
				{"description", desc, weightDesc},
			}

			m := Match{Issue: iss, ProjectKey: pk, Labels: lbls, FieldScores: map[string]int{}}
			matched := false
			bestFieldScore := 0
			for _, f := range fields {
				if f.text == "" {
					continue
				}
				hits := fuzzy.Find(q, []string{f.text})
				if len(hits) == 0 {
					continue
				}
				fieldScore := hits[0].Score * f.weight
				m.Score += fieldScore
				m.FieldScores[f.name] = fieldScore
				if !matched || fieldScore > bestFieldScore {
					bestFieldScore = fieldScore
					m.Positions = hits[0].MatchedIndexes
					m.BestField = f.name
				}
				matched = true
			}
			if matched {
				matches = append(matches, m)
			}
		}
		sort.SliceStable(matches, func(i, j int) bool {
			if matches[i].Score != matches[j].Score {
				return matches[i].Score > matches[j].Score
			}
			return matches[i].Issue.UpdatedAt.After(matches[j].Issue.UpdatedAt)
		})
	}

	if opts.Limit > 0 && len(matches) > opts.Limit {
		matches = matches[:opts.Limit]
	}
	return matches, nil
}

func firstOrEmpty(xs []string) string {
	if len(xs) == 0 {
		return ""
	}
	return xs[0]
}
