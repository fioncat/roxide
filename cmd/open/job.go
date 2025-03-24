package open

import (
	"errors"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newJob() *cobra.Command {
	var opts jobOptions

	c := &cobra.Command{
		Use:   "job",
		Short: "Open job",

		Args: cobra.NoArgs,

		ValidArgsFunction: cmd.NoneCompletion,
	}

	return cmd.Build(c, &opts)
}

type jobOptions struct{}

func (o *jobOptions) Complete(c *cobra.Command, args []string) error {
	return nil
}

func (o *jobOptions) Run(ctx *context.Context) error {
	term.Mute = true
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	job, err := repoutils.SelectActionJob(ctx)
	if err != nil {
		return err
	}

	if job.URL == "" {
		return errors.New("this job has no url to open")
	}

	return term.OpenURL(job.URL)
}
