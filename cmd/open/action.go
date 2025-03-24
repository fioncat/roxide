package open

import (
	"errors"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newAction() *cobra.Command {
	var opts actionOptions

	c := &cobra.Command{
		Use:   "action",
		Short: "Open action",

		Args: cobra.NoArgs,

		ValidArgsFunction: cmd.NoneCompletion,
	}

	return cmd.Build(c, &opts)
}

type actionOptions struct{}

func (o *actionOptions) Complete(c *cobra.Command, args []string) error {
	return nil
}

func (o *actionOptions) Run(ctx *context.Context) error {
	term.Mute = true
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	req, err := repoutils.GetActionRequest(ctx)
	if err != nil {
		return err
	}

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return err
	}

	action, err := api.GetAction(req)
	if err != nil {
		return err
	}

	if action == nil {
		return errors.New("no action found")
	}

	if action.URL == "" {
		return errors.New("this action has no url to open")
	}

	return term.OpenURL(action.URL)
}
