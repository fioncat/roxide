package db

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func runRemoteCacheRepoTests(t *testing.T, db *Database) {
	// Create test data
	caches := []*RemoteCacheRepo{
		{
			ID:            "github.com/fioncat/roxide",
			DefaultBranch: "main",
			WebURL:        "https://github.com/fioncat/roxide",
			UpstreamOwner: nil,
			UpstreamName:  nil,
		},
		{
			ID:                    "github.com/user/fork-repo",
			DefaultBranch:         "master",
			WebURL:                "https://github.com/user/fork-repo",
			UpstreamOwner:         StringPtr("original-owner"),
			UpstreamName:          StringPtr("original-repo"),
			UpstreamDefaultBranch: StringPtr("main"),
		},
		{
			ID:            "gitlab.com/fioncat/project",
			DefaultBranch: "develop",
			WebURL:        "https://gitlab.com/fioncat/project",
			UpstreamOwner: nil,
			UpstreamName:  nil,
		},
		{
			ID:            "github.com/golang/go",
			DefaultBranch: "master",
			WebURL:        "https://github.com/golang/go",
			UpstreamOwner: nil,
			UpstreamName:  nil,
		},
		{
			ID:                    "github.com/contributor/rust-fork",
			DefaultBranch:         "master",
			WebURL:                "https://github.com/contributor/rust-fork",
			UpstreamOwner:         StringPtr("rust-lang"),
			UpstreamName:          StringPtr("rust"),
			UpstreamDefaultBranch: StringPtr("master"),
		},
	}

	// Test Insert
	for _, cache := range caches {
		err := db.InsertRemoteCacheRepo(cache)
		assert.NoError(t, err)
	}

	// Test Get - repository without upstream
	fetchedCache, err := db.GetRemoteCacheRepo("github.com/fioncat/roxide")
	assert.NoError(t, err)
	assert.Equal(t, "github.com/fioncat/roxide", fetchedCache.ID)
	assert.Equal(t, "main", fetchedCache.DefaultBranch)
	assert.Equal(t, "https://github.com/fioncat/roxide", fetchedCache.WebURL)
	assert.Nil(t, fetchedCache.UpstreamOwner)
	assert.Nil(t, fetchedCache.UpstreamName)
	assert.Nil(t, fetchedCache.UpstreamDefaultBranch)

	// Test Get - repository with upstream (fork)
	fetchedFork, err := db.GetRemoteCacheRepo("github.com/user/fork-repo")
	assert.NoError(t, err)
	assert.Equal(t, "github.com/user/fork-repo", fetchedFork.ID)
	assert.Equal(t, "master", fetchedFork.DefaultBranch)
	assert.Equal(t, "https://github.com/user/fork-repo", fetchedFork.WebURL)
	assert.Equal(t, StringPtr("original-owner"), fetchedFork.UpstreamOwner)
	assert.Equal(t, StringPtr("original-repo"), fetchedFork.UpstreamName)
	assert.Equal(t, StringPtr("main"), fetchedFork.UpstreamDefaultBranch)

	// Test Get - non-existent repository
	_, err = db.GetRemoteCacheRepo("github.com/nonexistent/repo")
	assert.Equal(t, ErrRemoteCacheRepoNotFound, err)

	// Test Delete
	err = db.DeleteRemoteCacheRepo("github.com/golang/go")
	assert.NoError(t, err)

	// Verify deletion
	_, err = db.GetRemoteCacheRepo("github.com/golang/go")
	assert.Equal(t, ErrRemoteCacheRepoNotFound, err)

	// Test Delete non-existent repository
	err = db.DeleteRemoteCacheRepo("github.com/nonexistent/repo")
	assert.Equal(t, ErrRemoteCacheRepoNotFound, err)

	// Test Insert after Delete (new record)
	newCache := &RemoteCacheRepo{
		ID:            "github.com/golang/go",
		DefaultBranch: "main", // Changed from master to main
		WebURL:        "https://github.com/golang/go",
		UpstreamOwner: nil,
		UpstreamName:  nil,
	}
	err = db.InsertRemoteCacheRepo(newCache)
	assert.NoError(t, err)

	// Verify the new entry
	fetchedNewCache, err := db.GetRemoteCacheRepo("github.com/golang/go")
	assert.NoError(t, err)
	assert.Equal(t, "github.com/golang/go", fetchedNewCache.ID)
	assert.Equal(t, "main", fetchedNewCache.DefaultBranch) // Verify the branch was updated
	assert.Equal(t, "https://github.com/golang/go", fetchedNewCache.WebURL)

	// Test Insert with same key (should fail due to PRIMARY KEY constraint)
	duplicateCache := &RemoteCacheRepo{
		ID:                    "github.com/fioncat/roxide",
		DefaultBranch:         "develop", // Changed from main to develop
		WebURL:                "https://github.com/fioncat/roxide",
		UpstreamOwner:         StringPtr("some-owner"), // Added upstream info
		UpstreamName:          StringPtr("original-roxide"),
		UpstreamDefaultBranch: StringPtr("main"),
	}
	err = db.InsertRemoteCacheRepo(duplicateCache)
	assert.Error(t, err) // Should fail with constraint error

	// Verify the original record is unchanged
	fetchedOriginal, err := db.GetRemoteCacheRepo("github.com/fioncat/roxide")
	assert.NoError(t, err)
	assert.Equal(t, "github.com/fioncat/roxide", fetchedOriginal.ID)
	assert.Equal(t, "main", fetchedOriginal.DefaultBranch) // Should still be "main", not "develop"
	assert.Equal(t, "https://github.com/fioncat/roxide", fetchedOriginal.WebURL)
	assert.Nil(t, fetchedOriginal.UpstreamOwner) // Should still be nil, not "some-owner"
	assert.Nil(t, fetchedOriginal.UpstreamName)
	assert.Nil(t, fetchedOriginal.UpstreamDefaultBranch)

	// To update a record, we need to delete it first and then insert
	err = db.DeleteRemoteCacheRepo("github.com/fioncat/roxide")
	assert.NoError(t, err)

	// Now insert the updated record
	err = db.InsertRemoteCacheRepo(duplicateCache)
	assert.NoError(t, err)

	// Verify the update
	fetchedUpdated, err := db.GetRemoteCacheRepo("github.com/fioncat/roxide")
	assert.NoError(t, err)
	assert.Equal(t, "github.com/fioncat/roxide", fetchedUpdated.ID)
	assert.Equal(t, "develop", fetchedUpdated.DefaultBranch) // Now it should be updated
	assert.Equal(t, "https://github.com/fioncat/roxide", fetchedUpdated.WebURL)
	assert.Equal(t, StringPtr("some-owner"), fetchedUpdated.UpstreamOwner) // Now it should have upstream info
	assert.Equal(t, StringPtr("original-roxide"), fetchedUpdated.UpstreamName)
	assert.Equal(t, StringPtr("main"), fetchedUpdated.UpstreamDefaultBranch)

	// Clean up - delete all entries
	for _, cache := range caches {
		_ = db.DeleteRemoteCacheRepo(cache.ID) // Ignore errors for cleanup
	}
}
