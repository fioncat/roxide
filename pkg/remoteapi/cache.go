package remoteapi

import (
	"fmt"
	"strings"
	"time"

	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/timeutils"
)

type Cache struct {
	remoteName string

	db *db.Database

	upstream RemoteAPI

	force bool

	expire time.Duration

	listReposHit uint64
	getRepoHit   uint64
}

func NewCache(name string, db *db.Database, upstream RemoteAPI, force bool, expire time.Duration) *Cache {
	return &Cache{
		remoteName: name,
		db:         db,
		upstream:   upstream,
		force:      force,
		expire:     expire,
	}
}

func (c *Cache) Info() (*RemoteInfo, error) {
	info, err := c.upstream.Info()
	if err != nil {
		return nil, err
	}
	info.Cache = true
	return info, nil
}

func (c *Cache) ListRepos(owner string) ([]string, error) {
	now := timeutils.Now()
	id := fmt.Sprintf("%s_%s", c.remoteName, owner)

	cache, err := c.db.GetRemoteCacheList(id)
	if err != nil && !db.IsNotFound(err) {
		return nil, err
	}

	if cache != nil {
		if !c.force && now < cache.ExpireTime {
			c.listReposHit += 1
			repos := strings.Split(cache.Repos, ",")
			return repos, nil
		}

		err = c.db.DeleteRemoteCacheList(id)
		if err != nil {
			return nil, err
		}
	}

	repos, err := c.upstream.ListRepos(owner)
	if err != nil {
		return nil, err
	}

	cache = &db.RemoteCacheList{
		ID:         id,
		Repos:      strings.Join(repos, ","),
		ExpireTime: now + uint64(c.expire.Seconds()),
	}
	err = c.db.InsertRemoteCacheList(cache)
	return repos, err
}

func (c *Cache) GetRepo(owner string, name string) (*RemoteRepository, error) {
	now := timeutils.Now()
	id := fmt.Sprintf("%s_%s_%s", c.remoteName, owner, name)

	cache, err := c.db.GetRemoteCacheRepo(id)
	if err != nil && !db.IsNotFound(err) {
		return nil, err
	}

	if cache != nil {
		if !c.force && now < cache.ExpireTime {
			c.getRepoHit += 1
			var upstream *RemoteUpstream
			if cache.UpstreamOwner != nil && cache.UpstreamName != nil && cache.UpstreamDefaultBranch != nil {
				upstream = &RemoteUpstream{
					Owner: *cache.UpstreamOwner,
					Name:  *cache.UpstreamName,

					DefaultBranch: *cache.UpstreamDefaultBranch,
				}
			}

			return &RemoteRepository{
				DefaultBranch: cache.DefaultBranch,
				Upstream:      upstream,
				WebURL:        cache.WebURL,
			}, nil
		}

		err = c.db.DeleteRemoteCacheRepo(id)
		if err != nil {
			return nil, err
		}
	}

	repo, err := c.upstream.GetRepo(owner, name)
	if err != nil {
		return nil, err
	}

	var upstreamOwner *string
	var upstreamName *string
	var upstreamDefaultBranch *string
	if repo.Upstream != nil {
		upstreamOwner = &repo.Upstream.Owner
		upstreamName = &repo.Upstream.Name
		upstreamDefaultBranch = &repo.Upstream.DefaultBranch
	}

	cache = &db.RemoteCacheRepo{
		ID: id,

		DefaultBranch: repo.DefaultBranch,
		WebURL:        repo.WebURL,

		UpstreamOwner:         upstreamOwner,
		UpstreamName:          upstreamName,
		UpstreamDefaultBranch: upstreamDefaultBranch,

		ExpireTime: now + uint64(c.expire.Seconds()),
	}
	err = c.db.InsertRemoteCacheRepo(cache)
	return repo, err
}

func (c *Cache) SearchRepos(query string) ([]string, error) {
	return c.upstream.SearchRepos(query)
}

func (c *Cache) GetMergeRequest(req *MergeRequest) (string, error) {
	return c.upstream.GetMergeRequest(req)
}

func (c *Cache) CreateMergeRequest(req *MergeRequest, title, body string) (string, error) {
	return c.upstream.CreateMergeRequest(req, title, body)
}

func (c *Cache) GetAction(req *ActionRequest) (*Action, error) {
	return c.upstream.GetAction(req)
}

func (c *Cache) GetJob(owner string, name string, id int64) (*ActionJob, error) {
	return c.upstream.GetJob(owner, name, id)
}

func (c *Cache) JobLogs(owner string, name string, id int64) (string, error) {
	return c.upstream.JobLogs(owner, name, id)
}
