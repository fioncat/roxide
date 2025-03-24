package term

import (
	"errors"

	"github.com/manifoldco/promptui"
)

func Input(hint, defaultValue string) (string, error) {
	namePrompt := promptui.Prompt{
		Label:     hint,
		Default:   defaultValue,
		AllowEdit: true,
	}

	input, err := namePrompt.Run()
	if err != nil {
		return "", err
	}

	if input == "" {
		return "", errors.New("input is empty")
	}

	return input, nil
}
