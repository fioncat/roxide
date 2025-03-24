package repoutils

import (
	"errors"
	"fmt"

	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/remoteapi"
)

func GetActionRequest(ctx *context.Context) (*remoteapi.ActionRequest, error) {
	repo := ctx.GetRepo()

	gitCmd := git.WithPath(ctx.GetRepoPath())
	commit, err := gitCmd.Output("rev-parse", "HEAD")
	if err != nil {
		return nil, err
	}

	return &remoteapi.ActionRequest{
		Owner:  repo.Owner,
		Name:   repo.Name,
		Commit: commit,
	}, nil
}

func SelectActionJob(ctx *context.Context) (*remoteapi.ActionJob, error) {
	repo := ctx.GetRepo()

	req, err := GetActionRequest(ctx)
	if err != nil {
		return nil, err
	}

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return nil, err
	}

	action, err := api.GetAction(req)
	if err != nil {
		return nil, err
	}

	if action == nil {
		return nil, errors.New("no action found")
	}

	items := make([]string, 0, len(action.Runs))
	jobs := make([]*remoteapi.ActionJob, 0, len(action.Runs))
	for _, run := range action.Runs {
		for _, job := range run.Jobs {
			item := fmt.Sprintf("%s/%s, %s", run.Name, job.Name, job.Status.String())
			items = append(items, item)
			jobs = append(jobs, &job)
		}
	}
	if len(jobs) == 0 {
		return nil, errors.New("no job found")
	}

	idx, err := ctx.Selector.Select(items)
	if err != nil {
		return nil, err
	}

	return jobs[idx], nil
}
