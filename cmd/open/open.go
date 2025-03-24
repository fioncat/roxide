package open

import "github.com/spf13/cobra"

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "open",
		Short: "Open commands",
	}

	c.AddCommand(newBranch())
	c.AddCommand(newRepo())
	c.AddCommand(newTag())

	return c
}
