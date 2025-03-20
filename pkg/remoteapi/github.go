package remoteapi

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"sort"
	"time"

	"github.com/google/go-github/v69/github"
	"golang.org/x/oauth2"
)

const GitHubHost = "github.com"

type GitHub struct {
	client *github.Client

	hasToken bool

	limit   int
	timeout time.Duration
}

type pullRequest struct {
	owner string
	name  string

	head       string
	headSearch string

	base string
}

func NewGitHub(token string, limit int, timeout time.Duration) (RemoteAPI, error) {
	var client *github.Client
	if token != "" {
		ctx := context.Background()
		ts := oauth2.StaticTokenSource(
			&oauth2.Token{AccessToken: token},
		)
		tc := oauth2.NewClient(ctx, ts)
		client = github.NewClient(tc)
	} else {
		client = github.NewClient(nil)
	}

	return &GitHub{
		client:   client,
		hasToken: token != "",
		limit:    limit,
		timeout:  timeout,
	}, nil
}

func newPullRequest(mr *MergeRequest) *pullRequest {
	headSearch := fmt.Sprintf("%s/%s:%s", mr.Owner, mr.Name, mr.Source)

	var head string
	var owner string
	var name string
	if mr.Upstream != nil {
		head = fmt.Sprintf("%s:%s", mr.Owner, mr.Source)
		owner = mr.Upstream.Owner
		name = mr.Upstream.Name
	} else {
		head = mr.Source
		owner = mr.Owner
		name = mr.Name
	}

	return &pullRequest{
		owner:      owner,
		name:       name,
		head:       head,
		headSearch: headSearch,
		base:       mr.Target,
	}
}

func (g *GitHub) Info() (*RemoteInfo, error) {
	var authOk bool
	if g.hasToken {
		ctx, cancel := g.newContext()
		defer cancel()

		_, _, err := g.client.Users.Get(ctx, "")
		authOk = err == nil
	}
	_, err := http.Get("https://api.github.com")
	ping := err == nil

	return &RemoteInfo{
		Name:   fmt.Sprintf("GitHub API %s", github.Version),
		Auth:   g.hasToken,
		AuthOk: authOk,
		Ping:   ping,
	}, nil
}

func (g *GitHub) ListRepos(owner string) ([]string, error) {
	opts := &github.RepositoryListByUserOptions{
		ListOptions: github.ListOptions{
			PerPage: g.limit,
		},
	}

	ctx, cancel := g.newContext()
	defer cancel()
	repos, _, err := g.client.Repositories.ListByUser(ctx, owner, opts)
	if err != nil {
		return nil, err
	}

	names := make([]string, 0, len(repos))
	for _, repo := range repos {
		names = append(names, repo.GetName())
	}

	return names, nil
}

func (g *GitHub) GetRepo(owner, name string) (*RemoteRepository, error) {
	ctx, cancel := g.newContext()
	defer cancel()

	repo, _, err := g.client.Repositories.Get(ctx, owner, name)
	if err != nil {
		return nil, err
	}

	defaultBranch := repo.GetDefaultBranch()
	if defaultBranch == "" {
		return nil, fmt.Errorf("missing default branch for %s/%s", owner, name)
	}

	source := repo.GetSource()
	var upstream *RemoteUpstream
	if source != nil {
		name := source.GetName()
		if name == "" {
			return nil, fmt.Errorf("missing name for upstream of %s/%s", owner, name)
		}

		owner := source.GetOwner().GetLogin()
		if owner == "" {
			return nil, fmt.Errorf("missing owner for upstream of %s/%s", owner, name)
		}

		defaultBranch := source.GetDefaultBranch()
		if defaultBranch == "" {
			return nil, fmt.Errorf("missing default branch for upstream of %s/%s", owner, name)
		}

		upstream = &RemoteUpstream{
			Owner:         owner,
			Name:          name,
			DefaultBranch: defaultBranch,
		}
	}

	return &RemoteRepository{
		DefaultBranch: defaultBranch,
		Upstream:      upstream,
		WebURL:        repo.GetHTMLURL(),
	}, nil
}

func (g *GitHub) SearchRepos(query string) ([]string, error) {
	ctx, cancel := g.newContext()
	defer cancel()

	result, _, err := g.client.Search.Repositories(ctx, query, nil)
	if err != nil {
		return nil, err
	}

	names := make([]string, 0, len(result.Repositories))
	for _, repo := range result.Repositories {
		names = append(names, repo.GetFullName())
	}

	return names, nil
}

