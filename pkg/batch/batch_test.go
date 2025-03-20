package batch

import (
	"fmt"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

type testTask struct {
	index int
}

func (t *testTask) Name() string {
	return fmt.Sprintf("task-%d", t.index)
}

func (t *testTask) Run() (int, error) {
	time.Sleep(time.Millisecond * 100)
	return t.index, nil
}

func TestRun(t *testing.T) {
	tasks := make([]*testTask, 0)
	expected := make([]int, 0)
	for i := range 30 {
		tasks = append(tasks, &testTask{i})
		expected = append(expected, i)
	}

	results, err := Run("Test", tasks)
	assert.NoError(t, err)
	assert.Equal(t, len(results), len(tasks))
	assert.Equal(t, results, expected)
}
