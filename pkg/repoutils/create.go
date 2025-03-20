package repoutils

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"

	"github.com/fioncat/roxide/pkg/config"
	"github.com/fioncat/roxide/pkg/context"
	"github.com/fioncat/roxide/pkg/db"
	"github.com/fioncat/roxide/pkg/git"
	"github.com/fioncat/roxide/pkg/lang"
	"github.com/fioncat/roxide/pkg/term"
)

func EnsureCreate(ctx *context.Context, thin bool) error {
	path := ctx.GetRepoPath()

	info, err := os.Stat(path)
	if err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to check repo path: %w", err)
	}

	if info != nil {
		if !info.IsDir() {
			return fmt.Errorf("repo path %q is not a directory", path)
		}

		return nil
	}

	remoteConfig := ctx.GetRemoteConfig()
	if remoteConfig.Clone == "" {
		term.PrintInfo("Create directory %q", path)
		err = os.MkdirAll(path, 0755)
		if err != nil {
			return fmt.Errorf("failed to create repo directory: %w", err)
		}

		gitCmd := git.WithPath(path)
		err = gitCmd.Run("init")
		if err != nil {
			return fmt.Errorf("failed to init git repo: %w", err)
		}

	} else {
		cloneURL := getCloneURL(ctx)
		gitCmd := git.New()
		gitCmd.Info("Cloning from %s", cloneURL)
		if thin {
			err = gitCmd.Run("clone", "--depth", "1", cloneURL, path)
		} else {
			err = gitCmd.Run("clone", cloneURL, path)
		}
		if err != nil {
			return fmt.Errorf("failed to clone repo: %w", err)
		}

		err = EnsureUserEmail(ctx)
		if err != nil {
			return err
		}
	}

	ownerConfig := ctx.GetOwnerConfig()
	for _, script := range ownerConfig.OnCreate {
		err = executeOnCreate(ctx, script)
		if err != nil {
			return fmt.Errorf("failed to execute on create script %q: %w", script, err)
		}
	}

	return nil
}

func EnsureUserEmail(ctx *context.Context) error {
	ownerConfig := ctx.GetOwnerConfig()
	path := ctx.GetRepoPath()
	gitCmd := git.WithPath(path)

	if ownerConfig.User != "" {
		gitCmd.Info("Set user to %s", ownerConfig.User)
		err := gitCmd.Run("config", "user.name", ownerConfig.User)
		if err != nil {
			return err
		}
	}

	if ownerConfig.Email != "" {
		gitCmd.Info("Set email to %s", ownerConfig.Email)
		err := gitCmd.Run("config", "user.email", ownerConfig.Email)
		if err != nil {
			return err
		}
	}

	return nil
}

func EnsureLanguage(ctx *context.Context) error {
	repo := ctx.GetRepo()

	language, err := lang.Detect(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if language != nil && (repo.Language == nil || *language != *repo.Language) {
		term.PrintInfo("Update repo language to %q", *language)
		updateOpts := db.UpdateRepositoryOptions{
			Language: language,
		}
		err = ctx.Database.UpdateRepo(repo.ID, updateOpts)
		if err != nil {
			return err
		}
	}

	if language == nil && repo.Language != nil {
		term.PrintInfo("Reset repo language")
		err = ctx.Database.ResetRepoLanguage(repo.ID)
		if err != nil {
			return err
		}
	}

	return nil
}

func EnsureGitRemote(ctx *context.Context) error {
	url := getCloneURL(ctx)
	origin, err := git.GetOriginRemote(ctx.GetRepoPath())
	if err != nil {
		return err
	}

	if origin == nil {
		gitCmd := git.WithPath(ctx.GetRepoPath())
		term.PrintInfo("Add origin remote %s", url)
		err = gitCmd.Run("remote", "add", "origin", url)
		if err != nil {
			return err
		}

		return nil
	}

	oldURL, err := origin.GetURL()
	if err != nil {
		return err
	}

	if oldURL == url {
		return nil
	}

	gitCmd := git.WithPath(ctx.GetRepoPath())
	term.PrintInfo("Set origin remote URL to %s", url)
	return gitCmd.Run("remote", "set-url", "origin", url)
}

func getCloneURL(ctx *context.Context) string {
	repo := ctx.GetRepo()
	ownerConfig := ctx.GetOwnerConfig()
	remoteConfig := ctx.GetRemoteConfig()

	return getCloneRaw(repo.Owner, repo.Name, remoteConfig, ownerConfig)
}

func getCloneRaw(owner, name string, remoteConfig *config.Remote, ownerConfig *config.Owner) string {
	var ssh bool
	if ownerConfig.SSH != nil {
		ssh = *ownerConfig.SSH
	}

	host := remoteConfig.Clone
	if ssh {
		return fmt.Sprintf("git@%s:%s/%s.git", host, owner, name)
	}

	return fmt.Sprintf("https://%s/%s/%s.git", host, owner, name)
}

func executeOnCreate(ctx *context.Context, script string) error {
	var env []string

	repo := ctx.GetRepo()
	env = append(env, fmt.Sprintf("REPO_REMOTE=%s", repo.Remote))
	env = append(env, fmt.Sprintf("REPO_OWNER=%s", repo.Owner))
	env = append(env, fmt.Sprintf("REPO_NAME=%s", repo.Name))

	remoteConfig := ctx.GetRemoteConfig()
	env = append(env, fmt.Sprintf("REMOTE_CLONE=%s", remoteConfig.Clone))

	cmd := exec.Command("sh", "-c", script)
	cmd.Env = env
	cmd.Dir = ctx.GetRepoPath()
	cmd.Stdin = os.Stdin

	var out bytes.Buffer
	if !term.Mute {
		term.PrintInfo("Execute on create script: `%s`", script)
		cmd.Stdout = os.Stderr
		cmd.Stderr = os.Stderr
	} else {
		cmd.Stdout = &out
		cmd.Stderr = &out
	}

	err := cmd.Run()
	if err != nil {
		if !term.Mute {
			return errors.New("on create script failed")
		}
		return fmt.Errorf("failed to execute on create script %q: %w, output: %q", script, err, out.String())
	}

	return nil
}
