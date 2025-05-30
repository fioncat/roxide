package choice

import (
	"errors"
	"fmt"
	"net/url"
	"strings"

	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/remoteapi"
)

type Mode int

const (
	ModeFuzzy Mode = iota
	ModeSelect
)

const (
	latestQueryLimit   = 5
	latestQueryKeyword = "-"
)

type OneOptions struct {
	Mode Mode

	ForceLocal   bool
	SearchRemote bool
}

func (c *Choice) One(opts OneOptions) (*db.Repository, error) {
	if c.head == "" {
		return c.chooseOne(nil, nil, opts)
	}

	if c.query == "" {
		var parsedUrl *url.URL
		var ssh bool
		var err error
		switch {
		case strings.HasPrefix(c.head, "git@") && strings.HasSuffix(c.head, ".git"):
			ssh = true

		case strings.HasPrefix(c.head, "http://") || strings.HasPrefix(c.head, "https://"):
			parsedUrl, err = url.Parse(c.head)
			if err != nil {
				return nil, fmt.Errorf("failed to parse URL %q: %w", c.head, err)
			}
		}

		switch {
		case parsedUrl != nil:
			return c.oneFromURL(parsedUrl, opts)

		case ssh:
			return c.oneFromSsh(c.head, opts)

		default:
			return c.oneFromHead(opts)
		}
	}

	return c.oneFromOwner(opts)
}

func (c *Choice) oneFromURL(url *url.URL, opts OneOptions) (*db.Repository, error) {
	host := url.Hostname()
	if host == "" {
		return nil, errors.New("invalid URL, host cannot be empty")
	}

	var targetRemote *string
	var gitlab bool
	for _, remoteConfig := range c.ctx.RemoteConfigs {
		if remoteConfig.Clone == "" {
			continue
		}

		remoteHost := remoteConfig.Clone
		if remoteHost != host {
			continue
		}

		// We only support parsing two types of URLs: GitHub and GitLab. For
		// non-GitHub cases, we consider them all as GitLab.
		// TODO: Add support for parsing URLs from more types of remotes.
		if remoteHost != remoteapi.GitHubHost {
			gitlab = true
		}

		targetRemote = &remoteConfig.Name
		break
	}

	if targetRemote == nil {
		return nil, fmt.Errorf("cannot find remote with host %q", host)
	}

	// We use a simple method to parse repository URL:
	//
	// - For GitHub, both owner and name are required, and sub-owners are not
	// supported. Therefore, as long as two path segments are identified, it
	// is considered within a repository. The subsequent path is assumed to be
	// the branch or file path.
	//
	// - For GitLab, the presence of sub-owners complicates direct localization
	// of two segments. The path rule in GitLab is that starting from "-", the
	// subsequent path is the branch or file. Therefore, locating the "-" is
	// sufficient for GitLab.
	parts := make([]string, 0)
	for part := range strings.SplitSeq(url.Path, "/") {
		if part == "" {
			continue
		}
		if gitlab {
			if part == "-" {
				break
			}
			parts = append(parts, part)
			continue
		}

		if len(parts) == 2 {
			break
		}
		parts = append(parts, part)
	}

	// The owner and name are both required for GitHub and GitLab, so the length
	// of `parts` should be bigger than 2.
	// If not, it means that user are not in a repository, maybe in an owner.
	if len(parts) < 2 {
		return nil, fmt.Errorf("invalid URL %q, should be in a repository", url.String())
	}

	path := strings.Join(parts, "/")
	owner, name := ParseOwner(path)

	return c.oneFromID(*targetRemote, owner, name, opts)
}

func (c *Choice) oneFromSsh(ssh string, opts OneOptions) (*db.Repository, error) {
	// Parsing SSH is done in a clever way by reusing the code for parsing
	// URLs. The approach involves converting the SSH statement to a URL and
	// then calling the URL parsing code.
	fullName := strings.TrimPrefix(ssh, "git@")
	fullName = strings.TrimSuffix(fullName, ".git")
	fullName = strings.Replace(fullName, ":", "/", 1)

	fakeUrlRaw := fmt.Sprintf("https://%s", fullName)

	fakeUrl, err := url.Parse(fakeUrlRaw)
	if err != nil {
		return nil, fmt.Errorf("invalid ssh %q, cannot convert to URL", ssh)
	}

	return c.oneFromURL(fakeUrl, opts)
}

func (c *Choice) oneFromHead(opts OneOptions) (*db.Repository, error) {
	// Treating `head` as a remote (with higher priority) or fuzzy matching
	// keyword, we will call different functions from the database to retrieve
	// the information.
	if c.ctx.HasRemote(c.head) || c.head == latestQueryKeyword {
		return c.chooseOne(&c.head, nil, opts)
	}

	return c.fuzzyOne(nil, &c.head)
}

