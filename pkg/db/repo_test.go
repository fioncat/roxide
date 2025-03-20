package db

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func runRepoTests(t *testing.T, db *Database) {
	// Insert multiple repositories for testing
	repos := []*Repository{
		{
			ID:         "github-fioncat-roxide",
			Remote:     "github.com",
			Owner:      "fioncat",
			Name:       "roxide",
			Path:       StringPtr("/path/to/roxide"),
			Pin:        true,
			Sync:       true,
			Language:   StringPtr("Go"),
			VisitTime:  1000,
			VisitCount: 10,
			Score:      800,
		},
		{
			ID:         "github-fioncat-codebase",
			Remote:     "github.com",
			Owner:      "fioncat",
			Name:       "codebase",
			Path:       StringPtr("/path/to/codebase"),
			Pin:        false,
			Sync:       true,
			Language:   StringPtr("Go"),
			VisitTime:  1200,
			VisitCount: 5,
			Score:      500,
		},
		{
			ID:         "github-rust-lang-rust",
			Remote:     "github.com",
			Owner:      "rust-lang",
			Name:       "rust",
			Path:       StringPtr("/path/to/rust"),
			Pin:        true,
			Sync:       true,
			Language:   StringPtr("Rust"),
			VisitTime:  1500,
			VisitCount: 20,
			Score:      1000,
		},
		{
			ID:         "gitlab-fioncat-project",
			Remote:     "gitlab.com",
			Owner:      "fioncat",
			Name:       "project",
			Path:       StringPtr("/path/to/project"),
			Pin:        false,
			Sync:       false,
			Language:   StringPtr("Python"),
			VisitTime:  800,
			VisitCount: 3,
			Score:      300,
		},
		{
			ID:         "github-golang-go",
			Remote:     "github.com",
			Owner:      "golang",
			Name:       "go",
			Path:       StringPtr("/path/to/go"),
			Pin:        true,
			Sync:       true,
			Language:   StringPtr("Go"),
			VisitTime:  1800,
			VisitCount: 15,
			Score:      900,
		},
	}

	// Insert all repositories
	for _, repo := range repos {
		err := db.InsertRepo(repo)
		assert.NoError(t, err)
	}

	// Test basic Get
	fetchedRepo, err := db.GetRepo("github-fioncat-roxide")
	assert.NoError(t, err)
	assert.Equal(t, "github-fioncat-roxide", fetchedRepo.ID)
	assert.Equal(t, "github.com", fetchedRepo.Remote)
	assert.Equal(t, "fioncat", fetchedRepo.Owner)
	assert.Equal(t, "roxide", fetchedRepo.Name)
	assert.Equal(t, StringPtr("/path/to/roxide"), fetchedRepo.Path)
	assert.Equal(t, true, fetchedRepo.Pin)
	assert.Equal(t, true, fetchedRepo.Sync)
	assert.Equal(t, StringPtr("Go"), fetchedRepo.Language)

	// Test Get non-existent repo
	_, err = db.GetRepo("non-existent-repo")
	assert.Equal(t, ErrRepoNotFound, err)

	// Test basic Query - by Remote
	githubRepos, err := db.QueryRepos(QueryRepositoryOptions{
		Remote: StringPtr("github.com"),
	})
	assert.NoError(t, err)
	assert.Len(t, githubRepos, 4)

	// Test Query - by Owner
	fionRepos, err := db.QueryRepos(QueryRepositoryOptions{
		Owner: StringPtr("fioncat"),
	})
	assert.NoError(t, err)
	assert.Len(t, fionRepos, 3)

	// Test Query - by Language
	goRepos, err := db.QueryRepos(QueryRepositoryOptions{
		Language: StringPtr("Go"),
	})
	assert.NoError(t, err)
	assert.Len(t, goRepos, 3)

	// Test Query - by Pin
	pinnedRepos, err := db.QueryRepos(QueryRepositoryOptions{
		Pin: BoolPtr(true),
	})
	assert.NoError(t, err)
	assert.Len(t, pinnedRepos, 3)

	// Test Query - by Sync
	syncRepos, err := db.QueryRepos(QueryRepositoryOptions{
		Sync: BoolPtr(true),
	})
	assert.NoError(t, err)
	assert.Len(t, syncRepos, 4)

	// Test Query - name search
	rustRepos, err := db.QueryRepos(QueryRepositoryOptions{
		NameSearch: StringPtr("rust"),
	})
	assert.NoError(t, err)
	assert.Len(t, rustRepos, 1)
	assert.Equal(t, "github-rust-lang-rust", rustRepos[0].ID)

	// Test complex Query - multiple conditions
	complexQuery, err := db.QueryRepos(QueryRepositoryOptions{
		Remote:   StringPtr("github.com"),
		Language: StringPtr("Go"),
		Pin:      BoolPtr(true),
	})
	assert.NoError(t, err)
	assert.Len(t, complexQuery, 2)

	// Test Query with ordering - by visit count descending
	orderedByVisit, err := db.QueryRepos(QueryRepositoryOptions{
		OrderBy: []string{"visit_count DESC"},
	})
	assert.NoError(t, err)
	assert.Len(t, orderedByVisit, 5)
	assert.Equal(t, "github-rust-lang-rust", orderedByVisit[0].ID) // Highest visit count

	// Test Query with ordering - by score ascending
	orderedByScore, err := db.QueryRepos(QueryRepositoryOptions{
		OrderBy: []string{"score ASC"},
	})
	assert.NoError(t, err)
	assert.Len(t, orderedByScore, 5)
	assert.Equal(t, "gitlab-fioncat-project", orderedByScore[0].ID) // Lowest score

	// Test Query with multiple ordering criteria
	multiOrdered, err := db.QueryRepos(QueryRepositoryOptions{
		OrderBy: []string{"language ASC", "visit_time DESC"},
	})
	assert.NoError(t, err)
	assert.Len(t, multiOrdered, 5)
	// Should be ordered by language first, then by visit_time within each language

	// Test Query with limit
	limitedQuery, err := db.QueryRepos(QueryRepositoryOptions{
		OrderBy: []string{"visit_time DESC"},
		Limit:   IntPtr(2),
	})
	assert.NoError(t, err)
	assert.Len(t, limitedQuery, 2)
	assert.Equal(t, "github-golang-go", limitedQuery[0].ID) // Highest visit time

	// Test Query with offset
	offsetQuery, err := db.QueryRepos(QueryRepositoryOptions{
		OrderBy: []string{"visit_time DESC"},
		Limit:   IntPtr(2),
		Offset:  IntPtr(1),
	})
	assert.NoError(t, err)
	assert.Len(t, offsetQuery, 2)
	assert.Equal(t, "github-rust-lang-rust", offsetQuery[0].ID) // Second highest visit time

	// Test Count
	count, err := db.CountRepos(QueryRepositoryOptions{
		Remote: StringPtr("github.com"),
	})
	assert.NoError(t, err)
	assert.Equal(t, 4, count)

	// Test Update
	updateOpts := UpdateRepositoryOptions{
		Language:  StringPtr("Rust"),
		VisitTime: Uint64Ptr(2000),
		Score:     Uint64Ptr(1200),
	}
	err = db.UpdateRepo("github-fioncat-roxide", updateOpts)
	assert.NoError(t, err)

	// Verify update
	updatedRepo, err := db.GetRepo("github-fioncat-roxide")
	assert.NoError(t, err)
	assert.Equal(t, StringPtr("Rust"), updatedRepo.Language)
	assert.Equal(t, uint64(2000), updatedRepo.VisitTime)
	assert.Equal(t, uint64(1200), updatedRepo.Score)
	// Fields that weren't updated should remain the same
	assert.Equal(t, true, updatedRepo.Pin)
	assert.Equal(t, true, updatedRepo.Sync)

	// Test Query after update - language change should affect query results
	rustReposAfterUpdate, err := db.QueryRepos(QueryRepositoryOptions{
		Language: StringPtr("Rust"),
	})
	assert.NoError(t, err)
	assert.Len(t, rustReposAfterUpdate, 2)

	goReposAfterUpdate, err := db.QueryRepos(QueryRepositoryOptions{
		Language: StringPtr("Go"),
	})
	assert.NoError(t, err)
	assert.Len(t, goReposAfterUpdate, 2) // One less than before

	// Test Delete
	err = db.DeleteRepo("github-fioncat-codebase")
	assert.NoError(t, err)

	// Verify deletion
	_, err = db.GetRepo("github-fioncat-codebase")
	assert.Equal(t, ErrRepoNotFound, err)

	// Test Query after deletion
	fionReposAfterDelete, err := db.QueryRepos(QueryRepositoryOptions{
		Owner: StringPtr("fioncat"),
	})
	assert.NoError(t, err)
	assert.Len(t, fionReposAfterDelete, 2) // One less than before

	// Test Delete non-existent repo
	err = db.DeleteRepo("non-existent-repo")
	assert.Equal(t, ErrRepoNotFound, err)
}
