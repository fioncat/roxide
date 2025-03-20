package remove

import "github.com/spf13/cobra"

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "remove",
		Short: "Remove commands",
	}

	c.AddCommand(newBranch())
	c.AddCommand(newRepo())

	return c
}
