package cmd

import (
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NewDetach() *cobra.Command {
	var opts detachOptions
	c := &cobra.Command{
		Use:   "detach",
		Short: "Detach current directory from a repository",

		Args: cobra.NoArgs,

		ValidArgsFunction: NoneCompletion,
	}

	return Build(c, &opts)
}

type detachOptions struct{}

func (o *detachOptions) Complete(c *cobra.Command, args []string) error {
	return nil
}

func (o *detachOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}

	err = term.Confirm("Are you sure to detach the current directory from %q", repo.ID)
	if err != nil {
		return err
	}

	err = ctx.Database.DeleteRepo(repo.ID)
	if err != nil {
		return err
	}

	term.PrintInfo("The current directory has been detached from %q", repo.ID)
	return nil
}
