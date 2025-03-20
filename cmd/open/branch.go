package open

import (
	"path/filepath"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/spf13/cobra"
)

func newBranch() *cobra.Command {
	var opts branchOptions

	c := &cobra.Command{
		Use:   "branch [NAME]",
		Short: "Open the branch in the browser",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.BranchCompletion),
	}

	return cmd.BuildWithForceNoCache(c, &opts)
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

	branch := o.name
	if branch == "" {
		currentBranch, err := git.GetCurrentBranch(ctx.GetRepoPath())
		if err != nil {
			return err
		}
		branch = currentBranch
	}

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return err
	}

	apiRepo, err := api.GetRepo(repo.Owner, repo.Name)
	if err != nil {
		return err
	}

	url := apiRepo.WebURL
	url = filepath.Join(url, "tree", branch)
	return openURL(url)
}
