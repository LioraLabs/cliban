package main

import (
	"fmt"
	"os"

	"github.com/alex/cliban/internal/cli"
)

func main() {
	if err := cli.NewRoot().Execute(); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(cli.ExitCodeFor(err))
	}
}
