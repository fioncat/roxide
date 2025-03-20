package cmd

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NewConfig() *cobra.Command {
	var opts configOptions

	c := &cobra.Command{
		Use: "config [REMOTE]",

		Short: "Edit or show the config",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: BuildCompletion(RemoteCompletion),
	}

	c.Flags().BoolVarP(&opts.show, "show", "s", false, "show the config")
	c.Flags().BoolVarP(&opts.show, "edit", "e", false, "edit the config")

	return Build(c, &opts)
}

type configOptions struct {
	remote string

	show bool
}

func (o *configOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.remote = args[0]
	}

	return nil
}

func (o *configOptions) Run(ctx *context.Context) error {
	if o.show {
		if o.remote == "" {
			return term.PrintJson(ctx.Config)
		}

		remoteConfig, err := ctx.GetRemote(o.remote)
		if err != nil {
			return err
		}

		return term.PrintJson(remoteConfig)
	}

	dir := ctx.Config.GetDir()
	var path string
	if o.remote == "" {
		path = filepath.Join(dir, "config.toml")
	} else {
		path = filepath.Join(dir, "remotes", o.remote+".toml")
	}

	editor := os.Getenv("EDITOR")
	if editor == "" {
		return errors.New("EDITOR environment variable is not set")
	}

	cmd := exec.Command(editor, path)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stderr
	cmd.Stderr = os.Stderr

	err := cmd.Run()
	if err != nil {
		return fmt.Errorf("failed to run editor: %w", err)
	}

	return nil
}
