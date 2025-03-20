package remoteapi

import (
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"slices"
	"time"

	"github.com/fioncat/roxide/pkg/db"
	gitlab "gitlab.com/gitlab-org/api/client-go"
)

const (
	GitLabHost    = "gitlab.com"
	GitLabVersion = "v4"
)

type GitLab struct {
	client *gitlab.Client

	isPrivate bool
	hasToken  bool

	url string

	limit int
}

func NewGitLab(host, apiURL, token string, limit int, timeout time.Duration) (RemoteAPI, error) {
	var opts []gitlab.ClientOptionFunc
	if apiURL != "" {
		opts = append(opts, gitlab.WithBaseURL(apiURL))
	} else if host != "" {
		apiURL = fmt.Sprintf("https://%s/api/%s", host, GitLabVersion)
		opts = append(opts, gitlab.WithBaseURL(apiURL))
	}

	isPrivate := false
	if apiURL != "" {
		parsed, _ := url.Parse(apiURL)
		isPrivate = parsed.Host != GitLabHost
	} else {
		apiURL = fmt.Sprintf("https://%s/api/%s", GitLabHost, GitLabVersion)
	}

	httpClient := http.Client{Timeout: timeout}
	opts = append(opts, gitlab.WithHTTPClient(&httpClient))

	client, err := gitlab.NewClient(token, opts...)
	if err != nil {
		return nil, fmt.Errorf("create gitlab client: %w", err)
	}

	return &GitLab{
		client:    client,
		isPrivate: isPrivate,
		hasToken:  token != "",
		url:       apiURL,
		limit:     limit,
	}, nil
}

func (g *GitLab) Info() (*RemoteInfo, error) {
	var authOk bool
	if g.hasToken {
		_, _, err := g.client.Users.CurrentUser()
		authOk = err == nil
	}

	_, err := http.Get(g.url)
	ping := err != nil

	name := fmt.Sprintf("GitLab API %s", GitLabVersion)
	if g.isPrivate {
		name = fmt.Sprintf("%s (private)", name)
	}

	return &RemoteInfo{
		Name:   name,
		Auth:   g.hasToken,
		AuthOk: authOk,
		Ping:   ping,
	}, nil
}

func (g *GitLab) ListRepos(owner string) ([]string, error) {
	projects, _, err := g.client.Groups.ListGroupProjects(owner, &gitlab.ListGroupProjectsOptions{
		ListOptions: gitlab.ListOptions{
			PerPage: g.limit,
		},
	})
	if err != nil {
		return nil, err
	}

	names := make([]string, 0, len(projects))
	for _, project := range projects {
		names = append(names, project.Path)
	}

	return names, nil
}

func (g *GitLab) GetRepo(owner, name string) (*RemoteRepository, error) {
	id := fmt.Sprintf("%s/%s", owner, name)
	project, _, err := g.client.Projects.GetProject(id, &gitlab.GetProjectOptions{})
	if err != nil {
		return nil, err
	}

	return &RemoteRepository{
		DefaultBranch: project.DefaultBranch,
		Upstream:      nil,
		WebURL:        project.WebURL,
	}, nil
}

func (g *GitLab) SearchRepos(query string) ([]string, error) {
	projects, _, err := g.client.Search.Projects(query, &gitlab.SearchOptions{
		ListOptions: gitlab.ListOptions{
			PerPage: g.limit,
		},
	})
	if err != nil {
		return nil, err
	}

	names := make([]string, 0, len(projects))
	for _, project := range projects {
		names = append(names, project.NameWithNamespace)
	}

	return names, nil
}

func (g *GitLab) GetMergeRequest(req *MergeRequest) (string, error) {
	if req.Upstream != nil {
		return "", errors.New("now we don't support upstream for gitlab api")
	}

	id := fmt.Sprintf("%s/%s", req.Owner, req.Name)
	mrs, _, err := g.client.MergeRequests.ListProjectMergeRequests(id, &gitlab.ListProjectMergeRequestsOptions{
		State:        db.StringPtr("opened"),
		SourceBranch: db.StringPtr(req.Source),
		TargetBranch: db.StringPtr(req.Target),
	})
	if err != nil {
		return "", err
	}

	if len(mrs) == 0 {
		return "", nil
	}

	return mrs[0].WebURL, nil
}

