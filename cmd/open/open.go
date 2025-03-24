package open

import (
	"fmt"

	"github.com/skratchdot/open-golang/open"
	"github.com/spf13/cobra"
)

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "open",
		Short: "Open commands",
	}

	c.AddCommand(newBranch())
	c.AddCommand(newMerge())
	c.AddCommand(newRepo())
	c.AddCommand(newTag())

	return c
}

func openURL(url string) error {
	err := open.Run(url)
	if err != nil {
		return fmt.Errorf("failed to open url %q: %w", url, err)
	}
	return nil
}
