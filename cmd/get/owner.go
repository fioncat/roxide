package get

import (
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newOwner() *cobra.Command {
	var opts ownerOptions

	c := &cobra.Command{
		Use:   "owner [REMOTE]",
		Short: "List owners",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.RemoteCompletion),
	}

	c.Flags().IntVarP(&opts.page, "page", "p", 1, "the page number")
	c.Flags().IntVarP(&opts.limit, "limit", "", 10, "the number of owners per page")
	c.Flags().BoolVarP(&opts.json, "json", "", false, "output as json")

	return cmd.Build(c, &opts)
}

type ownerOptions struct {
	remote string

	page  int
	limit int

	json bool
}

type OwnerList struct {
	Items []*db.Owner `json:"items,omitempty"`
	Total int         `json:"total,omitempty"`
}

func (o *ownerOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.remote = args[0]
	}
	return nil
}

func (o *ownerOptions) Run(ctx *context.Context) error {
	offset := o.limit * (o.page - 1)
	var remote *string
	if o.remote != "" {
		remote = &o.remote
	}
	opts := db.QueryOwnerOptions{
		Remote: remote,
		Offset: &offset,
		Limit:  &o.limit,
	}

	owners, err := ctx.Database.QueryOwners(opts)
	if err != nil {
		return err
	}
	total, err := ctx.Database.CountOwners(opts)
	if err != nil {
		return err
	}

	if o.json {
		return term.PrintJson(OwnerList{
			Items: owners,
			Total: total,
		})
	}

	titles := []string{"Name", "Count"}
	showTable(titles, owners, total, o.page, o.limit)
	return nil
}
