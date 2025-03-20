package repoutils

import (
	"fmt"
	"strings"

	"github.com/fatih/color"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/term"
)

type SyncResult struct {
	Name string

	Uncommitted int

	Pushed  []string
	Pulled  []string
	Deleted []string

	Conflict []string
	Detached []string
}

func (r *SyncResult) Render(withHeader bool) string {
	fields := make([]string, 0)
	if r.Uncommitted > 0 {
		flag := color.YellowString("*")
		field := fmt.Sprintf("  %s %d dirty", flag, r.Uncommitted)
		fields = append(fields, field)
	}
	if len(r.Pushed) > 0 {
		flag := color.GreenString("↑")
		field := fmt.Sprintf("  %s %s", flag, strings.Join(r.Pushed, ", "))
		fields = append(fields, field)
	}
	if len(r.Pulled) > 0 {
		flag := color.GreenString("↓")
		field := fmt.Sprintf("  %s %s", flag, strings.Join(r.Pulled, ", "))
		fields = append(fields, field)
	}
	if len(r.Deleted) > 0 {
		flag := color.RedString("-")
		field := fmt.Sprintf("  %s %s", flag, strings.Join(r.Deleted, ", "))
		fields = append(fields, field)
	}
	if len(r.Conflict) > 0 {
		flag := color.MagentaString("$")
		field := fmt.Sprintf("  %s %s", flag, strings.Join(r.Conflict, ", "))
		fields = append(fields, field)
	}
	if len(r.Detached) > 0 {
		flag := color.YellowString("?")
		field := fmt.Sprintf("  %s %s", flag, strings.Join(r.Detached, ", "))
		fields = append(fields, field)
	}

	if len(fields) == 0 {
		return ""
	}

	sb := strings.Builder{}
	if withHeader {
		sb.WriteString(fmt.Sprintf("> %s:\n", r.Name))
	}
	sb.WriteString(strings.Join(fields, "\n"))

	return sb.String()
}

func Sync(ctx *context.Context) (*SyncResult, error) {
	err := EnsureCreate(ctx, false)
	if err != nil {
		return nil, fmt.Errorf("failed to ensure repo create: %w", err)
	}

	err = EnsureUserEmail(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to ensure user email: %w", err)
	}

	err = EnsureLanguage(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to ensure language: %w", err)
	}

	err = EnsureGitRemote(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to ensure git remote: %w", err)
	}

	result, err := syncBranches(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to sync branches: %w", err)
	}

	return result, nil
}

type syncBranchTask struct {
	branch string

	push   bool
	pull   bool
	delete bool
}

func syncBranches(ctx *context.Context) (*SyncResult, error) {
	path := ctx.GetRepoPath()
	repo := ctx.GetRepo()
	name := repo.String()

	result := &SyncResult{Name: name}

	remoteConfig := ctx.GetRemoteConfig()
	if remoteConfig.Clone == "" {
		return result, nil
	}

	gitCmd := git.WithPath(path)

	gitCmd.Info("Fetching origin remote")
	err := gitCmd.Run("fetch", "origin", "--prune")
	if err != nil {
		return nil, err
	}

	uncommittedCount, err := git.CountUncommittedChanges(path)
	if err != nil {
		return nil, err
	}
	if uncommittedCount > 0 {
		result.Uncommitted = uncommittedCount
		return result, nil
	}

	branches, err := git.ListBranches(path)
	if err != nil {
		return nil, err
	}

	defaultBranch, err := git.GetDefaultBranch(path)
	if err != nil {
		return nil, err
	}

	back := defaultBranch
	tasks := make([]*syncBranchTask, 0, len(branches))
	var current string
	for _, branch := range branches {
		if branch.Current {
			current = branch.Name
			switch branch.Status {
			case git.BranchStatusGone:
			default:
				back = branch.Name
			}
		}
		var task *syncBranchTask
		switch branch.Status {
		case git.BranchStatusAhead:
			task = &syncBranchTask{
				branch: branch.Name,
				push:   true,
			}
			result.Pushed = append(result.Pushed, branch.Name)

		case git.BranchStatusBehind:
			task = &syncBranchTask{
				branch: branch.Name,
				pull:   true,
			}
			result.Pulled = append(result.Pulled, branch.Name)

		case git.BranchStatusGone:
			if branch.Name == defaultBranch {
				// we cannot delete default branch
				continue
			}
			task = &syncBranchTask{
				branch: branch.Name,
				delete: true,
			}
			result.Deleted = append(result.Deleted, branch.Name)

		case git.BranchStatusConflict:
			result.Conflict = append(result.Conflict, branch.Name)

		case git.BranchStatusDetached:
			result.Detached = append(result.Detached, branch.Name)
		}

		if task != nil {
			tasks = append(tasks, task)
		}
	}

	if len(tasks) == 0 {
		term.PrintInfo("No branch to sync")
		return result, nil
	}

	term.PrintInfo("Backup branch is %s", color.MagentaString(back))

	for _, task := range tasks {
		switch {
		case task.push || task.pull:
			if current != task.branch {
				// checkout to this branch to perform push/pull
				gitCmd.Info("Checkout to branch %q", task.branch)
				err = gitCmd.Run("checkout", task.branch)
				if err != nil {
					return nil, err
				}
				current = task.branch
			}

			var op string
			var title string
			if task.push {
				op = "push"
				title = "Pushing"
			} else {
				op = "pull"
				title = "Pulling"
			}
			gitCmd.Info("%s branch %q", title, task.branch)
			err = gitCmd.Run(op)
			if err != nil {
				return nil, err
			}

		default:
			if current == task.branch {
				// we cannot delete branch when we are inside it, checkout
				// to default branch first.
				gitCmd.Info("Checkout to default branch %q", defaultBranch)
				err = gitCmd.Run("checkout", defaultBranch)
				if err != nil {
					return nil, err
				}
				current = defaultBranch
			}
			gitCmd.Info("Deleting branch %q", task.branch)
			err = gitCmd.Run("branch", "-D", task.branch)
			if err != nil {
				return nil, err
			}
		}
	}

	if current != back {
		gitCmd.Info("Checkout to backup branch %q", back)
		err = gitCmd.Run("checkout", back)
		if err != nil {
			return nil, err
		}
	}

	return result, nil
}
