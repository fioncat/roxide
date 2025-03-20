package cmd

import (
	"errors"
	"fmt"
	"strings"

	"github.com/fioncat/roxide/pkg/choice"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/lang"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func NewAttach() *cobra.Command {
	var opts attachOptions
	c := &cobra.Command{
		Use: "attach REMOTE NAME",

		Short: "Attach current directory to a repository",

		Args: cobra.ExactArgs(2),

		ValidArgsFunction: BuildCompletion(OwnerCompletion),
	}

	return BuildWithForceNoCache(c, &opts)
}

type attachOptions struct {
	args []string
}

func (o *attachOptions) Complete(c *cobra.Command, args []string) error {
	o.args = args
	return nil
}

func (o *attachOptions) Run(ctx *context.Context) error {
	var isWorkspace bool
	if strings.HasPrefix(ctx.WorkDir, ctx.Config.Workspace) {
		isWorkspace = true
		id := repoutils.ParseWorkspacePath(ctx, ctx.WorkDir)
		if id == "" {
			return errors.New("you are in workspace, but not in any repository")
		}

		_, err := ctx.Database.GetRepo(id)
		if err != nil && !db.IsNotFound(err) {
			return err
		}
		if err == nil {
			return fmt.Errorf("the current directory has already been bound to %q", id)
		}
	}

	repos, err := ctx.Database.QueryRepos(db.QueryRepositoryOptions{
		Path: &ctx.WorkDir,
	})
	if err != nil {
		return err
	}
	if len(repos) != 0 {
		return fmt.Errorf("the current directory has already been bound to %q", repos[0].String())
	}

	ch := choice.New(ctx, o.args)

	repo, err := ch.One(choice.OneOptions{
		Mode:         choice.ModeSelect,
		SearchRemote: true,
	})
	if err != nil {
		return err
	}

	if !repo.NewCreated {
		return fmt.Errorf("repository %q has already been bound to %q, please detach it first", repo.String(), ctx.GetRepoPath())
	}

	err = term.Confirm("Do you want to attach current directory to %q", repo.String())
	if err != nil {
		return err
	}

	if !isWorkspace {
		repo.Path = &ctx.WorkDir
	}
	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	repo.InitScore()
	ownerConfig := ctx.GetOwnerConfig()
	if ownerConfig.Sync != nil {
		repo.Sync = *ownerConfig.Sync
	}
	if ownerConfig.Pin != nil {
		repo.Pin = *ownerConfig.Pin
	}

	language, err := lang.Detect(ctx.GetRepoPath())
	if err != nil {
		return err
	}
	repo.Language = language

	err = ctx.Database.InsertRepo(repo)
	if err != nil {
		return err
	}

	err = repoutils.EnsureGitRemote(ctx)
	if err != nil {
		return err
	}

	err = repoutils.EnsureUserEmail(ctx)
	if err != nil {
		return err
	}

	term.PrintInfo("Attached current directory to %q", repo.String())
	return nil
}
