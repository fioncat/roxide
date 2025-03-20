package get

import (
	"fmt"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/jedib0t/go-pretty/v6/table"
	"github.com/spf13/cobra"
)

func newBranch() *cobra.Command {
	var opts branchOptions
	c := &cobra.Command{
		Use:   "branch",
		Short: "List local branches",

		Args: cobra.NoArgs,

		ValidArgsFunction: cmd.NoneCompletion,
	}

	c.Flags().BoolVarP(&opts.json, "json", "", false, "output as json")

	return cmd.Build(c, &opts)
}

type branchOptions struct {
	json bool
}

func (o *branchOptions) Complete(c *cobra.Command, args []string) error {
	return nil
}

func (o *branchOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	term.Mute = true
	branches, err := git.ListBranches(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if o.json {
		return term.PrintJson(branches)
	}

	if len(branches) == 0 {
		fmt.Println("<empty list>")
		return nil
	}

	t := table.NewWriter()
	t.AppendHeader(table.Row{"Name", "Status"})

	for _, branch := range branches {
		name := branch.Name
		if branch.Current {
			name = fmt.Sprintf("* %s", name)
		}
		row := table.Row{name, branch.StatusString()}
		t.AppendRow(row)
	}

	fmt.Println(t.Render())
	return nil
}
