package cmd

import (
	"fmt"
	"os"

	"github.com/fioncat/roxide/pkg/batch"
	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NewSync() *cobra.Command {
	var opts syncOptions
	c := &cobra.Command{
		Use:   "sync [HEAD] [QUERY]",
		Short: "Sync multiple repositories or current repository (if you are in one)",

		Args: cobra.ArbitraryArgs,

		ValidArgsFunction: BuildCompletion(RepoCompletion),
	}

	c.Flags().BoolVarP(&opts.recursive, "recursive", "r", false, "force to sync multiple repositories")
	c.Flags().BoolVarP(&opts.force, "force", "f", false, "force sync, ignore sync flag")

	return Build(c, &opts)
}

type syncOptions struct {
	args []string

	recursive bool

	force bool
}

func (o *syncOptions) Complete(c *cobra.Command, args []string) error {
	o.args = args
	return nil
}

func (o *syncOptions) Run(ctx *context.Context) error {
	if !o.recursive {
		repo, err := repoutils.GetCurrentRepo(ctx)
		if err != nil {
			return err
		}
		if repo != nil {
			err = ctx.SetRepo(repo)
			if err != nil {
				return err
			}
			result, err := repoutils.Sync(ctx)
			if err != nil {
				return err
			}

			fmt.Fprintln(os.Stderr)
			display := result.Render(false)
			if display == "" {
				fmt.Fprintln(os.Stderr, "No result to display")
				return nil
			}

			fmt.Fprintln(os.Stderr, "Result:")
			fmt.Fprintln(os.Stderr, display)
			return err
		}
	}

	ch := choice.New(ctx, o.args)
	opts := choice.ManyOptions{}
	if !o.force {
		opts.Sync = db.BoolPtr(true)
	}
	list, err := ch.ManyLocal(opts)
	if err != nil {
		return err
	}

	if len(list.Items) == 0 {
		term.PrintInfo("No repo to sync")
		return nil
	}

	items := make([]string, 0, len(list.Items))
	for _, repo := range list.Items {
		items = append(items, repo.String())
	}

	err = term.ConfirmItems(items, "sync", "synchronization", "Repo", "Repos")
	if err != nil {
		return err
	}

	tasks := make([]*syncTask, 0, len(list.Items))
	for _, repo := range list.Items {
		repoCtx, err := ctx.Derive(repo)
		if err != nil {
			return err
		}
		tasks = append(tasks, &syncTask{ctx: repoCtx})
	}

	results, err := batch.Run("Sync", tasks)
	if err != nil {
		return err
	}

	displays := make([]string, 0)
	for _, result := range results {
		display := result.Render(true)
		if display != "" {
			displays = append(displays, display)
		}
	}

	fmt.Fprintln(os.Stderr)
	if len(displays) == 0 {
		fmt.Fprintln(os.Stderr, "No result to display")
		return nil
	}

	for i, display := range displays {
		fmt.Fprintln(os.Stderr, display)
		if i != len(displays)-1 {
			fmt.Fprintln(os.Stderr)
		}
	}

	return nil
}

type syncTask struct {
	ctx *context.Context
}

func (t *syncTask) Name() string {
	return t.ctx.GetRepo().String()
}

func (t *syncTask) Run() (*repoutils.SyncResult, error) {
	return repoutils.Sync(t.ctx)
}
