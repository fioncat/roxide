package git

import (
	"errors"
	"strings"
)

func EnsureNoUncommittedChanges(path string) error {
	count, err := CountUncommittedChanges(path)
	if err != nil {
		return err
	}

	if count > 0 {
		return errors.New("uncommitted changes found, please commit them first")
	}

	return nil
}

func CountUncommittedChanges(path string) (int, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("Count uncommitted changes")

	lines, err := gitCmd.Lines("status", "-s")
	if err != nil {
		return 0, err
	}

	var count int
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		count++
	}

	return count, nil
}
