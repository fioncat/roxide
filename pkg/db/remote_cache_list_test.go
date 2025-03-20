package db

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
)

func runRemoteCacheListTests(t *testing.T, db *Database) {
	// Create test data
	caches := []*RemoteCacheList{
		{
			ID:         "github.com/fioncat",
			Repos:      `["repo1", "repo2", "repo3"]`,
			ExpireTime: 1000,
		},
		{
			ID:         "github.com/golang",
			Repos:      `["go", "tools", "website"]`,
			ExpireTime: 1500,
		},
		{
			ID:         "gitlab.com/fioncat",
			Repos:      `["project1", "project2"]`,
			ExpireTime: 2000,
		},
		{
			ID:         "github.com/rust-lang",
			Repos:      `["rust", "cargo", "rustup"]`,
			ExpireTime: 2500,
		},
		{
			ID:         "gitlab.com/company",
			Repos:      `["internal1", "internal2", "internal3", "internal4"]`,
			ExpireTime: 3000,
		},
	}

	// Test Insert
	for _, cache := range caches {
		err := db.InsertRemoteCacheList(cache)
		assert.NoError(t, err)
	}

	// Test Get
	fetchedCache, err := db.GetRemoteCacheList("github.com/fioncat")
	assert.NoError(t, err)
	assert.Equal(t, "github.com/fioncat", fetchedCache.ID)
	assert.Equal(t, `["repo1", "repo2", "repo3"]`, fetchedCache.Repos)
	assert.Equal(t, uint64(1000), fetchedCache.ExpireTime)

	// Verify JSON parsing works for repos
	var repos []string
	err = json.Unmarshal([]byte(fetchedCache.Repos), &repos)
	assert.NoError(t, err)
	assert.Len(t, repos, 3)
	assert.Equal(t, "repo1", repos[0])
	assert.Equal(t, "repo2", repos[1])
	assert.Equal(t, "repo3", repos[2])

	// Test delete
	err = db.DeleteRemoteCacheList("github.com/rust-lang")
	assert.NoError(t, err)

	// The remote cache is deleted
	_, err = db.GetRemoteCacheList("github.com/rust-lang")
	assert.Equal(t, err, ErrRemoteCacheListNotFound)
}
