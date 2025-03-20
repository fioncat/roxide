package git

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/fioncat/roxide/pkg/errors"
	"github.com/fioncat/roxide/pkg/term"
)

type Git struct {
	path string

	msg string

	noCapture bool
}

func New() *Git {
	return &Git{}
}

func WithPath(path string) *Git {
	return &Git{path: path}
}

func (g *Git) Info(msg string, args ...any) {
	g.msg = fmt.Sprintf(msg, args...)
}

func (g *Git) NoCapture() {
	g.noCapture = true
}

func (g *Git) Lines(a ...string) ([]string, error) {
	out, err := g.Output(a...)
	if err != nil {
		return nil, err
	}
	var items []string
	for line := range strings.SplitSeq(out, "\n") {
		if line != "" {
			items = append(items, line)
		}
	}
	return items, nil
}

func (g *Git) Run(a ...string) error {
	_, err := g.Output(a...)
	return err
}

func (g *Git) Output(a ...string) (string, error) {
	var args []string
	if g.path != "" {
		args = append(args, "-C", g.path)
	}
	args = append(args, a...)

	var stderr bytes.Buffer
	var stdout bytes.Buffer
	cmd := exec.Command("git", args...)
	if g.noCapture {
		cmd.Stdout = os.Stderr
	} else {
		cmd.Stdout = &stdout
	}
	cmd.Stdin = os.Stdin
	if !term.Mute {
		if g.msg != "" {
			term.PrintInfo(g.msg)
		} else {
			term.PrintInfo("git %s", strings.Join(a, " "))
		}
		cmd.Stderr = os.Stderr
	} else {
		cmd.Stderr = &stderr
	}

	err := cmd.Run()
	if err != nil {
		if !term.Mute {
			return "", errors.ErrSilenceExit
		}

		return "", fmt.Errorf("git command %q failed: %w, stdout: %q, stderr: %q",
			strings.Join(args, " "), err, stdout.String(), stderr.String())
	}

	return stdout.String(), nil
}
