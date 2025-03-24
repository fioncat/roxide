package main

import (
	"errors"
	"fmt"
	"os"

	"github.com/fatih/color"
	"github.com/fioncat/roxide/build"
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/cmd/create"
	"github.com/fioncat/roxide/cmd/get"
	"github.com/fioncat/roxide/cmd/open"
	"github.com/fioncat/roxide/cmd/remove"
	"github.com/fioncat/roxide/cmd/switchcmd"
	rerrors "github.com/fioncat/roxide/pkg/errors"
	"github.com/mattn/go-isatty"
	"github.com/spf13/cobra"
)

func newCmd() *cobra.Command {
	c := &cobra.Command{
		Use:   "roxide",
		Short: "Manage your git repositories",

		SilenceErrors: true,
		SilenceUsage:  true,

		// Completion is impletemented by `init` command, so disable this
		CompletionOptions: cobra.CompletionOptions{
			DisableDefaultCmd: true,
		},

		Version: build.Version,
	}

	c.AddCommand(cmd.NewAttach())
	c.AddCommand(cmd.NewConfig())
	c.AddCommand(cmd.NewDetach())
	c.AddCommand(cmd.NewDisplay())
	c.AddCommand(cmd.NewHome())
	c.AddCommand(cmd.NewInit())
	c.AddCommand(cmd.NewMerge())
	c.AddCommand(cmd.NewRebase())
	c.AddCommand(cmd.NewSquash())
	c.AddCommand(cmd.NewSync())
	c.AddCommand(create.New())
	c.AddCommand(get.New())
	c.AddCommand(open.New())
	c.AddCommand(remove.New())
	c.AddCommand(switchcmd.New())

	return c
}

func main() {
	color.NoColor = false
	if !isatty.IsTerminal(os.Stderr.Fd()) {
		color.NoColor = true
	}

	c := newCmd()

	err := c.Execute()
	if err != nil {
		if !errors.Is(err, rerrors.ErrSilenceExit) {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		}
		os.Exit(1)
	}
}
