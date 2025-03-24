package open

import (
	"errors"
	"fmt"
	"os"

	"github.com/fatih/color"
	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/remoteapi"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newMerge() *cobra.Command {
	var opts mergeOptions

	c := &cobra.Command{
		Use:   "merge [TARGET]",
		Short: "Open or create new MergeRequest (or PullRequest for GitHub)",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.BuildCompletion(cmd.BranchCompletion),
	}

	c.Flags().BoolVarP(&opts.upstream, "upstream", "u", false, "Upstream mode, only used for forked repo")

	return cmd.BuildWithForceNoCache(c, &opts)
}

type mergeOptions struct {
	target string

	upstream bool
}

func (o *mergeOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.target = args[0]
	}
	return nil
}

func (o *mergeOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}

	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	api, err := ctx.RemoteAPI(repo.Remote)
	if err != nil {
		return err
	}

	apiRepo, err := api.GetRepo(repo.Owner, repo.Name)
	if err != nil {
		return err
	}

	if o.upstream {
		if apiRepo.Upstream == nil {
			return errors.New("this repository does not have an upstream")
		}
	} else {
		apiRepo.Upstream = nil
	}

	target := o.target
	if target == "" {
		if apiRepo.Upstream != nil {
			target = apiRepo.Upstream.DefaultBranch
		} else {
			defaultBranch, err := git.GetDefaultBranch(ctx.GetRepoPath())
			if err != nil {
				return err
			}
			target = defaultBranch
		}
	}

	currentBranch, err := git.GetCurrentBranch(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if !o.upstream && currentBranch == target {
		return errors.New("cannot merge myself")
	}

	term.PrintInfo("Get merge from remote API")
	merge := &remoteapi.MergeRequest{
		Owner:    repo.Owner,
		Name:     repo.Name,
		Upstream: apiRepo.Upstream,
		Source:   currentBranch,
		Target:   target,
	}

	url, err := api.GetMergeRequest(merge)
	if err != nil {
		return err
	}

	if url != "" {
		return openURL(url)
	}

	err = term.Confirm("Do you want to create a new merge request")
	if err != nil {
		return err
	}

	gitRemote, err := repoutils.GetRemote(ctx, o.upstream)
	if err != nil {
		return err
	}

	commits, err := gitRemote.CommitsBetween(target)
	if err != nil {
		return err
	}

	var commitDesc string
	var init_title string

	switch len(commits) {
	case 0:
		return errors.New("no commits to merge")

	case 1:
		commitDesc = "1 commit"
		init_title = commits[0]

	default:
		commitDesc = fmt.Sprintf("%d commits", len(commits))
	}

	fmt.Fprintln(os.Stderr)
	fmt.Fprintf(os.Stderr, "About to create merge: %s\n", o.prettyMerge(merge))
	fmt.Fprintf(os.Stderr, "With %s\n", commitDesc)

	err = term.Confirm("Continue")
	if err != nil {
		return err
	}

	title, err := term.Input("Please input merge title", init_title)
	if err != nil {
		return err
	}

	term.PrintInfo("Creating merge request")
	url, err = api.CreateMergeRequest(merge, title, "")
	if err != nil {
		return err
	}

	return openURL(url)
}

func (o *mergeOptions) prettyMerge(merge *remoteapi.MergeRequest) string {
	if merge.Upstream != nil {
		return fmt.Sprintf("%s:%s => %s:%s",
			color.YellowString("%s/%s", merge.Owner, merge.Name),
			color.MagentaString(merge.Source),
			color.YellowString("%s/%s", merge.Upstream.Owner, merge.Upstream.Name),
			color.MagentaString(merge.Target))
	}

	return fmt.Sprintf("%s => %s",
		color.MagentaString(merge.Source),
		color.MagentaString(merge.Target))
}
