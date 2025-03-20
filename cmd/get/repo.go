package get

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newRepo() *cobra.Command {
	var opts repoOptions

	c := &cobra.Command{
		Use: "repo [HEAD] [QUERY]",

		Short: "List repositories",

		Args: cobra.MaximumNArgs(2),

		ValidArgsFunction: cmd.BuildCompletion(cmd.RepoCompletion),
	}

	c.Flags().StringVarP(&opts.language, "language", "l", "", "list repositories with the specified language")
	c.Flags().IntVarP(&opts.page, "page", "p", 1, "the page number")
	c.Flags().IntVarP(&opts.limit, "limit", "", 10, "the number of repositories per page")
	c.Flags().BoolVarP(&opts.json, "json", "", false, "output as json")
	c.Flags().BoolVarP(&opts.size, "size", "s", false, "list and sort repositories with size")

	return cmd.Build(c, &opts)
}

type repoOptions struct {
	args []string

	language string

	page  int
	limit int

	json bool

	size bool
}

func (o *repoOptions) Complete(c *cobra.Command, args []string) error {
	o.args = args
	return nil
}

func (o *repoOptions) Run(ctx *context.Context) error {
	var list *choice.RepositoryList
	var err error
	if o.size {
		list, err = o.getBySize(ctx)
	} else {
		list, err = o.getDefaults(ctx)
	}
	if err != nil {
		return err
	}

	if o.json {
		return term.PrintJson(list)
	}

	titles := []string{
		"Name",
		"Flags",
		"Language",
		"Visited",
		"VisitTime",
		"Score",
	}
	if o.size {
		titles = append(titles, "Size")
	}

	showTable(titles, list.Items, list.Total, o.page, o.limit)
	return nil
}

func (o *repoOptions) getDefaults(ctx *context.Context) (*choice.RepositoryList, error) {
	ch := choice.New(ctx, o.args)

	offset := o.limit * (o.page - 1)
	opts := choice.ManyOptions{
		Language: o.language,
		Offset:   offset,
		Limit:    o.limit,
	}

	list, err := ch.ManyLocal(opts)
	if err != nil {
		return nil, err
	}

	return list, nil
}

func (o *repoOptions) getBySize(ctx *context.Context) (*choice.RepositoryList, error) {
	ch := choice.New(ctx, o.args)

	opts := choice.ManyOptions{
		Language: o.language,
	}

	list, err := ch.ManyLocal(opts)
	if err != nil {
		return nil, err
	}

	for _, repo := range list.Items {
		path := repo.GetPath(ctx.Config.Workspace)
		size, err := getDirSize(path)
		if err != nil {
			return nil, fmt.Errorf("failed to get size of %s: %w", repo.String(), err)
		}
		repo.Size = size
	}

	// Re order by size
	sort.Slice(list.Items, func(i, j int) bool {
		return list.Items[i].Size > list.Items[j].Size
	})

	total := len(list.Items)

	offset := o.limit * (o.page - 1)
	newItems := paginate(list.Items, offset, o.limit)

	return &choice.RepositoryList{
		Items: newItems,
		Total: total,
	}, nil
}

func getDirSize(dir string) (int64, error) {
	stat, err := os.Stat(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil
		}
		return 0, err
	}
	if !stat.IsDir() {
		return 0, fmt.Errorf("%s is not a directory", dir)
	}

	var size int64
	err = filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		size += info.Size()
		return nil
	})
	if err != nil {
		return 0, err
	}
	return size, nil
}

func paginate(repos []*db.Repository, offset int, limit int) []*db.Repository {
	total := len(repos)
	start := offset
	if start < 0 || start > total {
		return make([]*db.Repository, 0)
	}

	end := offset + limit
	if end < 0 || end > total {
		end = total
	}

	return repos[start:end]
}
