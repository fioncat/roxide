package open

import (
	"path/filepath"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newTag() *cobra.Command {
	var opts tagOptions

	c := &cobra.Command{
		Use:   "tag",
		Short: "Open the tag in the browser",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.TagCompletion),
	}

	return cmd.Build(c, &opts)
}

type tagOptions struct {
	name string
}

func (o *tagOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.name = args[0]
	}
	return nil
}

func (o *tagOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}

	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	var tag *git.Tag
	if o.name != "" {
		tag, err = git.GetTag(ctx.GetRepoPath(), o.name)
	} else {
		tag, err = repoutils.SelectTag(ctx)
	}
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

	url := apiRepo.WebURL
	url = filepath.Join(url, "tree", tag.Name)
	return term.OpenURL(url)
}
