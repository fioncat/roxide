package git

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestParsebranch(t *testing.T) {
	tests := []struct {
		name string
		line string

		want *Branch
	}{
		{
			name: "Test main branch",
			line: "* main cf11adb [origin/main] My best commit since the project begin",

			want: &Branch{
				Name:    "main",
				Status:  BranchStatusSync,
				Current: true,

				CommitID:      "cf11adb",
				CommitMessage: "My best commit since the project begin",
			},
		},
		{
			name: "Test sync branch",
			line: "release/1.6 dc07e7ec7 [origin/release/1.6] Merge pull request #9024 from akhilerm/cherry-pick-9021-release/1.6",

			want: &Branch{
				Name:    "release/1.6",
				Status:  BranchStatusSync,
				Current: false,

				CommitID:      "dc07e7ec7",
				CommitMessage: "Merge pull request #9024 from akhilerm/cherry-pick-9021-release/1.6",
			},
		},
		{
			name: "Test ahead branch",
			line: "feat/update-version 3b0569d62 [origin/feat/update-version: ahead 1] chore: update cargo version",

			want: &Branch{
				Name:    "feat/update-version",
				Status:  BranchStatusAhead,
				Current: false,

				CommitID:      "3b0569d62",
				CommitMessage: "chore: update cargo version",
			},
		},
		{
			name: "Test conflict branch",
			line: "master       b4a40de [origin/master: ahead 1, behind 1] test commit",

			want: &Branch{
				Name:    "master",
				Status:  BranchStatusConflict,
				Current: false,

				CommitID:      "b4a40de",
				CommitMessage: "test commit",
			},
		},
		{
			name: "Test detached branch",
			line: "* dev        b4a40de test commit",

			want: &Branch{
				Name:    "dev",
				Status:  BranchStatusDetached,
				Current: true,

				CommitID:      "b4a40de",
				CommitMessage: "test commit",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := parseBranch(tt.line)
			assert.NoError(t, err)
			assert.Equal(t, tt.want, got)
		})
	}
}