func (c *Choice) oneFromOwner(opts OneOptions) (*db.Repository, error) {
	// Up to this point, with the `query` provided, indicating that `head`
	// represents a remote, we can directly retrieve the remote configuration.
	remoteConfig, err := c.ctx.GetRemote(c.head)
	if err != nil {
		return nil, err
	}

	// A special syntax: If `query` ends with "/", it indicates a selection
	// within the owner.
	if strings.HasSuffix(c.query, "/") {
		owner := strings.TrimSuffix(c.query, "/")
		selectLocal := opts.ForceLocal
		if remoteConfig.API == nil {
			selectLocal = true
		}

		if selectLocal {
			return c.selectOne(&remoteConfig.Name, &owner, false)
		}

		api, err := c.ctx.RemoteAPI(remoteConfig.Name)
		if err != nil {
			return nil, err
		}

		remoteRepos, err := api.ListRepos(owner)
		if err != nil {
			return nil, err
		}

		if opts.SearchRemote {
			repos, err := c.ctx.Database.QueryRepos(db.QueryRepositoryOptions{
				Remote: &remoteConfig.Name,
				Owner:  &owner,
			})
			if err != nil {
				return nil, err
			}

			locals := make(map[string]struct{}, len(repos))
			for _, repo := range repos {
				locals[repo.Name] = struct{}{}
			}

			filtered := make([]string, 0, len(remoteRepos))
			for _, remoteRepo := range remoteRepos {
				if _, ok := locals[remoteRepo]; ok {
					continue
				}
				filtered = append(filtered, remoteRepo)
			}
			remoteRepos = filtered
		}

		if len(remoteRepos) == 0 {
			return nil, fmt.Errorf("no repository in %q", owner)
		}

		idx, err := c.ctx.Selector.Select(remoteRepos)
		if err != nil {
			return nil, err
		}

		name := remoteRepos[idx]
		return c.oneFromID(remoteConfig.Name, owner, name, opts)
	}

	// At this point, there are still two potential branching scenarios:
	//
	// - `query` might be a fuzzy matching keyword or a keyword for searching.
	// This can be determined based on whether `query` contains "/".
	// - If `query` contains "/", the user wants to directly locate or
	// create a repository, and we can directly call the get function.
	owner, name := ParseOwner(c.query)
	if owner == "" {
		return c.chooseOne(&remoteConfig.Name, &name, opts)
	}

	return c.oneFromID(remoteConfig.Name, owner, name, opts)
}

func (c *Choice) oneFromID(remote, owner, name string, opts OneOptions) (*db.Repository, error) {
	id := db.BuildRepoID(remote, owner, name)
	repo, err := c.ctx.Database.GetRepo(id)
	if err != nil {
		if db.IsNotFound(err) {
			if opts.ForceLocal {
				return nil, fmt.Errorf("cannot find repository %q", id)
			}
			repo = &db.Repository{
				ID:     id,
				Remote: remote,
				Owner:  owner,
				Name:   name,

				NewCreated: true,
			}
			return repo, nil
		}

		return nil, err
	}

	return repo, nil
}

func (c *Choice) chooseOne(remote, query *string, opts OneOptions) (*db.Repository, error) {
	var latest bool
	if remote != nil && *remote == latestQueryKeyword {
		remote = nil
		latest = true
	}
	if query != nil && *query == latestQueryKeyword {
		query = nil
		latest = true
	}
	if latest {
		return c.selectOne(remote, query, true)
	}

	switch opts.Mode {
	case ModeSelect:
		return c.selectOne(remote, query, false)
	default:
		return c.fuzzyOne(remote, query)
	}
}

var (
	errFuzzyNotFound  = errors.New("cannot find matched repository")
	errSelectNotFound = errors.New("no repository to select")
)

func (c *Choice) fuzzyOne(remote, nameSearch *string) (*db.Repository, error) {
	query := db.QueryRepositoryOptions{
		Remote:     remote,
		NameSearch: nameSearch,
	}
	query.OrderByScore()

	repos, err := c.ctx.Database.QueryRepos(query)
	if err != nil {
		return nil, err
	}

	var filtered []*db.Repository
	for _, repo := range repos {
		path := repo.GetPath(c.ctx.Config.Workspace)
		if c.ctx.WorkDir == path {
			// If we are currently in the root directory of a repo, the fuzzyOne
			// method should return another repo, so the current repo will be
			// skipped here.
			continue
		}

		if strings.HasPrefix(c.ctx.WorkDir, path) {
			// If we are currently in a subdirectory of a repo, fuzzyOne will
			// directly return this repo.
			return repo, nil
		}

		filtered = append(filtered, repo)
	}

	if len(filtered) == 0 {
		return nil, errFuzzyNotFound
	}

	return filtered[0], nil
}

func (c *Choice) selectOne(remote, owner *string, latest bool) (*db.Repository, error) {
	query := db.QueryRepositoryOptions{
		Remote: remote,
		Owner:  owner,
	}
	if latest {
		query.OrderByVisitTime()
		limit := latestQueryLimit
		query.Limit = &limit
	} else {
		query.OrderByScore()
	}

	repos, err := c.ctx.Database.QueryRepos(query)
	if err != nil {
		return nil, err
	}

	var level db.DisplayRepoLevel
	if remote == nil && owner == nil {
		level = db.DisplayRepoRemote
	} else if remote != nil && owner == nil {
		level = db.DisplayRepoOwner
	} else {
		level = db.DisplayRepoName
	}

	var filtered []*db.Repository
	for _, repo := range repos {
		path := repo.GetPath(c.ctx.Config.Workspace)
		if strings.HasPrefix(c.ctx.WorkDir, path) {
			continue
		}

		filtered = append(filtered, repo)
	}

	if len(filtered) == 0 {
		return nil, errSelectNotFound
	}

	items := make([]string, 0, len(filtered))
	for _, repo := range filtered {
		items = append(items, repo.Display(level))
	}

	idx, err := c.ctx.Selector.Select(items)
	if err != nil {
		return nil, err
	}

	return filtered[idx], nil
}
