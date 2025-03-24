package remoteapi

import "github.com/fatih/color"

type RemoteInfo struct {
	Name   string
	Auth   bool
	AuthOk bool
	Ping   bool
	Cache  bool
}

type RemoteRepository struct {
	DefaultBranch string

	Upstream *RemoteUpstream

	WebURL string
}

type RemoteUpstream struct {
	Owner string
	Name  string

	DefaultBranch string
}

type MergeRequest struct {
	Owner string
	Name  string

	Upstream *RemoteUpstream

	Source string
	Target string
}

type ActionRequest struct {
	Owner string
	Name  string

	Commit string
	Branch string
}

type Action struct {
	URL string

	Commit ActionCommit

	Runs []ActionRun
}

type ActionCommit struct {
	ID string

	Message string

	AuthorName  string
	AuthorEmail string
}

type ActionRun struct {
	Name string

	URL string

	Jobs []ActionJob
}

type ActionJobStatus int

const (
	ActionJobPending ActionJobStatus = iota
	ActionJobRunning
	ActionJobSuccess
	ActionJobFailed
	ActionJobCanceled
	ActionJobSkipped
	ActionJobWaitingForConfirm
)

func (s ActionJobStatus) IsComplete() bool {
	switch s {
	case ActionJobSuccess, ActionJobCanceled, ActionJobSkipped, ActionJobWaitingForConfirm:
		return true
	}
	return false
}

func (s ActionJobStatus) String() string {
	switch s {
	case ActionJobPending:
		return "pending"
	case ActionJobRunning:
		return "running"
	case ActionJobSuccess:
		return "success"
	case ActionJobFailed:
		return "failed"
	case ActionJobCanceled:
		return "canceled"
	case ActionJobSkipped:
		return "skipped"
	case ActionJobWaitingForConfirm:
		return "manual"
	}
	return ""
}

func (s ActionJobStatus) ColoredString() string {
	switch s {
	case ActionJobPending:
		return color.YellowString("pending")
	case ActionJobRunning:
		return color.CyanString("running")
	case ActionJobSuccess:
		return color.GreenString("success")
	case ActionJobFailed:
		return color.RedString("failed")
	case ActionJobCanceled:
		return color.YellowString("canceled")
	case ActionJobSkipped:
		return color.YellowString("skipped")
	case ActionJobWaitingForConfirm:
		return color.MagentaString("manual")
	}
	return ""
}

type ActionJob struct {
	ID int64

	Name string

	Status ActionJobStatus

	URL string
}

type RemoteAPI interface {
	Info() (*RemoteInfo, error)

	ListRepos(owner string) ([]string, error)
	GetRepo(owner string, name string) (*RemoteRepository, error)
	SearchRepos(query string) ([]string, error)

	GetMergeRequest(req *MergeRequest) (string, error)
	CreateMergeRequest(req *MergeRequest, title, body string) (string, error)

	GetAction(req *ActionRequest) (*Action, error)
	GetJob(owner string, name string, id int64) (*ActionJob, error)
	JobLogs(owner string, name string, id int64) (string, error)
}
