package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

const (
	defaultWorkspace     = "$HOME/dev"
	defaultDataDir       = "$HOME/.local/share/roxide"
	defaultDisplayFormat = "{icon} {owner}/{name}"
	defaultSelectCmd     = "fzf"
)

type Config struct {
	Workspace string `json:"workspace" toml:"workspace"`

	DataDir string `json:"data_dir" toml:"data_dir"`

	DisplayFormat string `json:"display_format" toml:"display_format"`

	SelectCmd string `json:"select_cmd" toml:"select_cmd"`

	dir string `json:"-" toml:"-"`
}

func Load(dir string) (*Config, error) {
	if dir == "" {
		homeDir, err := os.UserHomeDir()
		if err != nil {
			return nil, err
		}
		dir = filepath.Join(homeDir, ".config", "roxide")
	}

	path := filepath.Join(dir, "config.toml")

	var cfg Config
	data, err := os.ReadFile(path)
	if err != nil && !os.IsNotExist(err) {
		return nil, fmt.Errorf("read config file: %w", err)
	}
	if data != nil {
		err = toml.Unmarshal(data, &cfg)
		if err != nil {
			return nil, fmt.Errorf("parse config toml: %w", err)
		}
	}

	err = cfg.complete()
	if err != nil {
		return nil, fmt.Errorf("complete config: %w", err)
	}
	cfg.dir = dir

	return &cfg, nil
}

func (c *Config) complete() error {
	if c.Workspace == "" {
		c.Workspace = defaultWorkspace
	}
	c.Workspace = os.ExpandEnv(c.Workspace)

	err := ensureDir(c.Workspace)
	if err != nil {
		return fmt.Errorf("ensure workspace: %w", err)
	}

	if c.DataDir == "" {
		c.DataDir = defaultDataDir
	}
	c.DataDir = os.ExpandEnv(c.DataDir)

	err = ensureDir(c.DataDir)
	if err != nil {
		return fmt.Errorf("ensure data dir: %w", err)
	}

	if c.DisplayFormat == "" {
		c.DisplayFormat = defaultDisplayFormat
	}

	if c.SelectCmd == "" {
		c.SelectCmd = defaultSelectCmd
	}

	return nil
}

func (c *Config) LoadRemotes() ([]*Remote, error) {
	remotesDir := filepath.Join(c.dir, "remotes")
	err := ensureDir(remotesDir)
	if err != nil {
		return nil, fmt.Errorf("ensure remotes dir: %w", err)
	}

	ents, err := os.ReadDir(remotesDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}

		return nil, fmt.Errorf("read remotes dir: %w", err)
	}

	var remotes []*Remote
	for _, ent := range ents {
		if ent.IsDir() {
			continue
		}

		name := ent.Name()
		if !strings.HasSuffix(name, ".toml") {
			continue
		}

		path := filepath.Join(remotesDir, name)
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("read remote config for %q: %w", name, err)
		}

		var remote Remote
		err = toml.Unmarshal(data, &remote)
		if err != nil {
			return nil, fmt.Errorf("parse remote config toml for %q: %w", name, err)
		}

		name = strings.TrimSuffix(name, ".toml")
		err = remote.complete(name)
		if err != nil {
			return nil, fmt.Errorf("complete remote config for %q: %w", name, err)
		}

		remotes = append(remotes, &remote)
	}

	return remotes, nil
}

func (c *Config) GetDir() string {
	return c.dir
}

func ensureDir(dir string) error {
	err := os.MkdirAll(dir, 0755)
	if err != nil {
		if os.IsExist(err) {
			return nil
		}
		return err
	}
	return nil
}
