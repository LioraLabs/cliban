package cli

import (
	"errors"

	"github.com/alex/cliban/internal/store"
)

func ExitCodeFor(err error) int {
	switch {
	case err == nil:
		return 0
	case errors.Is(err, store.ErrNotFound):
		return 1
	case errors.Is(err, store.ErrValidation), errors.Is(err, store.ErrConflict):
		return 2
	default:
		return 3
	}
}
