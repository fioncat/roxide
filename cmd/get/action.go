package get

import (
	"errors"
	"fmt"
	"os"
	"time"

	"github.com/fatih/color"
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/remoteapi"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newAction() *cobra.Command {
	var opts actionOptions

	c := &cobra.Command{
		Use:   "action",
		Short: "Show action for current commit",

		Args: cobra.NoArgs,

		ValidArgsFunction: cmd.NoneCompletion,
	}

	return cmd.Build(c, &opts)
}

type actionOptions struct{}

func (o *actionOptions) Complete(c *cobra.Command, args []string) error {
	return nil
}

func (o *actionOptions) Run(ctx *context.Context) error {
	term.Mute = true
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	req, err := repoutils.GetActionRequest(ctx)
	if err != nil {
		return err
	}

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return err
	}

	watcher := &actionWatcher{
		statusMap: make(map[int64]remoteapi.ActionJobStatus),

		api: api,
		req: req,
	}

	return watcher.wait()
}

type actionWatcher struct {
	statusMap map[int64]remoteapi.ActionJobStatus

	lastLines int

	completed bool

	action *remoteapi.Action

	api remoteapi.RemoteAPI

	req *remoteapi.ActionRequest
}

func (w *actionWatcher) wait() error {
	action, err := w.api.GetAction(w.req)
	if err != nil {
		return err
	}
	if action == nil {
		return errors.New("no action found")
	}

	var id string
	if len(action.Commit.ID) > 8 {
		id = action.Commit.ID[:8]
	} else {
		id = action.Commit.ID
	}
	fmt.Fprintf(os.Stderr, "Commit [%s] %s\n", id, color.YellowString(action.Commit.Message))
	author := fmt.Sprintf("%s <%s>", action.Commit.AuthorName, action.Commit.AuthorEmail)
	fmt.Fprintf(os.Stderr, "Author %s\n", color.BlueString(author))

	w.action = action

	for !w.completed {
		updated := w.updateStatus()
		if updated {
			w.display()
		}

		if !w.completed {
			err = w.next()
			if err != nil {
				return err
			}
		}
	}

	return nil
}

func (w *actionWatcher) updateStatus() bool {
	completedCount := 0
	jobsCount := 0
	updated := false

	for _, run := range w.action.Runs {
		for _, job := range run.Jobs {
			if job.Status.IsComplete() {
				completedCount += 1
			}
			jobsCount += 1

			var updateStatus bool
			if status, ok := w.statusMap[job.ID]; ok {
				updateStatus = status != job.Status
			} else {
				updateStatus = true
			}

			if updateStatus {
				updated = true
				w.statusMap[job.ID] = job.Status
			}
		}
	}

	w.completed = completedCount == jobsCount
	return updated
}

func (w *actionWatcher) display() {
	for range w.lastLines {
		term.CursorUp()
	}

	w.lastLines = 0
	for _, run := range w.action.Runs {
		fmt.Fprintln(os.Stderr)

		style := color.New(color.Bold, color.Underline)
		fmt.Fprintf(os.Stderr, "%s\n", style.Sprint(run.Name))

		w.lastLines += 2

		pad := 0
		for _, job := range run.Jobs {
			if len(job.Name) > pad {
				pad = len(job.Name)
			}
		}
		pad += 1

		for _, job := range run.Jobs {
			fmt.Fprintf(os.Stderr, "%-*s %s\n", pad, job.Name, job.Status.ColoredString())
			w.lastLines += 1
		}
	}
}

func (w *actionWatcher) next() error {
	time.Sleep(time.Millisecond * 100)
	currentAction, err := w.api.GetAction(w.req)
	if err != nil {
		return err
	}
	if currentAction == nil {
		return errors.New("action was removed during watching")
	}

	w.action = currentAction
	return nil
}
