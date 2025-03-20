package remove

import (
	"errors"
	"fmt"

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
		Use:   "branch [NAME]",
		Short: "Remove a branch, default will remove current branch",

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

	var isCurrent bool
	if o.name == "" {
		currentBranch, err := git.GetCurrentBranch(ctx.GetRepoPath())
		if err != nil {
			return err
		}
		o.name = currentBranch
		isCurrent = true
	} else {
		branches, err := git.ListBranches(ctx.GetRepoPath())
		if err != nil {
			return err
		}
		var found bool
		for _, branch := range branches {
			if branch.Name == o.name {
				isCurrent = branch.Current
				found = true
				break
			}
		}
		if !found {
			return fmt.Errorf("cannot find branch %q", o.name)
		}
	}

	defaultBranch, err := git.GetDefaultBranch(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if o.name == defaultBranch {
		return errors.New("cannot remove default branch")
	}

	gitCmd := git.WithPath(ctx.GetRepoPath())

	if isCurrent {
		err = gitCmd.Run("checkout", defaultBranch)
		if err != nil {
			return err
		}
	}

	err = gitCmd.Run("branch", "-D", o.name)
	if err != nil {
		return err
	}

	err = term.Confirm("Do you want to remove branch %q in remote", o.name)
	if err == nil {
		return gitCmd.Run("push", "origin", "--delete", o.name)
	}

	return nil
}
