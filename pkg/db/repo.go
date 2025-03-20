package db

import (
	"errors"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/dustin/go-humanize"
	"github.com/fioncat/roxide/pkg/timeutils"
)

var ErrRepoNotFound = errors.New("repository not found")

type Repository struct {
	ID string `json:"id,omitempty"`

	Remote string `json:"remote,omitempty"`
	Owner  string `json:"owner,omitempty"`
	Name   string `json:"name,omitempty"`

	Path *string `json:"path,omitempty"`

	Pin  bool `json:"pin"`
	Sync bool `json:"sync"`

	Language *string `json:"language,omitempty"`

	VisitTime  uint64 `json:"visit_time"`
	VisitCount uint64 `json:"visit_count"`
	Score      uint64 `json:"score"`

	NewCreated bool `json:"-"`

	DisplayLevel DisplayRepoLevel `json:"-"`

	Size int64 `json:"size,omitempty"`
}

func BuildRepoID(remote, owner, name string) string {
	return fmt.Sprintf("%s:%s/%s", remote, owner, name)
}

type DisplayRepoLevel int

const (
	DisplayRepoRemote DisplayRepoLevel = iota
	DisplayRepoOwner
	DisplayRepoName
)

func (r *Repository) GetFields(now uint64) map[string]any {
	name := r.String()

	var flags []string
	if r.Pin {
		flags = append(flags, "pin")
	}
	if r.Sync {
		flags = append(flags, "sync")
	}

	var flagsRow string
	if len(flags) > 0 {
		flagsRow = strings.Join(flags, ",")
	} else {
		flagsRow = "<none>"
	}

	visitTime := timeutils.FormatSince(r.VisitTime, now)

	var language string
	if r.Language != nil {
		language = *r.Language
	} else {
		language = "<none>"
	}

	size := humanize.IBytes(uint64(r.Size))

	return map[string]any{
		"Name":      name,
		"Flags":     flagsRow,
		"Language":  language,
		"Visited":   r.VisitCount,
		"VisitTime": visitTime,
		"Score":     r.Score,
		"Size":      size,
	}
}

func (r *Repository) String() string {
	return r.Display(r.DisplayLevel)
}

func (r *Repository) Display(level DisplayRepoLevel) string {
	switch level {
	case DisplayRepoOwner:
		return fmt.Sprintf("%s/%s", r.Owner, r.Name)
	case DisplayRepoName:
		return r.Name
	default:
		return fmt.Sprintf("%s:%s/%s", r.Remote, r.Owner, r.Name)
	}
}

func (r *Repository) GetPath(workspace string) string {
	if r.Path != nil {
		return *r.Path
	}

	remoteDir := filepath.Join(workspace, r.Remote)
	ownerDir := filepath.Join(remoteDir, r.Owner)
	return filepath.Join(ownerDir, r.Name)
}

func (r *Repository) UpdateVisitOptions() UpdateRepositoryOptions {
	now := timeutils.Now()
	count := r.VisitCount + 1
	delta := now - r.VisitTime
	score := getScore(delta, count)

	return UpdateRepositoryOptions{
		VisitTime:  Uint64Ptr(now),
		VisitCount: Uint64Ptr(count),
		Score:      Uint64Ptr(score),
	}
}

func (r *Repository) InitScore() {
	now := timeutils.Now()
	score := getScore(0, 1)

	r.VisitTime = now
	r.VisitCount = 1
	r.Score = score
}

// The scoring algorithm is borrowed from:
//
//	<https://github.com/ajeetdsouza/zoxide>
func getScore(delta, count uint64) (score uint64) {
	if delta < timeutils.HourSeconds {
		score = count * 16
	} else if delta < timeutils.DaySeconds {
		score = count * 8
	} else if delta < timeutils.WeekSeconds {
		score = count * 2
	} else {
		score = count
	}
	return
}

type QueryRepositoryOptions struct {
	ID *string

	Remote *string
	Owner  *string

	NameSearch *string

	Path *string

	Pin  *bool
	Sync *bool

	Language *string

	OrderBy []string
	Offset  *int
	Limit   *int
}

func (q *QueryRepositoryOptions) OrderByScore() {
	q.OrderBy = []string{"score DESC"}
}

