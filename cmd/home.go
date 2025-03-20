package cmd

import (
	"fmt"

	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NewHome() *cobra.Command {
	var opts homeOptions

	c := &cobra.Command{
		Use: "home [HEAD] [QUERY]",

		Short: "Change to a repository home, if it does not exist, create it",

		Args: cobra.MaximumNArgs(2),

		ValidArgsFunction: BuildCompletion(RepoCompletion),
	}

	c.Flags().BoolVarP(&opts.search, "search", "s", false, "use search mode rather than fuzzy match")
	c.Flags().BoolVarP(&opts.thin, "thin", "t", false, "clone the repo with a thin history")

	return BuildWithForceNoCache(c, &opts)
}

type homeOptions struct {
	args []string

	search bool

	thin bool
}

func (o *homeOptions) Complete(c *cobra.Command, args []string) error {
	o.args = args
	return nil
}

func (o *homeOptions) Run(ctx *context.Context) error {
	ch := choice.New(ctx, o.args)
	var mode choice.Mode
	if o.search {
		mode = choice.ModeSelect
	} else {
		mode = choice.ModeFuzzy
	}
	opts := choice.OneOptions{Mode: mode}

	repo, err := ch.One(opts)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	ownerConfig := ctx.GetOwnerConfig()
	if repo.NewCreated {
		err = term.Confirm("Do you want to create %s", repo.String())
		if err != nil {
			return err
		}

		repo.InitScore()

		if ownerConfig.Sync != nil {
			repo.Sync = *ownerConfig.Sync
		}
		if ownerConfig.Pin != nil {
			repo.Pin = *ownerConfig.Pin
		}

		err = ctx.Database.InsertRepo(repo)
		if err != nil {
			return err
		}
	} else {
		updateOpts := repo.UpdateVisitOptions()

		updateOpts.Sync = ownerConfig.Sync
		updateOpts.Pin = ownerConfig.Pin

		err = ctx.Database.UpdateRepo(repo.ID, updateOpts)
		if err != nil {
			return err
		}
	}

	err = repoutils.EnsureCreate(ctx, o.thin)
	if err != nil {
		return err
	}

	err = repoutils.EnsureLanguage(ctx)
	if err != nil {
		return err
	}

	fmt.Println(ctx.GetRepoPath())
	return nil
}
