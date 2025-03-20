package context

import (
	"fmt"

	"github.com/fioncat/roxide/pkg/config"
	"github.com/fioncat/roxide/pkg/remoteapi"
)

func (c *Context) HasRemote(remote string) bool {
	cfg, _ := c.GetRemote(remote)
	return cfg != nil
}

func (c *Context) GetRemote(remote string) (*config.Remote, error) {
	for _, remoteConfig := range c.RemoteConfigs {
		if remoteConfig.Name == remote {
			return remoteConfig, nil
		}
	}
	return nil, fmt.Errorf("cannot find remote %q", remote)
}

func (c *Context) RemoteAPI(remote string) (remoteapi.RemoteAPI, error) {
	c.apiLock.Lock()
	defer c.apiLock.Unlock()

	if api, ok := c.apiCache[remote]; ok {
		return api, nil
	}

	remoteConfig, err := c.GetRemote(remote)
	if err != nil {
		return nil, err
	}

	if remoteConfig.API == nil {
		return nil, fmt.Errorf("remote %q has no api config", remote)
	}

	apiConfig := remoteConfig.API
	var api remoteapi.RemoteAPI
	switch apiConfig.Type {
	case config.RemoteTypeGitHub:
		api, err = remoteapi.NewGitHub(apiConfig.Token, apiConfig.ListLimit, apiConfig.TimeoutDuration)
	case config.RemoteTypeGitLab:
		api, err = remoteapi.NewGitLab(apiConfig.Host, apiConfig.URL, apiConfig.Token, apiConfig.ListLimit, apiConfig.TimeoutDuration)
	default:
		return nil, fmt.Errorf("unknown remote type: %s", apiConfig.Type)
	}
	if err != nil {
		return nil, err
	}

	if apiConfig.CacheTimeDuration > 0 {
		api = remoteapi.NewCache(remote, c.Database, api, c.ForceNoCache, apiConfig.CacheTimeDuration)
	}

	c.apiCache[remote] = api
	return api, nil
}
