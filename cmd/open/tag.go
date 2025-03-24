package open

import (
	"errors"
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

	var toOpen string
	if o.name != "" {
		toOpen = o.name
	} else {
		tags, err := git.ListTags(ctx.GetRepoPath())
		if err != nil {
			return err
		}

		if len(tags) == 0 {
			return errors.New("no tag to open")
		}

		if len(tags) == 1 {
			toOpen = string(tags[0])
		} else {
			items := make([]string, 0, len(tags))
			for _, tag := range tags {
				items = append(items, string(tag))
			}

			idx, err := ctx.Selector.Select(items)
			if err != nil {
				return err
			}
			toOpen = items[idx]
		}
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
	url = filepath.Join(url, "tree", toOpen)
	return term.OpenURL(url)
}
