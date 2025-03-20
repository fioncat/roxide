package repoutils

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/term"
)

func Remove(ctx *context.Context, repo *db.Repository) error {
	dir := repo.GetPath(ctx.Config.Workspace)
	err := removeDir(ctx.Config.Workspace, dir)
	if err != nil {
		return err
	}

	term.PrintInfo("Remove repo %q from database", repo.String())
	return ctx.Database.DeleteRepo(repo.ID)
}

func removeDir(root, dir string) error {
	stat, err := os.Stat(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}

	if !stat.IsDir() {
		return fmt.Errorf("%q is not a directory", dir)
	}

	term.PrintInfo("Remove dir %q", dir)
	err = os.RemoveAll(dir)
	if err != nil {
		return err
	}

	dir = filepath.Dir(dir)
	for {
		if dir == root {
			return nil
		}
		ents, err := os.ReadDir(dir)
		if err != nil {
			return err
		}

		if len(ents) > 0 {
			return nil
		}

		term.PrintInfo("Remove empty dir %q", dir)
		err = os.Remove(dir)
		if err != nil {
			return err
		}
		dir = filepath.Dir(dir)
	}
}
