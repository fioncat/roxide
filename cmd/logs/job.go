package logs

import (
	"fmt"

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
		Short: "Print logs of a job",

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

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return err
	}

	logs, err := api.JobLogs(repo.Owner, repo.Name, job.ID)
	if err != nil {
		return err
	}

	fmt.Print(logs)
	return nil
}
