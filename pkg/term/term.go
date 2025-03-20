package term

import (
	"os"

	"golang.org/x/term"
)

func GetWidth() (int, error) {
	width, _, err := term.GetSize(int(os.Stdout.Fd()))
	return width, err
}
