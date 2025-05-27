package repoutils

import (
	"errors"
	"path/filepath"
	"strings"

	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
)

func MustGetCurrentRepo(ctx *context.Context) (*db.Repository, error) {
	repo, err := GetCurrentRepo(ctx)
	if err != nil {
		return nil, err
	}
	if repo == nil {
		return nil, errors.New("you are not in any repository")
	}

	return repo, nil
}

func GetCurrentRepo(ctx *context.Context) (*db.Repository, error) {
	dir := ctx.WorkDir
	for dir != "" {
		repo, err := getCurrentRepo(ctx, dir)
		if err != nil {
			return nil, err
		}
		if repo != nil {
			return repo, nil
		}
		dir = filepath.Dir(dir)
	}
	return nil, nil
}

func getCurrentRepo(ctx *context.Context, dir string) (*db.Repository, error) {
	id := ParseWorkspacePath(ctx, dir)
	if id == "" {
		repos, err := ctx.Database.QueryRepos(db.QueryRepositoryOptions{
			Path: &dir,
		})
		if err != nil {
			return nil, err
		}
		if len(repos) == 0 {
			return nil, nil
		}

		return repos[0], nil
	}

	repo, err := ctx.Database.GetRepo(id)
	if err != nil {
		if db.IsNotFound(err) {
			return nil, nil
		}
		return nil, err
	}

	return repo, nil
}

func ParseWorkspacePath(ctx *context.Context, dir string) string {
	if !strings.HasPrefix(dir, ctx.Config.Workspace) {
		return ""
	}

	path := strings.TrimPrefix(dir, ctx.Config.Workspace)
	path = strings.Trim(path, "/")

	fields := strings.Split(path, "/")
	if len(fields) < 3 {
		return ""
	}

	remote := fields[0]
	path = strings.Join(fields[1:], "/")

	owner, name := choice.ParseOwner(path)
	if owner == "" || name == "" {
		return ""
	}

	return db.BuildRepoID(remote, owner, name)
}
