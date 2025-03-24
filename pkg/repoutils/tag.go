package repoutils

import (
	"errors"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
)

func SelectTag(ctx *context.Context) (*git.Tag, error) {
	tags, err := git.ListTags(ctx.GetRepoPath())
	if err != nil {
		return nil, err
	}

	if len(tags) == 0 {
		return nil, errors.New("no tag to select")
	}

	if len(tags) == 1 {
		return tags[0], nil
	}

	items := make([]string, 0, len(tags))
	for _, tag := range tags {
		items = append(items, tag.Name)
	}

	idx, err := ctx.Selector.Select(items)
	if err != nil {
		return nil, err
	}

	return tags[idx], nil
}
