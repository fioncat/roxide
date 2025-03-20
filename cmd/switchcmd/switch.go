package switchcmd

import "github.com/spf13/cobra"

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "switch",
		Short: "Switch commands",
	}

	c.AddCommand(newBranch())

	return c
}
