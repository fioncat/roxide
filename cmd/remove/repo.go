package remove

import (
	"fmt"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newRepo() *cobra.Command {
	var opts repoOptions

	c := &cobra.Command{
		Use:   "repo [HEAD] [QUERY]",
		Short: "Remove one or more repositories",

		Args: cobra.MaximumNArgs(2),

		ValidArgsFunction: cmd.BuildCompletion(cmd.RepoCompletion),
	}

	c.Flags().BoolVarP(&opts.recursive, "recursive", "r", false, "remove multiple repositories")
	c.Flags().BoolVarP(&opts.force, "force", "f", false, "force remove, ignore pin flag")

	return cmd.Build(c, &opts)
}

type repoOptions struct {
	args []string

	recursive bool

	force bool
}

func (o *repoOptions) Complete(c *cobra.Command, args []string) error {
	o.args = args
	return nil
}

func (o *repoOptions) Run(ctx *context.Context) error {
	if o.recursive {
		return o.runMany(ctx)
	} else {
		return o.runOne(ctx)
	}
}

func (o *repoOptions) runMany(ctx *context.Context) error {
	ch := choice.New(ctx, o.args)

	var opts choice.ManyOptions
	if !o.force {
		opts.Sync = db.BoolPtr(false)
	}
	list, err := ch.ManyLocal(opts)
	if err != nil {
		return err
	}
	if len(list.Items) == 0 {
		term.PrintInfo("No repo to remove")
		return nil
	}

	names := make([]string, 0, len(list.Items))
	for _, repo := range list.Items {
		names = append(names, repo.String())
	}

	err = term.ConfirmItems(names, "remove", "removal", "Repo", "Repos")
	if err != nil {
		return err
	}

	for _, repo := range list.Items {
		err = repoutils.Remove(ctx, repo)
		if err != nil {
			return fmt.Errorf("failed to remove repo %q: %w", repo.String(), err)
		}
	}

	return nil
}

func (o *repoOptions) runOne(ctx *context.Context) error {
	ch := choice.New(ctx, o.args)

	repo, err := ch.One(choice.OneOptions{
		Mode:       choice.ModeSelect,
		ForceLocal: true,
	})
	if err != nil {
		return err
	}

	return repoutils.Remove(ctx, repo)
}
