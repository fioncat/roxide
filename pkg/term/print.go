package term

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/fatih/color"
)

var Mute bool

func PrintInfo(msg string, args ...any) {
	if Mute {
		return
	}
	msg = fmt.Sprintf(msg, args...)
	c := color.New(color.FgGreen, color.Bold)
	fmt.Fprintf(os.Stderr, "%s %s\n", c.Sprint("==>"), msg)
}

func PrintJson(v any) error {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}

	fmt.Println(string(data))
	return nil
}

func CursorUp() {
	const cursorUpChars = "\x1b[A\x1b[K"
	fmt.Fprint(os.Stderr, cursorUpChars)
}
