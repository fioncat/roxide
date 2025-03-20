package cmd

import (
	"fmt"
	"os"
	"strings"

	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NoneCompletion(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
	return nil, cobra.ShellCompDirectiveNoFileComp
}

func DirCompletion(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
	if len(args) != 0 {
		return nil, cobra.ShellCompDirectiveNoFileComp
	}
	return nil, cobra.ShellCompDirectiveFilterDirs
}

type CompletionResult struct {
	Items   []string
	NoSpace bool
}

type CompletionFunc func(ctx *context.Context, args []string, toComplete string) (*CompletionResult, error)

func BuildCompletion(f CompletionFunc) cobra.CompletionFunc {
	return func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
		ctx, err := context.Load(false)
		if err != nil {
			writeErrorLog(fmt.Errorf("failed to load context: %w", err))
			return nil, cobra.ShellCompDirectiveError
		}

		result, err := f(ctx, args, toComplete)
		if err != nil {
			writeErrorLog(err)
			return nil, cobra.ShellCompDirectiveError
		}

		if result == nil {
			return nil, cobra.ShellCompDirectiveNoFileComp
		}

		if result.NoSpace {
			return result.Items, cobra.ShellCompDirectiveNoSpace
		}

		return result.Items, cobra.ShellCompDirectiveNoFileComp
	}
}

func RepoCompletion(ctx *context.Context, args []string, toComplete string) (*CompletionResult, error) {
	switch len(args) {
	case 0:
		result, err := remoteCompletion(ctx, toComplete)
		if err != nil {
			return nil, err
		}

		if toComplete == "" {
			return result, nil
		}

		var hasRemotePrefix bool
		for _, item := range result.Items {
			if strings.HasPrefix(item, toComplete) {
				hasRemotePrefix = true
				break
			}
		}
		if hasRemotePrefix {
			return result, nil
		}

		repos, err := ctx.Database.QueryRepos(db.QueryRepositoryOptions{})
		if err != nil {
			return nil, err
		}
		nameSet := make(map[string]struct{}, len(repos))
		for _, repo := range repos {
			if !strings.HasPrefix(repo.Name, toComplete) {
				continue
			}
			if _, ok := nameSet[repo.Name]; ok {
				continue
			}
			nameSet[repo.Name] = struct{}{}
			result.Items = append(result.Items, repo.Name)
		}

		return result, nil

	case 1:
		remote := args[0]

		if !strings.Contains(toComplete, "/") {
			return ownerCompletion(ctx, remote)
		}

		owner, _ := choice.ParseOwner(toComplete)
		repos, err := ctx.Database.QueryRepos(db.QueryRepositoryOptions{
			Remote: &remote,
			Owner:  &owner,
		})
		if err != nil {
			return nil, err
		}

		items := make([]string, 0, len(repos))
		for _, repo := range repos {
			item := repo.Display(db.DisplayRepoOwner)
			items = append(items, item)
		}

		return &CompletionResult{Items: items}, nil
	}

	return nil, nil
}

func RemoteCompletion(ctx *context.Context, args []string, toComplete string) (*CompletionResult, error) {
	if len(args) != 0 {
		return nil, nil
	}

	return remoteCompletion(ctx, toComplete)
}

func OwnerCompletion(ctx *context.Context, args []string, toComplete string) (*CompletionResult, error) {
	switch len(args) {
	case 0:
		return remoteCompletion(ctx, toComplete)

	case 1:
		remote := args[0]
		return ownerCompletion(ctx, remote)
	}

	return nil, nil
}

func remoteCompletion(ctx *context.Context, toComplete string) (*CompletionResult, error) {
	var items []string
	for _, remoteConfig := range ctx.RemoteConfigs {
		if !strings.HasPrefix(remoteConfig.Name, toComplete) {
			continue
		}
		items = append(items, remoteConfig.Name)
	}
	return &CompletionResult{Items: items}, nil
}

func ownerCompletion(ctx *context.Context, remote string) (*CompletionResult, error) {
	owners, err := ctx.Database.QueryOwners(db.QueryOwnerOptions{
		Remote: &remote,
	})
	if err != nil {
		return nil, err
	}

	items := make([]string, 0, len(owners))
	for _, owner := range owners {
		item := fmt.Sprintf("%s/", owner.Owner)
		items = append(items, item)
	}

	return &CompletionResult{
		Items:   items,
		NoSpace: true,
	}, nil
}

func BranchCompletion(ctx *context.Context, args []string, toComplete string) (*CompletionResult, error) {
	if len(args) != 0 {
		return nil, nil
	}

	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return nil, err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return nil, err
	}

	term.Mute = true
	branches, err := git.ListBranches(ctx.GetRepoPath())
	if err != nil {
		return nil, err
	}

	items := make([]string, 0, len(branches))
	for _, branch := range branches {
		if branch.Current {
			continue
		}
		status := branch.StatusString()
		item := fmt.Sprintf("%s\t[%s] [%s] %s", branch.Name, status, branch.CommitID, branch.CommitMessage)
		items = append(items, item)
	}

	return &CompletionResult{Items: items}, nil
}

func writeErrorLog(logErr error) {
	file, err := os.OpenFile("/tmp/roxide_completion_error.log", os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		os.Exit(12)
	}
	defer file.Close()

	msg := logErr.Error() + "\n"
	_, err = file.WriteString(msg)
	if err != nil {
		os.Exit(13)
	}
}
