package db

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestQuerySql(t *testing.T) {
	tests := []struct {
		name       string
		setupQuery func() *querySql
		wantSql    string
		wantValues []any
	}{
		{
			name: "simple select",
			setupQuery: func() *querySql {
				return newQuerySql("users", "id", "name", "age")
			},
			wantSql:    "SELECT id, name, age FROM users",
			wantValues: nil,
		},
		{
			name: "select with where condition",
			setupQuery: func() *querySql {
				q := newQuerySql("users", "id", "name")
				q.where("age", 18)
				q.where("name", "test")
				return q
			},
			wantSql:    "SELECT id, name FROM users WHERE age = ? AND name = ?",
			wantValues: []any{18, "test"},
		},
		{
			name: "select with where like",
			setupQuery: func() *querySql {
				q := newQuerySql("users", "*")
				q.whereLike("name", "%test%")
				return q
			},
			wantSql:    "SELECT * FROM users WHERE name LIKE ?",
			wantValues: []any{"%test%"},
		},
		{
			name: "select with order by",
			setupQuery: func() *querySql {
				q := newQuerySql("users", "id", "name")
				q.orderBy("id DESC", "name ASC")
				return q
			},
			wantSql:    "SELECT id, name FROM users ORDER BY id DESC, name ASC",
			wantValues: nil,
		},
		{
			name: "select with limit and offset",
			setupQuery: func() *querySql {
				q := newQuerySql("users", "*")
				q.withLimit(10)
				q.withOffset(20)
				return q
			},
			wantSql:    "SELECT * FROM users LIMIT ? OFFSET ?",
			wantValues: []any{10, 20},
		},
		{
			name: "complex query",
			setupQuery: func() *querySql {
				q := newQuerySql("users", "id", "name", "age")
				q.where("age", 18)
				q.whereLike("name", "%test%")
				q.orderBy("id DESC")
				q.withLimit(10)
				q.withOffset(20)
				return q
			},
			wantSql:    "SELECT id, name, age FROM users WHERE age = ? AND name LIKE ? ORDER BY id DESC LIMIT ? OFFSET ?",
			wantValues: []any{18, "%test%", 10, 20},
		},
		{
			name: "count query",
			setupQuery: func() *querySql {
				q := newCountSql("users", "COUNT(1)")
				q.where("age", 18)
				return q
			},
			wantSql:    "SELECT COUNT(1) FROM users WHERE age = ?",
			wantValues: []any{18},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			q := tt.setupQuery()
			gotSql, gotValues := q.build()
			assert.Equal(t, tt.wantSql, gotSql)
			assert.Equal(t, tt.wantValues, gotValues)
		})
	}
}

func TestUpdateSql(t *testing.T) {
	tests := []struct {
		name        string
		setupUpdate func() *updateSql
		wantSql     string
		wantValues  []any
	}{
		{
			name: "simple update",
			setupUpdate: func() *updateSql {
				u := newUpdateSql("users", "id", 1)
				u.set("name", "test")
				u.set("age", 20)
				return u
			},
			wantSql:    "UPDATE users SET name = ?, age = ? WHERE id = ?",
			wantValues: []any{"test", 20, 1},
		},
		{
			name: "update with string id",
			setupUpdate: func() *updateSql {
				u := newUpdateSql("users", "uuid", "123e4567-e89b-12d3-a456-426614174000")
				u.set("status", "active")
				return u
			},
			wantSql:    "UPDATE users SET status = ? WHERE uuid = ?",
			wantValues: []any{"active", "123e4567-e89b-12d3-a456-426614174000"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			u := tt.setupUpdate()
			gotSql, gotValues := u.build()
			assert.Equal(t, tt.wantSql, gotSql)
			assert.Equal(t, tt.wantValues, gotValues)
		})
	}
}