func (q *QueryRepositoryOptions) setupSql(query *querySql) {
	if q.ID != nil {
		query.where("id", *q.ID)
	}
	if q.Remote != nil {
		query.where("remote", *q.Remote)
	}
	if q.Owner != nil {
		query.where("owner", *q.Owner)
	}
	if q.NameSearch != nil {
		query.whereLike("name", fmt.Sprintf("%%%s%%", *q.NameSearch))
	}
	if q.Path != nil {
		query.where("path", *q.Path)
	}
	if q.Pin != nil {
		query.where("pin", *q.Pin)
	}
	if q.Sync != nil {
		query.where("sync", *q.Sync)
	}
	if q.Language != nil {
		query.where("language", *q.Language)
	}

	if len(q.OrderBy) > 0 {
		query.orderBy(q.OrderBy...)
	}

	if q.Limit != nil {
		query.withLimit(*q.Limit)
	}

	if q.Offset != nil {
		query.withOffset(*q.Offset)
	}
}

type UpdateRepositoryOptions struct {
	Pin  *bool
	Sync *bool

	Language *string

	VisitTime  *uint64
	VisitCount *uint64
	Score      *uint64
}

func (u *UpdateRepositoryOptions) setupSql(update *updateSql) {
	if u.Pin != nil {
		update.set("pin", *u.Pin)
	}
	if u.Sync != nil {
		update.set("sync", *u.Sync)
	}
	if u.Language != nil {
		update.set("language", *u.Language)
	}
	if u.VisitTime != nil {
		update.set("visit_time", *u.VisitTime)
	}
	if u.VisitCount != nil {
		update.set("visit_count", *u.VisitCount)
	}
	if u.Score != nil {
		update.set("score", *u.Score)
	}
}

type Owner struct {
	Remote string
	Owner  string

	Name string

	Count uint64
}

func (o *Owner) GetFields(_ uint64) map[string]any {
	return map[string]any{
		"Remote": o.Remote,
		"Owner":  o.Owner,
		"Name":   o.Name,
		"Count":  o.Count,
	}
}

type QueryOwnerOptions struct {
	Remote *string

	Offset *int
	Limit  *int
}

func (o *QueryOwnerOptions) setupSql(query *querySql) {
	if o.Remote != nil {
		query.where("remote", *o.Remote)
	}
	if o.Limit != nil {
		query.withLimit(*o.Limit)
	}
	if o.Offset != nil {
		query.withOffset(*o.Offset)
	}
}

const createRepoTable = `
CREATE TABLE IF NOT EXISTS repo (
	id TEXT PRIMARY KEY,
	remote TEXT NOT NULL,
	owner TEXT NOT NULL,
	name TEXT NOT NULL,
	path TEXT,
	pin INTEGER NOT NULL,
	sync INTEGER NOT NULL,
	language TEXT,
	visit_time INTEGER NOT NULL,
	visit_count INTEGER NOT NULL,
	score INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_repo_remote ON repo(remote);
CREATE INDEX IF NOT EXISTS idx_repo_owner ON repo(owner);
CREATE INDEX IF NOT EXISTS idx_repo_name ON repo(name);
CREATE INDEX IF NOT EXISTS idx_repo_path ON repo(path);
CREATE INDEX IF NOT EXISTS idx_repo_score ON repo(score);
`

const insertRepoSql = `
INSERT INTO repo (
	id,
	remote,
	owner,
	name,
	path,
	pin,
	sync,
	language,
	visit_time,
	visit_count,
	score
) VALUES (
	?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
);
`

func (d *Database) InsertRepo(repo *Repository) error {
	_, err := d.db.Exec(insertRepoSql,
		repo.ID,
		repo.Remote,
		repo.Owner,
		repo.Name,
		repo.Path,
		repo.Pin,
		repo.Sync,
		repo.Language,
		repo.VisitTime,
		repo.VisitCount,
		repo.Score)
	if err != nil {
		return fmt.Errorf("failed to insert repo: %w", err)
	}

	return nil
}

func (d *Database) GetRepo(id string) (*Repository, error) {
	repos, err := d.QueryRepos(QueryRepositoryOptions{
		ID: StringPtr(id),
	})
	if err != nil {
		return nil, err
	}

	if len(repos) == 0 {
		return nil, ErrRepoNotFound
	}

	return repos[0], nil
}

