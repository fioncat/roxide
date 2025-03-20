package create

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
		Use:   "branch NAME",
		Short: "Create a new branch",

		Args: cobra.ExactArgs(1),

		ValidArgsFunction: cmd.NoneCompletion,
	}

	return cmd.Build(c, &opts)
}

type branchOptions struct {
	name string
}

func (o *branchOptions) Complete(c *cobra.Command, args []string) error {
	o.name = args[0]
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

	gitCmd := git.WithPath(ctx.GetRepoPath())
	err = gitCmd.Run("checkout", "-b", o.name)
	if err != nil {
		return err
	}

	err = term.Confirm("Do you want to push branch %q to remote", o.name)
	if err == nil {
		return gitCmd.Run("push", "--set-upstream", "origin", o.name)
	}

	return nil
}
