package git

import (
	"fmt"
	"strings"
)

const OriginRemoteName = "origin"

type Remote struct {
	Name string
	path string
}

func NewRemote(name, path string) *Remote {
	return &Remote{
		Name: name,
		path: path,
	}
}

func GetOriginRemote(path string) (*Remote, error) {
	remotes, err := ListRemotes(path)
	if err != nil {
		return nil, err
	}

	for _, remote := range remotes {
		if remote.Name == OriginRemoteName {
			return remote, nil
		}
	}

	return nil, nil
}

func ListRemotes(path string) ([]*Remote, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("List git remotes")

	items, err := gitCmd.Lines("remote")
	if err != nil {
		return nil, fmt.Errorf("failed to list git remotes: %w", err)
	}

	remotes := make([]*Remote, 0, len(items))
	for _, item := range items {
		remotes = append(remotes, &Remote{
			Name: item,
			path: path,
		})
	}

	return remotes, nil
}

func (r *Remote) GetURL() (string, error) {
	gitCmd := WithPath(r.path)
	gitCmd.Info("Get remote %s URL", r.Name)

	url, err := gitCmd.Output("remote", "get-url", r.Name)
	if err != nil {
		return "", fmt.Errorf("failed to get remote %s URL: %w", r.Name, err)
	}

	return url, nil
}

func (r *Remote) GetTarget(branch string) (string, error) {
	if branch == "" {
		defaultBranch, err := GetRemoteDefaultBranch(r.path, r.Name)
		if err != nil {
			return "", err
		}
		branch = defaultBranch
	}

	target := fmt.Sprintf("%s/%s", r.Name, branch)
	gitCmd := WithPath(r.path)
	gitCmd.Info("Fetching target %q", target)
	err := gitCmd.Run("fetch", r.Name, branch)
	return target, err
}

func (r *Remote) CommitsBetween(branch string) ([]string, error) {
	target, err := r.GetTarget(branch)
	if err != nil {
		return nil, err
	}

	compare := fmt.Sprintf("HEAD...%s", target)
	gitCmd := WithPath(r.path)
	lines, err := gitCmd.Lines(
		"log",
		"--left-right",
		"--cherry-pick",
		"--oneline",
		compare)
	if err != nil {
		return nil, err
	}

	commits := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "<") {
			// If the commit message output by "git log xxx" does not start
			// with "<", it means that this commit is from the target branch.
			// Since we only list commits from current branch, ignore such
			// commits.
			continue
		}
		line = strings.TrimPrefix(line, "<")
		line = strings.TrimSpace(line)

		commits = append(commits, line)
	}

	return commits, nil
}
