package term

import (
	"errors"
	"fmt"

	"github.com/skratchdot/open-golang/open"
)

func OpenURL(url string) error {
	if url == "" {
		return errors.New("open url cannot be empty")
	}

	err := open.Run(url)
	if err != nil {
		return fmt.Errorf("failed to open url %q: %w", url, err)
	}
	return nil
}
