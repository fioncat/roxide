package db

import (
	"errors"
	"fmt"
)

var ErrRemoteCacheRepoNotFound = errors.New("remote_cache_repo not found")

type RemoteCacheRepo struct {
	ID string

	DefaultBranch string

	WebURL string

	UpstreamOwner         *string
	UpstreamName          *string
	UpstreamDefaultBranch *string

	ExpireTime uint64
}

const createRemoteCacheRepoTable = `
CREATE TABLE IF NOT EXISTS remote_cache_repo (
	id TEXT PRIMARY KEY,
	default_branch TEXT NOT NULL,
	web_url TEXT NOT NULL,
	upstream_owner TEXT,
	upstream_name TEXT,
	upstream_default_branch TEXT,
	expire_time INTEGER NOT NULL
);
`

const insertRemoteCacheRepoSql = `
INSERT INTO remote_cache_repo (
	id,
	default_branch,
	web_url,
	upstream_owner,
	upstream_name,
	upstream_default_branch,
	expire_time
) VALUES (
	?, ?, ?, ?, ?, ?, ?
);
`

func (d *Database) InsertRemoteCacheRepo(cache *RemoteCacheRepo) error {
	_, err := d.db.Exec(
		insertRemoteCacheRepoSql,
		cache.ID,
		cache.DefaultBranch,
		cache.WebURL,
		cache.UpstreamOwner,
		cache.UpstreamName,
		cache.UpstreamDefaultBranch,
		cache.ExpireTime,
	)
	if err != nil {
		return fmt.Errorf("failed to insert remote_cache_repo: %w", err)
	}

	return nil
}

func (d *Database) GetRemoteCacheRepo(id string) (*RemoteCacheRepo, error) {
	query := newQuerySql("remote_cache_repo",
		"id",
		"default_branch",
		"web_url",
		"upstream_owner",
		"upstream_name",
		"upstream_default_branch",
		"expire_time")

	query.where("id", id)

	sql, values := query.build()

	rows, err := d.db.Query(sql, values...)
	if err != nil {
		return nil, fmt.Errorf("failed to get remote_cache_repo: %w", err)
	}
	defer rows.Close()

	if rows.Next() {
		var cache RemoteCacheRepo
		err = rows.Scan(
			&cache.ID,
			&cache.DefaultBranch,
			&cache.WebURL,
			&cache.UpstreamOwner,
			&cache.UpstreamName,
			&cache.UpstreamDefaultBranch,
			&cache.ExpireTime)
		if err != nil {
			return nil, fmt.Errorf("failed to scan remote_cache_repo: %w", err)
		}

		return &cache, nil
	}

	return nil, ErrRemoteCacheRepoNotFound
}

func (d *Database) DeleteRemoteCacheRepo(id string) error {
	sql := `DELETE FROM remote_cache_repo WHERE id = ?`
	result, err := d.db.Exec(sql, id)
	if err != nil {
		return fmt.Errorf("failed to delete remote_cache_repo: %w", err)
	}

	affected, err := result.RowsAffected()
	if err != nil {
		return err
	}
	if affected == 0 {
		return ErrRemoteCacheRepoNotFound
	}

	return nil
}
