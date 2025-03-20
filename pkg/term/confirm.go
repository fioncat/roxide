package term

import (
	"fmt"
	"os"
	"strings"

	"github.com/fatih/color"
	"github.com/fioncat/roxide/pkg/errors"
)

func Confirm(msg string, args ...any) error {
	noConfirm := os.Getenv("ROXIDE_NOCONFIRM")
	if noConfirm == "true" {
		return nil
	}

	msg = fmt.Sprintf(msg, args...)
	c := color.New(color.Bold)
	msg = c.Sprintf(":: %s", msg)
	fmt.Fprintf(os.Stderr, "%s? [Y/n] ", msg)

	var resp string
	fmt.Scanf("%s", &resp)

	if strings.ToLower(resp) != "y" {
		return errors.ErrSilenceExit
	}

	return nil
}

func ConfirmItems(items []string, action, noun, name, plural string) error {
	if len(items) == 0 {
		return fmt.Errorf("nothing to %s", action)
	}

	PrintInfo("Require confirm to %s", action)
	fmt.Fprintln(os.Stderr)

	maxWidth, err := GetWidth()
	if err != nil {
		return err
	}

	if len(items) > 1 {
		name = plural
	}

	head := fmt.Sprintf("%s (%d): ", name, len(items))
	headWidth := len(head)
	headSpace := strings.Repeat(" ", headWidth)

	currentWidth := 0
	for idx, item := range items {
		itemWidth := len(item)
		if currentWidth == 0 {
			if idx == 0 {
				fmt.Fprintf(os.Stderr, "%s%s", head, item)
			} else {
				fmt.Fprintf(os.Stderr, "%s%s", headSpace, item)
			}
			currentWidth = headWidth + itemWidth
			continue
		}

		currentWidth += 2 + itemWidth
		if currentWidth > maxWidth {
			fmt.Fprintln(os.Stderr)
			fmt.Fprintf(os.Stderr, "%s%s", headSpace, item)
			currentWidth = headWidth + itemWidth
			continue
		}

		fmt.Fprintf(os.Stderr, "  %s", item)
	}

	fmt.Fprintln(os.Stderr)
	fmt.Fprintln(os.Stderr)

	fmt.Fprintf(os.Stderr, "Total %d %s to %s\n", len(items), strings.ToLower(name), action)
	fmt.Fprintln(os.Stderr)

	err = Confirm("Proceed with %s", noun)
	if err != nil {
		return err
	}

	fmt.Fprintln(os.Stderr)
	return nil
}
