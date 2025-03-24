package create

import (
	"fmt"

	"github.com/fioncat/roxide/cmd"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/repoutils"
	"github.com/fioncat/roxide/pkg/term"
	"github.com/spf13/cobra"
)

func newTag() *cobra.Command {
	var opts tagOptions

	c := &cobra.Command{
		Use:   "tag [NAME]",
		Short: "Create a new tag",

		Args: cobra.MaximumNArgs(1),

		ValidArgsFunction: cmd.NoneCompletion,
	}

	c.Flags().StringVarP(&opts.rule, "rule", "r", "", "The rule to apply on the tag")

	return cmd.Build(c, &opts)
}

type tagOptions struct {
	name string

	rule string
}

func (o *tagOptions) Complete(c *cobra.Command, args []string) error {
	if len(args) > 0 {
		o.name = args[0]
	}
	return nil
}

func (o *tagOptions) Run(ctx *context.Context) error {
	repo, err := repoutils.MustGetCurrentRepo(ctx)
	if err != nil {
		return err
	}

	err = ctx.SetRepo(repo)
	if err != nil {
		return err
	}

	if o.name != "" {
		return o.createTag(ctx, o.name)
	}

	tag, err := git.GetLatestTag(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	var rule string
	if o.rule != "" {
		var ok bool
		rule, ok = ctx.Config.GetTagRule(o.rule)
		if !ok {
			return fmt.Errorf("cannot find tag rule %q in config", o.rule)
		}
	} else {
		rules := ctx.Config.TagRules
		if len(rules) == 0 {
			return fmt.Errorf("no tag rule found in config")
		}

		items := make([]string, 0, len(rules))
		for _, rule := range rules {
			items = append(items, rule.Name)
		}

		idx, err := ctx.Selector.Select(items)
		if err != nil {
			return err
		}

		rule = rules[idx].Rule
	}

	newTag, err := tag.ApplyRule(rule)
	if err != nil {
		return err
	}

	term.PrintInfo("Apply rule %q on %q", rule, tag)
	err = term.Confirm("Do you want to create tag %q", newTag)
	if err != nil {
		return err
	}

	return o.createTag(ctx, string(newTag))
}

func (o *tagOptions) createTag(ctx *context.Context, tag string) error {
	gitCmd := git.WithPath(ctx.GetRepoPath())

	err := gitCmd.Run("tag", tag)
	if err != nil {
		return err
	}

	err = term.Confirm("Do you want to push tag %q to remote", tag)
	if err == nil {
		return gitCmd.Run("push", "origin", tag)
	}

	return nil
}
