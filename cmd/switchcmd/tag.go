package switchcmd

import (
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/spf13/cobra"
)

func newTag() *cobra.Command {
	var opts tagOptions
	c := &cobra.Command{
		Use:   "tag",
		Short: "Switch to a tag",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.TagCompletion),
	}

	return cmd.Build(c, &opts)
}

type tagOptions struct {
	name string
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
	gitCmd := git.WithPath(ctx.GetRepoPath())

	if o.name != "" {
		return gitCmd.Run("checkout", o.name)
	}

	tags, err := git.ListTags(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if len(tags) == 0 {
		return nil
	}

	if len(tags) == 1 {
		return gitCmd.Run("checkout", tags[0].Name)
	}

	items := make([]string, 0, len(tags))
	for _, tag := range tags {
		items = append(items, tag.Name)
	}

	idx, err := ctx.Selector.Select(items)
	if err != nil {
		return err
	}

	return gitCmd.Run("checkout", tags[idx].Name)
}
