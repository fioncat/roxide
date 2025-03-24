package batch

import (
	"fmt"
	"os"
	"runtime"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/fatih/color"
	"github.com/fioncat/roxide/pkg/term"
)

type Task[R any] interface {
	Name() string
	Run() (R, error)
}

type TaskResult[R any] struct {
	Error error
	Value R
}

type taskSend[R any] struct {
	index int
	task  Task[R]
}

type runningTask struct {
	index int
	name  string
}

type doneTask[R any] struct {
	index int

	result R
	err    error
}

type reportTask[R any] struct {
	running *runningTask
	done    *doneTask[R]
}

type failMessage struct {
	name    string
	message string
}

type tracker[R any] struct {
	total    int
	totalPad int

	runnings []*runningTask
	dones    []*doneTask[R]

	desc      string
	descPure  string
	descWidth int
	descHead  string

	okCount   int
	failCount int

	failMessages []*failMessage
}

const (
	defaultTerminalWidth = 20

	minimalBarWidth = 20

	spaceStr = " "
	sepStr   = ", "
	omitStr  = ", ..."

	spaceWidth = len(spaceStr)
	sepWidth   = len(sepStr)
	omitWidth  = len(omitStr)
)

func newTracker[R any](desc string, total int) *tracker[R] {
	descWidth := len(desc)
	descPure := desc
	c := color.New(color.FgCyan, color.Bold)
	desc = c.Sprint(desc)

	descHead := strings.Repeat(spaceStr, descWidth)

	totalStr := fmt.Sprint(total)
	totalPad := len(totalStr)

	return &tracker[R]{
		total:    total,
		totalPad: totalPad,

		runnings: make([]*runningTask, 0, total),
		dones:    make([]*doneTask[R], 0, total),

		desc:      desc,
		descPure:  descPure,
		descWidth: descWidth,
		descHead:  descHead,

		okCount:   0,
		failCount: 0,

		failMessages: nil,
	}
}

func (t *tracker[R]) wait(reportChan <-chan *reportTask[R]) ([]R, error) {
	start := time.Now()
	for len(t.dones) < t.total {
		report, ok := <-reportChan
		if !ok {
			break
		}

		if report.running != nil {
			t.traceRunning(report.running)
		} else {
			t.traceDone(report.done)
		}
	}
	elapsed := time.Since(start)

	var result string
	if t.failCount > 0 {
		result = color.RedString("fail")
	} else {
		result = color.GreenString("ok")
	}

	term.CursorUp()
	fmt.Fprintln(os.Stderr)
	fmt.Fprintf(os.Stderr, "%s result: %s. %d ok; %d failed; finished in %v\n", t.descPure, result, t.okCount, t.failCount, elapsed)

	if len(t.failMessages) > 0 {
		fmt.Fprintln(os.Stderr)
		fmt.Fprintln(os.Stderr, "Error message:")
		for _, msg := range t.failMessages {
			fmt.Fprintf(os.Stderr, "  %s: %s\n", msg.name, msg.message)

		}
		fmt.Fprintln(os.Stderr)

		return nil, fmt.Errorf("%s task failed", t.descPure)
	}

	results := make([]R, t.total)
	for _, task := range t.dones {
		results[task.index] = task.result
	}

	return results, nil
}

func (t *tracker[R]) traceRunning(task *runningTask) {
	t.runnings = append(t.runnings, task)
	line := t.render()
	term.CursorUp()
	fmt.Fprintln(os.Stderr, line)
}

func (t *tracker[R]) traceDone(task *doneTask[R]) {
	var name string
	foundIdx := -1
	for idx, runningTask := range t.runnings {
		if runningTask.index == task.index {
			foundIdx = idx
			name = runningTask.name
			break
		}
	}

	if foundIdx < 0 {
		return
	}

	t.runnings = slices.Delete(t.runnings, foundIdx, foundIdx+1)

	term.CursorUp()
	if task.err != nil {
		t.failCount += 1
		fmt.Fprintf(os.Stderr, "%s %s %s\n", t.descHead, name, color.RedString("fail"))
		msg := &failMessage{
			name:    name,
			message: task.err.Error(),
		}
		t.failMessages = append(t.failMessages, msg)
	} else {
		t.okCount += 1
		fmt.Fprintf(os.Stderr, "%s %s %s\n", t.descHead, name, color.GreenString("ok"))
	}

	t.dones = append(t.dones, task)
	line := t.render()
	fmt.Fprintln(os.Stderr, line)
}

