package remoteapi

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
