package cmd

import (
	"fmt"
	"os"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NewSquash() *cobra.Command {
	var opts squashOptions

	c := &cobra.Command{
		Use:   "squash [TARGET]",
		Short: "Squash multiple commits into one",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: BuildCompletion(BranchCompletion),
	}

	c.Flags().BoolVarP(&opts.upstream, "upstream", "u", false, "Upstream mode, only used for forked repo")
	c.Flags().StringVarP(&opts.message, "message", "m", "", "Commit message")

	return BuildWithForceNoCache(c, &opts)
}

type squashOptions struct {
	target string

	upstream bool

	message string
}

func (o *squashOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.target = args[0]
	}

	return nil
}

func (o *squashOptions) Run(ctx *context.Context) error {
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

	commits, err := remote.CommitsBetween(o.target)
	if err != nil {
		return err
	}

	fmt.Fprintln(os.Stderr)
	if len(commits) == 0 {
		fmt.Fprintln(os.Stderr, "No commit to squash")
		return nil
	}

	if len(commits) == 1 {
		fmt.Fprintln(os.Stderr, "No need to squash a single commit")
		return nil
	}

	fmt.Fprintf(os.Stderr, "Found %d commits to squash:\n", len(commits))
	for _, commit := range commits {
		fmt.Fprintf(os.Stderr, "  * %s\n", commit)
	}

	err = term.Confirm("Continue")
	if err != nil {
		return err
	}

	gitCmd := git.WithPath(ctx.GetRepoPath())
	set := fmt.Sprintf("HEAD~%d", len(commits))

	err = gitCmd.Run("reset", "--soft", set)
	if err != nil {
		return err
	}

	args := []string{"commit"}
	if o.message != "" {
		args = append(args, "-m", o.message)
	}

	gitCmd.NoCapture()
	return gitCmd.Run(args...)
}
