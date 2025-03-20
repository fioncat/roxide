package cmd

import (
	"fmt"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/spf13/cobra"
)

type Options interface {
	Complete(c *cobra.Command, args []string) error
	Run(ctx *context.Context) error
}

func BuildWithForceNoCache(c *cobra.Command, opts Options) *cobra.Command {
	return build(c, opts, true)
}

func Build(c *cobra.Command, opts Options) *cobra.Command {
	return build(c, opts, false)
}

func build(c *cobra.Command, opts Options, withForceFlag bool) *cobra.Command {
	var forceNoCache bool

	c.RunE = func(cmd *cobra.Command, args []string) error {
		err := opts.Complete(cmd, args)
		if err != nil {
			return fmt.Errorf("validate command args: %w", err)
		}

		ctx, err := context.Load(forceNoCache)
		if err != nil {
			return err
		}
		defer ctx.Database.Close()

		return opts.Run(ctx)
	}

	if withForceFlag {
		c.Flags().BoolVarP(&forceNoCache, "force-no-cache", "f", false, "force to not use cache, this is useful when you are sure that server has been updated, and want to refresh the cache data. This is unuseful when the cache is disabled")
	}

	return c
}
