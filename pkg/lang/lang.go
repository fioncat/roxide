package lang

import (
	"os"
	"path/filepath"
)

type Rule struct {
	language string
	dir      bool
	paths    []string
}

var rules = []Rule{
	{
		language: "go",
		paths:    []string{"go.mod"},
	},
	{
		language: "rust",
		paths:    []string{"Cargo.toml"},
	},
}

func Detect(dir string) (*string, error) {
	for _, rule := range rules {
		var fit bool
		for _, path := range rule.paths {
			fullPath := filepath.Join(dir, path)
			stat, err := os.Stat(fullPath)
			if err != nil {
				if os.IsNotExist(err) {
					fit = false
					break
				}
				return nil, err
			}

			switch {
			case rule.dir && stat.IsDir():
				fit = true

			case !rule.dir && !stat.IsDir():
				fit = true

			default:
				fit = false
			}

			if !fit {
				break
			}
		}

		if fit {
			return &rule.language, nil
		}
	}

	return nil, nil
}
