package cmd

import (
	"errors"
	"fmt"
	"os"

	"github.com/fioncat/roxide/hack"
	"github.com/spf13/cobra"
)

func NewInit() *cobra.Command {
	var shell string
	var name string
	var binary string

	c := &cobra.Command{
		Use:   "init SHELL",
		Short: "Print init script, you should source this in the profile",
		Args:  cobra.ExactArgs(1),

		RunE: func(c *cobra.Command, args []string) error {
			if name == "" {
				return errors.New("name cannot be empty")
			}
			shell = args[0]
			if shell == "" {
				return errors.New("shell type cannot be empty")
			}

			root := c.Root()
			root.Use = name

			fmt.Println(hack.GetWrap(name, binary))

			switch shell {
			case "bash", "sh":
				return root.GenBashCompletionV2(os.Stdout, true)

			case "zsh":
				return root.GenZshCompletion(os.Stdout)

			case "fish":
				return root.GenFishCompletion(os.Stdout, true)

			default:
				return fmt.Errorf("unknown shell type: %q", shell)
			}
		},
	}

	c.Flags().StringVarP(&name, "name", "n", "rox", "The command name")
	c.Flags().StringVarP(&binary, "binary", "b", "roxide", "The wrap binary name")

	return c
}
