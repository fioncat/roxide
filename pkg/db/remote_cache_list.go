package db

import (
	"errors"
	"fmt"
)

var ErrRemoteCacheListNotFound = errors.New("remote_cache_list not found")

type RemoteCacheList struct {
	ID string

	Repos      string
	ExpireTime uint64
}

const createRemoteCacheListTable = `
CREATE TABLE IF NOT EXISTS remote_cache_list (
	id TEXT PRIMARY KEY,
	repos TEXT NOT NULL,
	expire_time INTEGER NOT NULL
);
`

const insertRemoteCacheListSql = `
INSERT INTO remote_cache_list ( id, repos, expire_time ) VALUES ( ?, ?, ? );
`

func (d *Database) InsertRemoteCacheList(cache *RemoteCacheList) error {
	_, err := d.db.Exec(insertRemoteCacheListSql, cache.ID, cache.Repos, cache.ExpireTime)
	if err != nil {
		return fmt.Errorf("failed to insert remote_cache_list: %w", err)
	}

	return nil
}

func (d *Database) GetRemoteCacheList(id string) (*RemoteCacheList, error) {
	query := newQuerySql("remote_cache_list", "id", "repos", "expire_time")

	query.where("id", id)

	sql, values := query.build()

	rows, err := d.db.Query(sql, values...)
	if err != nil {
		return nil, fmt.Errorf("failed to get remote_cache_list: %w", err)
	}
	defer rows.Close()

	if rows.Next() {
		var cache RemoteCacheList
		err = rows.Scan(&cache.ID, &cache.Repos, &cache.ExpireTime)
		if err != nil {
			return nil, fmt.Errorf("failed to scan remote_cache_list: %w", err)
		}

		return &cache, nil
	}

	return nil, ErrRemoteCacheListNotFound
}

func (d *Database) DeleteRemoteCacheList(id string) error {
	sql := `DELETE FROM remote_cache_list WHERE id = ?`

	result, err := d.db.Exec(sql, id)
	if err != nil {
		return fmt.Errorf("failed to delete remote_cache_list: %w", err)
	}

	affected, err := result.RowsAffected()
	if err != nil {
		return err
	}

	if affected == 0 {
		return ErrRemoteCacheListNotFound
	}

	return nil
}
