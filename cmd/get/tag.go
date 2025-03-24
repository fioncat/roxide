package get

import (
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newTag() *cobra.Command {
	var opts tagOptions
	c := &cobra.Command{
		Use:   "tag [NAME]",
		Short: "List tags",

		Args: cobra.NoArgs,

		ValidArgsFunction: cmd.NoneCompletion,
	}

	setListFlags(c, &opts.listOptions)

	return cmd.Build(c, &opts)
}

type tagOptions struct {
	listOptions
}

func (o *tagOptions) Complete(c *cobra.Command, args []string) error {
	return nil
}

func (o *tagOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	term.Mute = true
	tags, err := git.ListTags(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if o.json {
		return term.PrintJson(tags)
	}

	total := len(tags)
	items := paginate(tags, o.page, o.limit)
	titles := []string{
		"Tag", "CommitID", "Commit",
	}

	showTable(titles, items, total, o.page, o.limit)
	return nil
}
