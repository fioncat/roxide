package term

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/fioncat/roxide/pkg/errors"
)

const userCanceledCode = 130

type Selector interface {
	Select(items []string) (int, error)
}

func NewCmdSelector(cmd string) Selector {
	return &cmdSelector{cmd: cmd}
}

type cmdSelector struct {
	cmd string
}

func (s *cmdSelector) Select(items []string) (int, error) {
	var inputBuf bytes.Buffer
	inputBuf.Grow(len(items))
	for _, item := range items {
		inputBuf.WriteString(item + "\n")
	}

	var outputBuf bytes.Buffer
	cmd := exec.Command("sh", "-c", s.cmd)
	cmd.Stdin = &inputBuf
	cmd.Stderr = os.Stderr
	cmd.Stdout = &outputBuf

	err := cmd.Run()
	if err != nil {
		if exitError, ok := err.(*exec.ExitError); ok {
			code := exitError.ExitCode()
			if code == userCanceledCode {
				return 0, errors.ErrSilenceExit
			}
			return 0, fmt.Errorf("select command %q exited with bad code %d", s.cmd, code)
		}

		return 0, fmt.Errorf("select command %q failed: %w", s.cmd, err)
	}

	result := outputBuf.String()
	result = strings.TrimSpace(result)
	for idx, item := range items {
		if item == result {
			return idx, nil
		}
	}

	return 0, fmt.Errorf("select command %q: cannot find %q", s.cmd, result)
}
