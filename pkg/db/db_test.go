package db

import (
	"os"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestMemoryDatabase(t *testing.T) {
	db, err := Memory()
	assert.NoError(t, err)
	defer db.Close()

	runRepoTests(t, db)
	runRemoteCacheListTests(t, db)
	runRemoteCacheRepoTests(t, db)
}

func TestDatabase(t *testing.T) {
	path := "./testdata/sqlite.db"
	_ = os.Remove(path)

	db, err := Open(path)
	assert.NoError(t, err)
	defer db.Close()

	runRepoTests(t, db)
	runRemoteCacheListTests(t, db)
	runRemoteCacheRepoTests(t, db)
}
