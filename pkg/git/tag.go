package git

import (
	"errors"
	"fmt"
	"regexp"
	"strconv"
)

type Tag string

func (t Tag) GetFields(_ uint64) map[string]any {
	return map[string]any{
		"Tag": string(t),
	}
}

func ListTags(path string) ([]Tag, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("List git tags")

	lines, err := gitCmd.Lines(
		"for-each-ref",
		"--sort=-creatordate",
		"refs/tags/",
		"--format=%(refname:short)")
	if err != nil {
		return nil, err
	}

	tags := make([]Tag, 0, len(lines))
	for _, line := range lines {
		tags = append(tags, Tag(line))
	}

	return tags, nil
}

func GetTag(path, name string) (Tag, error) {
	tags, err := ListTags(path)
	if err != nil {
		return "", err
	}

	for _, tag := range tags {
		if string(tag) == name {
			return tag, nil
		}
	}
	return "", fmt.Errorf("tag %q not found", name)
}

func GetLatestTag(path string) (Tag, error) {
	gitCmd := WithPath(path)
	gitCmd.Info("Get latest git tag")

	out, err := gitCmd.Output("describe", "--tags", "--abbrev=0")
	if err != nil {
		return "", err
	}

	if out == "" {
		return "", errors.New("no latest tag")
	}

	return Tag(out), nil
}

var (
	ruleNumberRegex      = regexp.MustCompile(`\d+`)
	rulePlaceholderRegex = regexp.MustCompile(`\{(\d+|%[yYmMdD])(\+)*}`)
)

func (t Tag) ApplyRule(rule string) (Tag, error) {
	matches := ruleNumberRegex.FindAllStringSubmatch(string(t), -1)

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

	return Tag(result), applyErr
}
