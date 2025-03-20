package cmd

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/spf13/cobra"
)

func NewDisplay() *cobra.Command {
	var opts displayOptions
	c := &cobra.Command{
		Use:   "display",
		Short: "Display the current repository",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: DirCompletion,
	}

	return Build(c, &opts)
}

type displayOptions struct {
	path string
}

func (o *displayOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		path := args[0]
		path, err := filepath.Abs(path)
		if err != nil {
			return fmt.Errorf("failed to get the absolute path: %w", err)
		}

		path = strings.TrimSuffix(path, "/")
		o.path = path
	}
	return nil
}

func (o *displayOptions) Run(ctx *context.Context) error {
	if o.path != "" {
		ctx.WorkDir = o.path
	}
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}

	remoteConfig, err := ctx.GetRemote(repo.Remote)
	if err != nil {
		return err
	}

	format := ctx.Config.DisplayFormat

	output := strings.ReplaceAll(format, "{icon}", remoteConfig.Icon)
	output = strings.ReplaceAll(output, "{remote}", repo.Remote)
	output = strings.ReplaceAll(output, "{owner}", repo.Owner)
	output = strings.ReplaceAll(output, "{name}", repo.Name)

	fmt.Println(output)
	return nil
}
