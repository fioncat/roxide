package logs

import "github.com/spf13/cobra"

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "logs",
		Short: "Logs commands",
	}

	c.AddCommand(newJob())

	return c
}
