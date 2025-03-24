package git

import (
	"errors"
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

type Tag struct {
	Name string `json:"name"`

	CommitID      string `json:"commit_id"`
	CommitMessage string `json:"commit_message"`
}

func (t *Tag) GetFields(_ uint64) map[string]any {
	msg := truncateCommitMessage(t.CommitMessage)
	return map[string]any{
		"Tag":      string(t.Name),
		"CommitID": t.CommitID,
		"Commit":   msg,
	}
}

func ListTags(path string) ([]*Tag, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("List git tags")

	lines, err := gitCmd.Lines(
		"for-each-ref",
		"--sort=-creatordate",
		"refs/tags/",
		"--format=%(refname:short) %(objectname:short) %(subject)")
	if err != nil {
		return nil, err
	}

	tags := make([]*Tag, 0, len(lines))
	for _, line := range lines {
		fields := strings.Fields(line)
		if len(fields) < 3 {
			continue
		}
		name := fields[0]
		commitID := fields[1]
		commitMsg := strings.Join(fields[2:], " ")
		tags = append(tags, &Tag{
			Name:          name,
			CommitID:      commitID,
			CommitMessage: commitMsg,
		})
	}

	return tags, nil
}

func GetTag(path, name string) (*Tag, error) {
	tags, err := ListTags(path)
	if err != nil {
		return nil, err
	}

	for _, tag := range tags {
		if tag.Name == name {
			return tag, nil
		}
	}
	return nil, fmt.Errorf("tag %q not found", name)
}

func GetLatestTag(path string) (*Tag, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("Get latest git tag")

	out, err := gitCmd.Output("describe", "--tags", "--abbrev=0")
	if err != nil {
		return nil, err
	}

	if out == "" {
		return nil, errors.New("no latest tag")
	}

	return GetTag(path, out)
}

var (
	ruleNumberRegex      = regexp.MustCompile(`\d+`)
	rulePlaceholderRegex = regexp.MustCompile(`\{(\d+|%[yYmMdD])(\+)*}`)
)

func (t *Tag) ApplyRule(rule string) (string, error) {
	matches := ruleNumberRegex.FindAllStringSubmatch(t.Name, -1)

	nums := make([]int, 0, len(matches))
	for _, match := range matches {
		if len(match) != 1 {
			continue
		}
		num, err := strconv.Atoi(match[0])
		if err != nil {
			continue
		}
		nums = append(nums, num)
	}

	var applyErr error
	result := rulePlaceholderRegex.ReplaceAllStringFunc(rule, func(s string) string {
		if applyErr != nil {
			return ""
		}

		matches := rulePlaceholderRegex.FindStringSubmatch(s)
		if len(matches) != 3 {
			applyErr = fmt.Errorf("invalid placeholder: %q", s)
			return ""
		}

		idxStr := matches[1]
		idx, err := strconv.Atoi(idxStr)
		if err != nil {
			applyErr = fmt.Errorf("invalid index %q in placeholder", idxStr)
			return ""
		}
		if idx < 0 {
			applyErr = fmt.Errorf("index %d in placeholder must be positive", idx)
			return ""
		}
		if idx >= len(nums) {
			applyErr = fmt.Errorf("index %d in placeholder is out of range of tag", idx)
			return ""
		}

		var resultNum int
		num := nums[idx]
		switch matches[2] {
		case "+":
			resultNum = num + 1

		case "":
			resultNum = num

		default:
			applyErr = fmt.Errorf("invalid operator %q in placeholder", matches[2])
			return ""
		}

		return strconv.Itoa(resultNum)
	})

	return result, applyErr
}
