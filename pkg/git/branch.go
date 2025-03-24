package git

import (
	"errors"
	"fmt"
	"regexp"
	"strings"

	"github.com/fioncat/roxide/pkg/term"
)

const (
	headBranchPrefix = "HEAD branch:"

	branchRemotePrefix = "remotes/"
	branchOriginPrefix = "origin/"
)

type BranchStatus int

const (
	BranchStatusSync BranchStatus = iota
	BranchStatusGone
	BranchStatusAhead
	BranchStatusBehind
	BranchStatusConflict
	BranchStatusDetached
)

type Branch struct {
	Name   string       `json:"name"`
	Status BranchStatus `json:"status"`

	Current bool `json:"current"`

	CommitID      string `json:"commit_id"`
	CommitMessage string `json:"commit_message"`
}

func (b *Branch) GetFields(_ uint64) map[string]any {
	name := b.Name
	if b.Current {
		name = fmt.Sprintf("* %s", name)
	}
	status := b.StatusString()

	msg := truncateCommitMessage(b.CommitMessage)

	return map[string]any{
		"Name":     name,
		"Status":   status,
		"CommitID": b.CommitID,
		"Commit":   msg,
	}
}

func (b *Branch) StatusString() string {
	switch b.Status {
	case BranchStatusSync:
		return "sync"
	case BranchStatusGone:
		return "gone"
	case BranchStatusAhead:
		return "ahead"
	case BranchStatusBehind:
		return "behind"
	case BranchStatusConflict:
		return "conflict"
	case BranchStatusDetached:
		return "detached"
	default:
		return "unknown"
	}
}

func ListBranches(path string) ([]*Branch, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("List git branches")

	lines, err := gitCmd.Lines("branch", "-vv")
	if err != nil {
		return nil, err
	}

	branches := make([]*Branch, 0, len(lines))
	for _, line := range lines {
		branch, err := parseBranch(line)
		if err != nil {
			return nil, err
		}
		branches = append(branches, branch)
	}

	return branches, nil
}

func ListRemoteBranches(path string) ([]string, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("List remote branches")

	lines, err := gitCmd.Lines("branch", "-al")
	if err != nil {
		return nil, err
	}

	items := make([]string, 0, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		if !strings.HasPrefix(line, branchRemotePrefix) {
			continue
		}
		line = strings.TrimPrefix(line, branchRemotePrefix)

		if !strings.HasPrefix(line, branchOriginPrefix) {
			continue
		}
		line = strings.TrimPrefix(line, branchOriginPrefix)
		if line == "" {
			continue
		}

		if strings.HasPrefix(line, "HEAD ->") {
			continue
		}

		items = append(items, line)
	}

	return items, nil
}

func GetDefaultBranch(path string) (string, error) {
	return GetRemoteDefaultBranch(path, "origin")
}

func GetRemoteDefaultBranch(path, remote string) (string, error) {
	term.PrintInfo("Get default branch for %q", remote)
	headRef := fmt.Sprintf("refs/remotes/%s/HEAD", remote)
	remoteRef := fmt.Sprintf("refs/remotes/%s/", remote)

	gitCmd := WithPath(path)
	out, err := gitCmd.Output("symbolic-ref", headRef)
	if err == nil && out != "" {
		branch := strings.TrimPrefix(out, remoteRef)
		branch = strings.TrimSpace(branch)
		if branch == "" {
			return "", errors.New("empty default branch")
		}

		return branch, nil
	}

	// If failed, user might not switch to this branch yet, let's
	// use "git remote show <remote>" instead to get default branch.
	lines, err := gitCmd.Lines("remote", "show", remote)
	if err != nil {
		return "", err
	}

	for _, line := range lines {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, headBranchPrefix) {
			line = strings.TrimPrefix(line, headBranchPrefix)
			line = strings.TrimSpace(line)
			if line == "" {
				return "", errors.New("default branch returned by git remote show is empty")
			}
			return line, nil
		}
	}

	return "", errors.New("no default branch returned by git remote show, please check your git command")
}

var ErrNoCurrentBranch = errors.New("no current branch")

func GetCurrentBranch(path string) (string, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("Get current branch")
	out, err := gitCmd.Output("branch", "--show-current")
	if err != nil {
		return "", err
	}
	out = strings.TrimSpace(out)

	if out == "" {
		return "", ErrNoCurrentBranch
	}

	return out, nil
}

var branchRegex = regexp.MustCompile(`^(\*)*[ ]*([^ ]*)[ ]*([^ ]*)[ ]*(\[[^\]]*\])*[ ]*(.*)$`)

func parseBranch(line string) (*Branch, error) {
	branch, err := parseBranchRaw(line)
	if err != nil {
		return nil, fmt.Errorf("invalid branch line %q: %w, please check your git command", line, err)
	}
	return branch, nil
}

func parseBranchRaw(line string) (*Branch, error) {
	// We have 6 captures:
	//   0 -> line itself
	//   1 -> current branch
	//   2 -> branch name
	//   3 -> commit id
	//   4 -> remote description
	//   5 -> commit message
	caps := branchRegex.FindStringSubmatch(line)
	if len(caps) != 6 {
		return nil, errors.New("invalid format")
	}

	current := caps[1] != ""

	name := caps[2]
	if name == "" {
		return nil, errors.New("missing name")
	}

	desc := caps[4]
	var status BranchStatus
	if desc != "" {
		behind := strings.Contains(desc, "behind")
		ahead := strings.Contains(desc, "ahead")

		if strings.Contains(desc, "gone") {
			status = BranchStatusGone
		} else if ahead && behind {
			status = BranchStatusConflict
		} else if ahead {
			status = BranchStatusAhead
		} else if behind {
			status = BranchStatusBehind
		} else {
			status = BranchStatusSync
		}
	} else {
		status = BranchStatusDetached
	}

	return &Branch{
		Name:    name,
		Status:  status,
		Current: current,

		CommitID:      caps[3],
		CommitMessage: caps[5],
	}, nil
}
