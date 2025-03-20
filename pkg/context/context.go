package context

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sync"

	"github.com/fioncat/roxide/pkg/config"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/remoteapi"
	"github.com/fioncat/roxide/pkg/term"
)

const ConfigEnvName = "ROXIDE_CONFIG"

type Context struct {
	rawContext context.Context

	Config *config.Config

	RemoteConfigs []*config.Remote

	WorkDir string

	Database *db.Database

	Selector term.Selector

	ForceNoCache bool

	apiLock  sync.Mutex
	apiCache map[string]remoteapi.RemoteAPI
}

func Load(forceNoCache bool) (*Context, error) {
	configPath := os.Getenv(ConfigEnvName)

	config, err := config.Load(configPath)
	if err != nil {
		return nil, err
	}

	remoteConfigs, err := config.LoadRemotes()
	if err != nil {
		return nil, err
	}

	dbPath := filepath.Join(config.DataDir, "sqlite.db")
	sqliteDb, err := db.Open(dbPath)
	if err != nil {
		return nil, err
	}

	selector := term.NewCmdSelector(config.SelectCmd)

	workDir, err := os.Getwd()
	if err != nil {
		return nil, fmt.Errorf("get work dir: %w", err)
	}

	return &Context{
		rawContext:    context.Background(),
		Config:        config,
		RemoteConfigs: remoteConfigs,
		WorkDir:       workDir,
		Database:      sqliteDb,
		Selector:      selector,
		ForceNoCache:  forceNoCache,
		apiCache:      make(map[string]remoteapi.RemoteAPI),
	}, nil
}

type contextKey string

const (
	repoContextKey         contextKey = "repo"
	remoteConfigContextKey contextKey = "remoteConfig"
	ownerConfigContextKey  contextKey = "ownerConfig"
	pathContextKey         contextKey = "path"
)

func (c *Context) SetRepo(repo *db.Repository) error {
	path := repo.GetPath(c.Config.Workspace)
	remoteConfig, err := c.GetRemote(repo.Remote)
	if err != nil {
		return err
	}

	ownerConfig := remoteConfig.GetOwnerConfig(repo.Owner)

	c.rawContext = context.WithValue(c.rawContext, repoContextKey, repo)
	c.rawContext = context.WithValue(c.rawContext, remoteConfigContextKey, remoteConfig)
	c.rawContext = context.WithValue(c.rawContext, ownerConfigContextKey, ownerConfig)
	c.rawContext = context.WithValue(c.rawContext, pathContextKey, path)

	return nil
}

func (c *Context) GetRepo() *db.Repository {
	return c.rawContext.Value(repoContextKey).(*db.Repository)
}

func (c *Context) GetRemoteConfig() *config.Remote {
	return c.rawContext.Value(remoteConfigContextKey).(*config.Remote)
}

func (c *Context) GetOwnerConfig() *config.Owner {
	return c.rawContext.Value(ownerConfigContextKey).(*config.Owner)
}

func (c *Context) GetRepoPath() string {
	return c.rawContext.Value(pathContextKey).(string)
}

func (c *Context) Derive(repo *db.Repository) (*Context, error) {
	newCtx := &Context{
		rawContext:    context.Background(),
		Config:        c.Config,
		RemoteConfigs: c.RemoteConfigs,
		WorkDir:       c.WorkDir,
		Database:      c.Database,
		Selector:      c.Selector,
		ForceNoCache:  c.ForceNoCache,
		apiCache:      c.apiCache,
	}
	err := newCtx.SetRepo(repo)
	return newCtx, err
}
