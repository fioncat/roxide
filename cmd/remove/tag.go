package remove

import (
	"errors"

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

	var toDelete string
	if o.name != "" {
		toDelete = o.name
	} else {
		tags, err := git.ListTags(ctx.GetRepoPath())
		if err != nil {
			return err
		}

		if len(tags) == 0 {
			return errors.New("no tag to remove")
		}

		if len(tags) == 1 {
			toDelete = string(tags[0])
		} else {
			items := make([]string, 0, len(tags))
			for _, tag := range tags {
				items = append(items, string(tag))
			}

			idx, err := ctx.Selector.Select(items)
			if err != nil {
				return err
			}
			toDelete = items[idx]
		}

	}

	gitCmd := git.WithPath(ctx.GetRepoPath())
	err = gitCmd.Run("tag", "-d", toDelete)
	if err != nil {
		return err
	}

	err = term.Confirm("Do you want to remove tag %q in remote", toDelete)
	if err == nil {
		return gitCmd.Run("push", "origin", "--delete", toDelete)
	}

	return nil
}
