package switchcmd

import (
	"errors"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/spf13/cobra"
)

func newBranch() *cobra.Command {
	var opts branchOptions
	c := &cobra.Command{
		Use:   "branch",
		Short: "Switch to a branch",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.BranchCompletion),
	}

	return cmd.Build(c, &opts)
}

type branchOptions struct {
	name string
}

func (o *branchOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.name = args[0]
	}

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

	if o.name != "" {
		return gitCmd.Run("checkout", o.name)
	}

	branches, err := git.ListRemoteBranches(ctx.GetRepoPath())
	if err != nil {
		return err
	}
	currentBranch, err := git.GetCurrentBranch(ctx.GetRepoPath())
	if err != nil && !errors.Is(err, git.ErrNoCurrentBranch) {
		return err
	}

	filtered := make([]string, 0, len(branches))
	for _, branch := range branches {
		if branch == currentBranch {
			continue
		}
		filtered = append(filtered, branch)
	}
	branches = filtered

	if len(branches) == 0 {
		return errors.New("no branch to switch")
	}

	if len(branches) == 1 {
		return gitCmd.Run("checkout", branches[0])
	}

	idx, err := ctx.Selector.Select(branches)
	if err != nil {
		return err
	}

	return gitCmd.Run("checkout", branches[idx])
}
