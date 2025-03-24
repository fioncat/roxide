package create

import "github.com/spf13/cobra"

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "create",
		Short: "Create commands",
	}

	c.AddCommand(newBranch())
	c.AddCommand(newTag())

	return c
}
