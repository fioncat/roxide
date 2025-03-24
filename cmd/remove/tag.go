package remove

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
		Short: "Remove a tag, default will remove the latest tag",

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

	var tag *git.Tag
	if o.name != "" {
		tag, err = git.GetTag(ctx.GetRepoPath(), o.name)
	} else {
		tag, err = repoutils.SelectTag(ctx)
	}
	if err != nil {
		return err
	}

	gitCmd := git.WithPath(ctx.GetRepoPath())
	err = gitCmd.Run("tag", "-d", tag.Name)
	if err != nil {
		return err
	}

	err = term.Confirm("Do you want to remove tag %q in remote", tag.Name)
	if err == nil {
		return gitCmd.Run("push", "origin", "--delete", tag.Name)
	}

	return nil
}
