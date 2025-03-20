package open

import (
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/spf13/cobra"
)

func newRepo() *cobra.Command {
	var opts repoOptions

	c := &cobra.Command{
		Use:   "repo [HEAD] [QUERY]",
		Short: "Open the repository in the browser",

		Args: cobra.MaximumNArgs(2),

		ValidArgsFunction: cmd.BuildCompletion(cmd.RepoCompletion),
	}

	return cmd.BuildWithForceNoCache(c, &opts)
}

type repoOptions struct {
	args []string
}

func (o *repoOptions) Complete(c *cobra.Command, args []string) error {
	o.args = args
	return nil
}

func (o *repoOptions) Run(ctx *context.Context) error {
	var repo *db.Repository
	var err error
	if len(o.args) == 0 {
		repo, err = repoutils.GetCurrentRepo(ctx)
		if err != nil {
			return err
		}
	}

	if repo == nil {
		ch := choice.New(ctx, o.args)
		repo, err = ch.One(choice.OneOptions{
			Mode: choice.ModeSelect,
		})
		if err != nil {
			return err
		}
	}

	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return err
	}

	apiRepo, err := api.GetRepo(repo.Owner, repo.Name)
	if err != nil {
		return err
	}

	return openURL(apiRepo.WebURL)
}
