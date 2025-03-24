package get

import (
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
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

	c.Flags().IntVarP(&opts.page, "page", "p", 1, "the page number")
	c.Flags().IntVarP(&opts.limit, "limit", "", 10, "the number of repositories per page")
	c.Flags().BoolVarP(&opts.json, "json", "", false, "output as json")

	return cmd.Build(c, &opts)
}

type branchOptions struct {
	page  int
	limit int

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

	total := len(branches)
	items := paginate(branches, o.page, o.limit)
	titles := []string{
		"Name",
		"Status",
		"Commit",
	}

	showTable(titles, items, total, o.page, o.limit)
	return nil
}
