package open

import "github.com/spf13/cobra"

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "open",
		Short: "Open commands",
	}

	c.AddCommand(newAction())
	c.AddCommand(newBranch())
	c.AddCommand(newJob())
	c.AddCommand(newRepo())
	c.AddCommand(newTag())

	return c
}