func (d *Database) UpdateRepo(id string, opts UpdateRepositoryOptions) error {
	update := newUpdateSql("repo", "id", id)

	opts.setupSql(update)

	sql, values := update.build()

	result, err := d.db.Exec(sql, values...)
	if err != nil {
		return fmt.Errorf("failed to update repo: %w", err)
	}

	affected, err := result.RowsAffected()
	if err != nil {
		return err
	}

	if affected == 0 {
		return ErrRepoNotFound
	}

	return nil
}

func (d *Database) ResetRepoLanguage(id string) error {
	sql := `UPDATE repo SET language = NULL WHERE id = ?`
	_, err := d.db.Exec(sql, id)
	if err != nil {
		return fmt.Errorf("failed to reset repo language: %w", err)
	}

	return nil
}

func (d *Database) DeleteRepo(id string) error {
	sql := `DELETE FROM repo WHERE id = ?`

	result, err := d.db.Exec(sql, id)
	if err != nil {
		return fmt.Errorf("failed to delete repo: %w", err)
	}

	affected, err := result.RowsAffected()
	if err != nil {
		return err
	}

	if affected == 0 {
		return ErrRepoNotFound
	}

	return nil
}

func (d *Database) QueryOwners(opts QueryOwnerOptions) ([]*Owner, error) {
	query := newQuerySql("repo", "remote", "owner", "COUNT(1) AS count")
	query.groupBy("owner")
	query.orderBy("count DESC")
	opts.setupSql(query)

	sql, values := query.build()

	rows, err := d.db.Query(sql, values...)
	if err != nil {
		return nil, fmt.Errorf("failed to query owners: %w", err)
	}
	defer rows.Close()

	var owners []*Owner
	for rows.Next() {
		var owner Owner
		err = rows.Scan(&owner.Remote, &owner.Owner, &owner.Count)
		if err != nil {
			return nil, fmt.Errorf("failed to scan owner: %w", err)
		}
		if opts.Remote == nil {
			owner.Name = fmt.Sprintf("%s/%s", owner.Remote, owner.Owner)
		} else {
			owner.Name = owner.Owner
		}
		owners = append(owners, &owner)
	}

	return owners, nil
}

func (d *Database) CountOwners(opts QueryOwnerOptions) (int, error) {
	query := newCountSql("repo", "count(DISTINCT owner)")
	opts.setupSql(query)

	sql, values := query.build()

	var count int
	err := d.db.QueryRow(sql, values...).Scan(&count)
	if err != nil {
		return 0, fmt.Errorf("failed to count owners: %w", err)
	}

	return count, nil
}

func (d *Database) QueryRepos(opts QueryRepositoryOptions) ([]*Repository, error) {
	query := newQuerySql("repo",
		"id",
		"remote",
		"owner",
		"name",
		"path",
		"pin",
		"sync",
		"language",
		"visit_time",
		"visit_count",
		"score")

	opts.setupSql(query)

	sql, values := query.build()

	rows, err := d.db.Query(sql, values...)
	if err != nil {
		return nil, fmt.Errorf("failed to query repos: %w", err)
	}
	defer rows.Close()

	var repos []*Repository
	for rows.Next() {
		var repo Repository
		err = rows.Scan(
			&repo.ID,
			&repo.Remote,
			&repo.Owner,
			&repo.Name,
			&repo.Path,
			&repo.Pin,
			&repo.Sync,
			&repo.Language,
			&repo.VisitTime,
			&repo.VisitCount,
			&repo.Score)
		if err != nil {
			return nil, fmt.Errorf("failed to scan repo: %w", err)
		}

		repos = append(repos, &repo)
	}

	return repos, nil
}

func (d *Database) CountRepos(opts QueryRepositoryOptions) (int, error) {
	query := newCountSql("repo", "COUNT(1)")
	opts.setupSql(query)

	sql, values := query.build()

	var count int
	err := d.db.QueryRow(sql, values...).Scan(&count)
	if err != nil {
		return 0, fmt.Errorf("failed to count repos: %w", err)
	}

	return count, nil
}
