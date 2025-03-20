package cmd

import (
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/spf13/cobra"
)

func NewRebase() *cobra.Command {
	var opts rebaseOptions

	c := &cobra.Command{
		Use:   "rebase [TARGET]",
		Short: "Rebase the current branch",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: BuildCompletion(BranchCompletion),
	}

	c.Flags().BoolVarP(&opts.upstream, "upstream", "u", false, "Upstream mode, only used for forked repo")

	return BuildWithForceNoCache(c, &opts)
}

type rebaseOptions struct {
	target string

	upstream bool
}

func (o *rebaseOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.target = args[0]
	}

	return nil
}

func (o *rebaseOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	err = git.EnsureNoUncommittedChanges(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	remote, err := repoutils.GetRemote(ctx, o.upstream)
	if err != nil {
		return err
	}

	target, err := remote.GetTarget(o.target)
	if err != nil {
		return err
	}

	gitCmd := git.WithPath(ctx.GetRepoPath())

	return gitCmd.Run("rebase", target)
}