func (t *tracker[R]) render() string {
	termWidth, _ := term.GetWidth()
	if termWidth <= 0 {
		termWidth = defaultTerminalWidth
	}

	if t.descWidth > termWidth {
		// The terminal is too small, no space to print info, just print "....".
		return strings.Repeat(".", termWidth)
	}

	// Render desc (with color).
	sb := strings.Builder{}
	sb.WriteString(t.desc)
	renderWidth := t.descWidth
	if renderWidth+spaceWidth > termWidth || t.getBarWidth(termWidth) == 0 {
		return sb.String()
	}
	sb.WriteString(spaceStr)
	renderWidth += spaceWidth

	// Render progress bar
	bar := t.renderBar(len(t.dones), t.total, termWidth)
	barWidth := len(bar)
	if renderWidth+barWidth > termWidth {
		return sb.String()
	}
	sb.WriteString(bar)
	renderWidth += barWidth

	// Splitter
	if renderWidth+spaceWidth > termWidth {
		return sb.String()
	}
	sb.WriteString(spaceStr)
	renderWidth += spaceWidth

	// Render tag
	tag := t.renderTag()
	tagWidth := len(tag)
	if renderWidth+tagWidth > termWidth {
		return sb.String()
	}
	sb.WriteString(tag)
	renderWidth += tagWidth

	// Splitter
	if renderWidth+spaceWidth > termWidth {
		return sb.String()
	}
	sb.WriteString(spaceStr)
	renderWidth += spaceWidth

	// Runnings
	left := termWidth - renderWidth
	if left <= 0 {
		return sb.String()
	}
	running := t.renderRunning(left)
	sb.WriteString(running)

	return sb.String()
}

func (t *tracker[R]) getBarWidth(termWidth int) int {
	if termWidth <= minimalBarWidth {
		return 0
	}

	return termWidth / 4
}

func (t *tracker[R]) renderBar(current, total, termWidth int) string {
	barWidth := t.getBarWidth(termWidth)
	var currentCount int
	if current >= total {
		currentCount = barWidth
	} else {
		percent := float64(current) / float64(total)
		currentFloat := float64(barWidth) * percent
		currentFixed := int(currentFloat)
		currentCount = min(currentFixed, barWidth)
	}

	var currentBar string
	switch currentCount {
	case 0:

	case 1:
		currentBar = ">"

	default:
		currentBar = fmt.Sprintf("%s>", strings.Repeat("=", currentCount-1))
	}

	if currentCount >= barWidth {
		return fmt.Sprintf("[%s]", currentBar)
	}

	pending := strings.Repeat(" ", barWidth-currentCount)
	return fmt.Sprintf("[%s%s]", currentBar, pending)
}

func (t *tracker[R]) renderTag() string {
	pad := t.totalPad
	current := fmt.Sprintf("%*d", pad, len(t.dones))
	return fmt.Sprintf("(%s/%d)", current, t.total)
}

func (t *tracker[R]) renderRunning(width int) string {
	result := strings.Builder{}
	result.Grow(width)

	for idx, task := range t.runnings {
		var addWidth int
		if task.index == 0 {
			addWidth = len(task.name)
		} else {
			addWidth = len(task.name) + sepWidth
		}
		isLast := idx == len(t.runnings)-1
		resultWidth := result.Len()
		newWidth := resultWidth + addWidth
		if newWidth > width || (!isLast && newWidth == width) {
			delta := width - resultWidth
			if delta <= 0 {
				break
			}
			if delta < omitWidth {
				result.WriteString(strings.Repeat(".", delta))
			} else {
				result.WriteString(omitStr)
			}
			break
		}
		if idx != 0 {
			result.WriteString(sepStr)
		}
		result.WriteString(task.name)
	}

	s := result.String()
	if len(s) > width {
		return s[:width]
	}

	return s
}

func Run[R any, T Task[R]](desc string, tasks []T) ([]R, error) {
	if len(tasks) == 0 {
		return nil, nil
	}

	term.Mute = true
	defer func() {
		term.Mute = false
	}()

	// Set the number of workers to the number of cpu cores to maximize the use of
	// multicore cpu.
	workerCount := runtime.NumCPU()

	taskChan := make(chan *taskSend[R], len(tasks))

	reportChan := make(chan *reportTask[R], len(tasks))

	c := color.New(color.FgCyan, color.Bold)
	title := c.Sprintf("%s with %d workers\n", desc, workerCount)
	fmt.Fprintln(os.Stderr, title)

	wg := sync.WaitGroup{}
	for range workerCount {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for taskSend := range taskChan {
				runningReport := &reportTask[R]{
					running: &runningTask{
						index: taskSend.index,
						name:  taskSend.task.Name(),
					},
				}
				reportChan <- runningReport

				value, err := taskSend.task.Run()
				doneReport := &reportTask[R]{
					done: &doneTask[R]{
						index:  taskSend.index,
						result: value,
						err:    err,
					},
				}
				reportChan <- doneReport
			}
		}()
	}

	for idx, task := range tasks {
		send := &taskSend[R]{
			index: idx,
			task:  task,
		}
		taskChan <- send
	}
	close(taskChan)

	tracker := newTracker[R](desc, len(tasks))
	results, err := tracker.wait(reportChan)

	wg.Wait()

	return results, err
}
