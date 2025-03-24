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

	for _, tag := range tags {
		fmt.Println(tag)
	}

	return nil
}
