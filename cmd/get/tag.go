package get

import (
	"fmt"

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

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.TagCompletion),
	}

	c.Flags().IntVarP(&opts.page, "page", "p", 1, "the page number")
	c.Flags().IntVarP(&opts.limit, "limit", "", 10, "the number of repositories per page")

	return cmd.Build(c, &opts)
}

type tagOptions struct {
	name string

	page  int
	limit int
}

func (o *tagOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.name = args[0]
	}
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
	if o.name != "" {
		tag, err := git.GetTag(ctx.GetRepoPath(), o.name)
		if err != nil {
			return err
		}
		fmt.Println(tag)
		return nil
	}

	tags, err := git.ListTags(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	total := len(tags)
	items := paginate(tags, o.page, o.limit)

	showTable([]string{"Tag"}, items, total, o.page, o.limit)
	return nil
}
