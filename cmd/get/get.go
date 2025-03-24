package get

import (
	"fmt"

	"github.com/fioncat/roxide/pkg/timeutils"
	"github.com/jedib0t/go-pretty/v6/table"
	"github.com/spf13/cobra"
)

func New() *cobra.Command {
	c := &cobra.Command{
		Use:   "get",
		Short: "Get commands",
	}

	c.AddCommand(newAction())
	c.AddCommand(newBranch())
	c.AddCommand(newOwner())
	c.AddCommand(newRepo())
	c.AddCommand(newTag())

	return c
}

type rowObject interface {
	GetFields(now uint64) map[string]any
}

func showTable[T rowObject](titles []string, list []T, total, page, limit int) {
	if limit == 0 || len(list) == 0 || page == 0 {
		fmt.Println("<empty list>")
		return
	}
	totalPage := (total + limit - 1) / limit
	fmt.Printf("Page: %d/%d, Total: %d\n", page, totalPage, total)

	t := table.NewWriter()
	titleRow := make(table.Row, 0, len(titles))
	for _, t := range titles {
		titleRow = append(titleRow, t)
	}
	t.AppendHeader(titleRow)

	now := timeutils.Now()
	for _, item := range list {
		fields := item.GetFields(now)
		row := make(table.Row, 0, len(titles))
		for _, title := range titles {
			value := fields[title]
			row = append(row, value)
		}
		t.AppendRow(row)
	}

	fmt.Println(t.Render())
}

func paginate[T any](items []T, page, limit int) []T {
	offset := limit * (page - 1)

	total := len(items)
	start := offset
	if start < 0 || start > total {
		return make([]T, 0)
	}

	end := offset + limit
	if end < 0 || end > total {
		end = total
	}

	return items[start:end]
}