func (g *GitHub) GetMergeRequest(req *MergeRequest) (string, error) {
	pr := newPullRequest(req)

	ctx, cancel := g.newContext()
	defer cancel()

	prs, _, err := g.client.PullRequests.List(ctx, pr.owner, pr.name, &github.PullRequestListOptions{
		State: "open",
		Head:  pr.headSearch,
		Base:  pr.base,
	})
	if err != nil {
		return "", err
	}

	if len(prs) == 0 {
		return "", nil
	}

	return prs[0].GetHTMLURL(), nil
}

func (g *GitHub) CreateMergeRequest(req *MergeRequest, title, body string) (string, error) {
	pr := newPullRequest(req)

	ctx, cancel := g.newContext()
	defer cancel()

	result, _, err := g.client.PullRequests.Create(ctx, pr.owner, pr.name, &github.NewPullRequest{
		Head:  &pr.head,
		Base:  &pr.base,
		Title: &title,
		Body:  &body,
	})
	if err != nil {
		return "", err
	}

	return result.GetHTMLURL(), nil
}

func (g *GitHub) GetAction(req *ActionRequest) (*Action, error) {
	ctx, cancel := g.newContext()
	defer cancel()

	rawRuns, _, err := g.client.Actions.ListRepositoryWorkflowRuns(ctx, req.Owner, req.Name, &github.ListWorkflowRunsOptions{
		Branch:  req.Branch,
		HeadSHA: req.Commit,
		ListOptions: github.ListOptions{
			PerPage: g.limit,
		},
	})
	if err != nil {
		return nil, err
	}

	var commit *ActionCommit
	var runs []ActionRun
	for _, rawRun := range rawRuns.WorkflowRuns {
		headCommit := rawRun.GetHeadCommit()
		if headCommit == nil {
			continue
		}

		if commit == nil {
			commit = &ActionCommit{
				ID:          headCommit.GetID(),
				Message:     headCommit.GetMessage(),
				AuthorName:  headCommit.GetAuthor().GetName(),
				AuthorEmail: headCommit.GetAuthor().GetEmail(),
			}
		} else {
			if commit.ID != headCommit.GetID() {
				continue
			}
		}

		ctx, cancel = g.newContext()
		defer cancel()

		rawJobs, _, err := g.client.Actions.ListWorkflowJobs(ctx, req.Owner, req.Name, rawRun.GetID(), &github.ListWorkflowJobsOptions{})
		if err != nil {
			return nil, err
		}

		jobs := make([]ActionJob, 0, len(rawJobs.Jobs))
		for _, rawJob := range rawJobs.Jobs {
			status := g.convertJobStatus(rawJob)
			jobs = append(jobs, ActionJob{
				ID:     rawJob.GetID(),
				Name:   rawJob.GetName(),
				Status: status,
				URL:    rawJob.GetHTMLURL(),
			})
		}

		runs = append(runs, ActionRun{
			Name: rawRun.GetName(),
			URL:  rawRun.GetHTMLURL(),
			Jobs: jobs,
		})
	}

	if commit == nil {
		return nil, errors.New("commit info from GitHub workflow runs is empty")
	}

	sort.Slice(runs, func(i, j int) bool {
		return runs[i].Name < runs[j].Name
	})

	return &Action{
		URL:    "",
		Commit: *commit,
		Runs:   runs,
	}, nil
}

func (g *GitHub) GetJob(owner, name string, id int64) (*ActionJob, error) {
	ctx, cancel := g.newContext()
	defer cancel()

	job, _, err := g.client.Actions.GetWorkflowJobByID(ctx, owner, name, id)
	if err != nil {
		return nil, err
	}

	status := g.convertJobStatus(job)
	return &ActionJob{
		ID:     id,
		Name:   job.GetName(),
		Status: status,
		URL:    job.GetHTMLURL(),
	}, nil
}

func (g *GitHub) JobLogs(owner string, name string, id int64) (string, error) {
	ctx, cancel := g.newContext()
	defer cancel()

	_, resp, err := g.client.Actions.GetWorkflowJobLogs(ctx, owner, name, id, 3)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}

	return string(data), nil
}

func (g *GitHub) newContext() (context.Context, context.CancelFunc) {
	ctx, cancel := context.WithTimeout(context.Background(), g.timeout)
	return ctx, cancel
}

func (g *GitHub) convertJobStatus(rawJob *github.WorkflowJob) ActionJobStatus {
	switch rawJob.GetStatus() {
	case "queued", "waiting":
		return ActionJobPending
	case "in_progress":
		return ActionJobRunning
	case "completed":
		switch rawJob.GetConclusion() {
		case "success":
			return ActionJobSuccess
		case "action_required":
			return ActionJobWaitingForConfirm
		case "cancelled":
			return ActionJobCanceled
		case "skipped":
			return ActionJobSkipped
		default:
			return ActionJobFailed
		}
	default:
		return ActionJobFailed
	}
}
