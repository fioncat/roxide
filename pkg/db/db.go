package db

import (
	"database/sql"
	"errors"
	"fmt"
	"strings"

	_ "github.com/mattn/go-sqlite3"
)

type Database struct {
	db *sql.DB
}

func Open(path string) (*Database, error) {
	db, err := sql.Open("sqlite3", path)
	if err != nil {
		return nil, fmt.Errorf("failed to open sqlite3 database for %q: %w", path, err)
	}

	return newDatabase(db)
}

func Memory() (*Database, error) {
	db, err := sql.Open("sqlite3", ":memory:")
	if err != nil {
		return nil, fmt.Errorf("failed to open sqlite3 database in memory: %w", err)
	}
	db.SetMaxOpenConns(1)
	return newDatabase(db)
}

func newDatabase(db *sql.DB) (*Database, error) {
	sb := strings.Builder{}
	sb.WriteString(createRepoTable)
	sb.WriteString(createRemoteCacheListTable)
	sb.WriteString(createRemoteCacheRepoTable)
	_, err := db.Exec(sb.String())
	if err != nil {
		return nil, fmt.Errorf("failed to create table: %w", err)
	}

	return &Database{db: db}, nil
}

func (d *Database) Close() error {
	return d.db.Close()
}

func IsNotFound(err error) bool {
	return errors.Is(err, ErrRepoNotFound) || errors.Is(err, ErrRemoteCacheListNotFound) || errors.Is(err, ErrRemoteCacheRepoNotFound)
}
