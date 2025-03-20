package choice

import (
	"strings"

	"github.com/fioncat/roxide/pkg/context"
)

type Choice struct {
	ctx *context.Context

	head  string
	query string
}

func New(ctx *context.Context, args []string) *Choice {
	var head string
	if len(args) > 0 {
		head = args[0]
	}

	var query string
	if len(args) > 1 {
		query = args[1]
	}

	return &Choice{
		ctx:   ctx,
		head:  head,
		query: query,
	}
}

func ParseOwner(path string) (string, string) {
	items := strings.Split(path, "/")
	groups := make([]string, 0)
	var base string
	for i, item := range items {
		if i == len(items)-1 {
			base = item
		} else {
			groups = append(groups, item)
		}
	}
	return strings.Join(groups, "/"), base
}
