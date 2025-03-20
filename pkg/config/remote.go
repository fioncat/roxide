package config

import (
	"fmt"
	"os"
	"time"
)

const (
	defaultCacheTime = "24h"
	defaultTimeout   = "5s"
	defaultListLimit = 100
)

type Remote struct {
	Name string `json:"-" toml:"-"`

	Clone string `json:"clone" toml:"clone"`
	Icon  string `json:"icon" toml:"icon"`

	API *RemoteAPI `json:"api" toml:"api"`

	Default *Owner `json:"default" toml:"default"`

	Owners map[string]*Owner `json:"owners" toml:"owners"`
}

type RemoteAPI struct {
	Type RemoteType `json:"type" toml:"type"`

	Token string `json:"token" toml:"token"`

	Timeout string `json:"timeout" toml:"timeout"`

	CacheTime string `json:"cache_time" toml:"cache_time"`

	ListLimit int `json:"list_limit" toml:"list_limit"`

	Host string `json:"host" toml:"host"`
	URL  string `json:"url" toml:"url"`

	TimeoutDuration   time.Duration `json:"-" toml:"-"`
	CacheTimeDuration time.Duration `json:"-" toml:"-"`
}

type RemoteType string

const (
	RemoteTypeGitHub RemoteType = "github"
	RemoteTypeGitLab RemoteType = "gitlab"
)

type Owner struct {
	Sync *bool `json:"sync" toml:"sync"`
	Pin  *bool `json:"pin" toml:"pin"`

	SSH *bool `json:"ssh" toml:"ssh"`

	User  string `json:"user" toml:"user"`
	Email string `json:"email" toml:"email"`

	OnCreate []string `json:"on_create" toml:"on_create"`
}

func (r *Remote) complete(name string) error {
	r.Name = name

	if r.API != nil {
		err := r.API.complete()
		if err != nil {
			return err
		}
	}

	return nil
}

func (a *RemoteAPI) complete() error {
	switch a.Type {
	case RemoteTypeGitHub, RemoteTypeGitLab:
	default:
		return fmt.Errorf("unknown API type: %s", a.Type)
	}

	a.Token = os.ExpandEnv(a.Token)

	if a.CacheTime == "" {
		a.CacheTime = defaultCacheTime
	}
	cacheTime, err := time.ParseDuration(a.CacheTime)
	if err != nil {
		return fmt.Errorf("parse cache time %q: %w", a.CacheTime, err)
	}
	a.CacheTimeDuration = cacheTime

	if a.Timeout == "" {
		a.Timeout = defaultTimeout
	}
	timeout, err := time.ParseDuration(a.Timeout)
	if err != nil {
		return fmt.Errorf("parse timeout %q: %w", a.Timeout, err)
	}
	a.TimeoutDuration = timeout

	if a.ListLimit <= 0 {
		a.ListLimit = defaultListLimit
	}

	return nil
}

func (o *Owner) merge(other *Owner) {
	if other == nil {
		return
	}
	if other.Sync != nil {
		o.Sync = other.Sync
	}
	if other.Pin != nil {
		o.Pin = other.Pin
	}
	if other.SSH != nil {
		o.SSH = other.SSH
	}

	if other.User != "" {
		o.User = other.User
	}
	if other.Email != "" {
		o.Email = other.Email
	}

	if len(other.OnCreate) > 0 {
		o.OnCreate = other.OnCreate
	}
}

func (r *Remote) GetOwnerConfig(owner string) *Owner {
	var config Owner

	if r.Default != nil {
		config = *r.Default
	}

	if cfg, ok := r.Owners[owner]; ok {
		config.merge(cfg)
	}

	return &config
}
