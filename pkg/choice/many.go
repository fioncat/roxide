package choice

import (
	"strings"

	"github.com/fioncat/roxide/pkg/db"
)

type ManyOptions struct {
	Sync *bool
	Pin  *bool

	Language string

	Offset int
	Limit  int
}

type RepositoryList struct {
	Items []*db.Repository `json:"items,omitempty"`
	Total int              `json:"total,omitempty"`
}

func (c *Choice) ManyLocal(opts ManyOptions) (*RepositoryList, error) {
	var level db.DisplayRepoLevel
	query := db.QueryRepositoryOptions{}
	query.OrderByScore()

	query.Sync = opts.Sync
	query.Pin = opts.Pin
	if opts.Language != "" {
		query.Language = &opts.Language
	}
	if opts.Limit > 0 {
		query.Limit = &opts.Limit
		query.Offset = &opts.Offset
	}

	switch {
	case c.head == "":
		level = db.DisplayRepoRemote

	case c.query == "":
		// If `head` is the remote name, select all repositories under that
		// remote; otherwise, perform a fuzzy search.
		if c.ctx.HasRemote(c.head) {
			query.Remote = &c.head
		} else {
			query.NameSearch = &c.head
		}
		level = db.DisplayRepoOwner

	default:
		// When selecting multiple repositories, the logic here is similar to
		// selecting one. Adding "/" after the query indicates selecting the entire
		// owner, and not adding it uses fuzzy matching. The difference from
		// selecting one is that there is no search performed here.
		_, err := c.ctx.GetRemote(c.head)
		if err != nil {
			return nil, err
		}

		query.Remote = &c.head

		if strings.HasSuffix(c.query, "/") {
			owner := strings.TrimSuffix(c.query, "/")
			query.Owner = &owner
		} else {
			owner, name := ParseOwner(c.query)
			if owner == "" {
				query.NameSearch = &name
			} else {
				id := db.BuildRepoID(c.head, owner, name)
				query = db.QueryRepositoryOptions{ID: &id}
			}
		}

		level = db.DisplayRepoName
	}

	repos, err := c.ctx.Database.QueryRepos(query)
	if err != nil {
		return nil, err
	}
	for _, repo := range repos {
		repo.DisplayLevel = level
	}
	count, err := c.ctx.Database.CountRepos(query)
	if err != nil {
		return nil, err
	}

	return &RepositoryList{
		Items: repos,
		Total: count,
	}, nil
}
