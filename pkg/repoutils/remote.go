package repoutils

import (
	"fmt"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/term"
)

func GetRemote(ctx *context.Context, upstream bool) (*git.Remote, error) {
	if upstream {
		return getUpstreamRemote(ctx)
	}

	return git.NewRemote("origin", ctx.GetRepoPath()), nil
}

func getUpstreamRemote(ctx *context.Context) (*git.Remote, error) {
	remotes, err := git.ListRemotes(ctx.GetRepoPath())
	if err != nil {
		return nil, err
	}

	for _, remote := range remotes {
		if remote.Name == "upstream" {
			return remote, nil
		}
	}

	repo := ctx.GetRepo()
	remoteConfig := ctx.GetRemoteConfig()

	term.PrintInfo("Get upstream for %q", repo.String())
	api, err := ctx.RemoteAPI(remoteConfig.Name)
	if err != nil {
		return nil, err
	}

	apiRepo, err := api.GetRepo(repo.Owner, repo.Name)
	if err != nil {
		return nil, err
	}

	if apiRepo.Upstream == nil {
		return nil, fmt.Errorf("repo %q is not forked and without an upstream", repo.String())
	}

	upstream := apiRepo.Upstream

	upstreamOwnerConfig := remoteConfig.GetOwnerConfig(upstream.Owner)
	upstreamURL := getCloneRaw(upstream.Owner, upstream.Name, remoteConfig, upstreamOwnerConfig)

	err = term.Confirm("Do you want to set upstream to %q: %q",
		fmt.Sprintf("%s/%s", upstream.Owner, upstream.Name), upstreamURL)
	if err != nil {
		return nil, err
	}

	gitCmd := git.WithPath(ctx.GetRepoPath())
	gitCmd.Info("Set upstream remote to %q", upstreamURL)
	err = gitCmd.Run("remote", "add", "upstream", upstreamURL)
	if err != nil {
		return nil, err
	}

	return git.NewRemote("upstream", ctx.GetRepoPath()), nil
}
