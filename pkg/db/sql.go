package db

import (
	"fmt"
	"strings"
)

type querySql struct {
	fields []string
	table  string

	count bool

	wheres   []string
	groupBys []string
	orderBys []string

	offset int
	limit  int

	values []any
}

func newQuerySql(table string, fields ...string) *querySql {
	return &querySql{
		fields: fields,
		table:  table,
	}
}

func newCountSql(table string, field string) *querySql {
	return &querySql{
		fields: []string{field},
		table:  table,
		count:  true,
	}
}

func (q *querySql) where(field string, value any) {
	sql := fmt.Sprintf("%s = ?", field)
	q.wheres = append(q.wheres, sql)
	q.values = append(q.values, value)
}

func (q *querySql) whereLike(field string, value any) {
	sql := fmt.Sprintf("%s LIKE ?", field)
	q.wheres = append(q.wheres, sql)
	q.values = append(q.values, value)
}

func (q *querySql) orderBy(fields ...string) {
	q.orderBys = append(q.orderBys, fields...)
}

func (q *querySql) groupBy(fields ...string) {
	q.groupBys = append(q.groupBys, fields...)
}

func (q *querySql) withLimit(limit int) {
	if q.count {
		return
	}
	if limit > 0 {
		q.limit = limit
	}
}

func (q *querySql) withOffset(offset int) {
	if q.count {
		return
	}
	if offset > 0 {
		q.offset = offset
	}
}

func (q *querySql) build() (string, []any) {
	fields := strings.Join(q.fields, ", ")
	sql := fmt.Sprintf("SELECT %s FROM %s", fields, q.table)

	sb := strings.Builder{}
	sb.WriteString(sql)

	if len(q.wheres) > 0 {
		cond := strings.Join(q.wheres, " AND ")
		where := fmt.Sprintf(" WHERE %s", cond)
		sb.WriteString(where)
	}

	if q.count {
		return sb.String(), q.values
	}

	values := q.values

	if len(q.groupBys) > 0 {
		groups := strings.Join(q.groupBys, ", ")
		group := fmt.Sprintf(" GROUP BY %s", groups)
		sb.WriteString(group)
	}

	if len(q.orderBys) > 0 {
		orders := strings.Join(q.orderBys, ", ")
		order := fmt.Sprintf(" ORDER BY %s", orders)
		sb.WriteString(order)
	}

	if q.limit > 0 {
		sb.WriteString(" LIMIT ?")
		values = append(values, q.limit)
		if q.offset > 0 {
			sb.WriteString(" OFFSET ?")
			values = append(values, q.offset)
		}
	}

	return sb.String(), values
}

type updateSql struct {
	table   string
	idField string
	idValue any

	fields []string
	values []any
}

func newUpdateSql(table, idField string, idValue any) *updateSql {
	return &updateSql{
		table:   table,
		idField: idField,
		idValue: idValue,
	}
}

func (u *updateSql) set(field string, value any) {
	set := fmt.Sprintf("%s = ?", field)
	u.fields = append(u.fields, set)
	u.values = append(u.values, value)
}

func (u *updateSql) build() (string, []any) {
	if len(u.fields) == 0 {
		panic("no fields to update")
	}

	fields := strings.Join(u.fields, ", ")
	sql := fmt.Sprintf("UPDATE %s SET %s WHERE %s = ?", u.table, fields, u.idField)
	values := append(u.values, u.idValue)
	return sql, values
}