func (g *GitLab) CreateMergeRequest(req *MergeRequest, title, body string) (string, error) {
	if req.Upstream != nil {
		return "", errors.New("now we don't support upstream for gitlab api")
	}

	id := fmt.Sprintf("%s/%s", req.Owner, req.Name)
	mr, _, err := g.client.MergeRequests.CreateMergeRequest(id, &gitlab.CreateMergeRequestOptions{
		SourceBranch: db.StringPtr(req.Source),
		TargetBranch: db.StringPtr(req.Target),
		Title:        db.StringPtr(title),
		Description:  db.StringPtr(body),
	})
	if err != nil {
		return "", err
	}

	return mr.WebURL, nil
}

func (g *GitLab) GetAction(req *ActionRequest) (*Action, error) {
	var sha *string
	if req.Commit != "" {
		sha = &req.Commit
	}
	var ref *string
	if req.Branch != "" {
		ref = &req.Branch
	}

	id := fmt.Sprintf("%s/%s", req.Owner, req.Name)
	pipelines, _, err := g.client.Pipelines.ListProjectPipelines(id, &gitlab.ListProjectPipelinesOptions{
		SHA: sha,
		Ref: ref,
		ListOptions: gitlab.ListOptions{
			PerPage: g.limit,
		},
	})
	if err != nil {
		return nil, err
	}

	if len(pipelines) == 0 {
		return nil, nil
	}
	pipeline := pipelines[0]

	rawJobs, _, err := g.client.Jobs.ListPipelineJobs(id, pipeline.ID, &gitlab.ListJobsOptions{
		ListOptions: gitlab.ListOptions{
			PerPage: g.limit,
		},
	})
	if err != nil {
		return nil, err
	}
	slices.Reverse(rawJobs)

	var commit *ActionCommit
	stagesIndex := make(map[string]int, len(rawJobs))
	runs := make([]ActionRun, 0, len(rawJobs))

	for _, rawJob := range rawJobs {
		if rawJob.Commit == nil {
			continue
		}
		if commit == nil {
			commit = &ActionCommit{
				ID:          rawJob.Commit.ID,
				Message:     rawJob.Commit.Title,
				AuthorName:  rawJob.Commit.AuthorName,
				AuthorEmail: rawJob.Commit.AuthorEmail,
			}
		} else {
			if commit.ID != rawJob.Commit.ID {
				continue
			}
		}

		status := g.convertStatus(rawJob.Status)
		job := ActionJob{
			ID:     int64(rawJob.ID),
			Name:   rawJob.Name,
			Status: status,
			URL:    rawJob.WebURL,
		}

		stageIndex, ok := stagesIndex[rawJob.Stage]
		if !ok {
			run := ActionRun{
				Name: rawJob.Stage,
				URL:  "",
				Jobs: []ActionJob{job},
			}
			idx := len(runs)
			runs = append(runs, run)
			stagesIndex[rawJob.Stage] = idx
		} else {
			runs[stageIndex].Jobs = append(runs[stageIndex].Jobs, job)
		}
	}

	if commit == nil {
		return nil, errors.New("commit info from GitHub workflow runs is empty")
	}

	return &Action{
		URL:    pipeline.WebURL,
		Commit: *commit,
		Runs:   runs,
	}, nil
}

func (g *GitLab) GetJob(owner, name string, id int64) (*ActionJob, error) {
	pid := fmt.Sprintf("%s/%s", owner, name)
	job, _, err := g.client.Jobs.GetJob(pid, int(id))
	if err != nil {
		return nil, err
	}

	return &ActionJob{
		ID:     int64(job.ID),
		Name:   job.Name,
		Status: g.convertStatus(job.Status),
		URL:    job.WebURL,
	}, nil
}

func (g *GitLab) JobLogs(owner, name string, id int64) (string, error) {
	pid := fmt.Sprintf("%s/%s", owner, name)
	reader, _, err := g.client.Jobs.GetTraceFile(pid, int(id))
	if err != nil {
		return "", err
	}

	data, err := io.ReadAll(reader)
	if err != nil {
		return "", fmt.Errorf("read job logs: %w", err)
	}

	return string(data), nil
}

func (g *GitLab) convertStatus(status string) ActionJobStatus {
	switch status {
	case "created", "pending", "waiting_for_resource":
		return ActionJobPending
	case "running":
		return ActionJobRunning
	case "failed":
		return ActionJobFailed
	case "success":
		return ActionJobSuccess
	case "canceled":
		return ActionJobCanceled
	case "skipped":
		return ActionJobSkipped
	case "manual":
		return ActionJobWaitingForConfirm
	default:
		return ActionJobFailed
	}
}
