# roxide

roxide is a git repositories manager CLI, it is easy to use, and very fast!

roxide has good support for terminal use, including autocompletion and fuzzy search based on [fzf](https://github.com/junegunn/fzf).

**Now roxide does not support Windows system**

## Install

Download from [release](https://github.com/fioncat/roxide/releases).

## Config

Add a basic config in `~/.config/roxide/config.toml`:

```
# ~/.config/roxide/config.toml

workspace = "~/dev"
data_dir = "~/.local/share/roxide"
default_branch = "main"
stats_ignore = []

[fzf]
name = "fzf"
args = []

[git]
name = "git"
args = []

[bash]
name = "bash"
args = []

[edit]
name = "vim"
args = []

```

Add a github remote config in `~/.config/roxide/remotes/github.toml`:

```toml
# ~/.config/roxide/remotes/github.toml

clone = "github.com"
icon = ""

[api]
provider = "github"
token = "${GITHUB_TOKEN}"
cache_hours = 24

[default]
sync = false
pin = false
ssh = false
user = "<your-user>"
email = "<your-email>"
on_create = []

[owners."<your-user>"]
sync = true
pin = true
ssh = true
on_create = []

[owners."<your-organization>"]
sync = true
pin = true
ssh = true
on_create = []
```

Add a test remote config in `~/.config/roxide/remotes/test.toml`

```toml
# ~/.config/roxide/remotes/test.toml
icon = ""

[owners.rust]
sync = false
pin = false
ssh = false
on_create = ["cargo-init"]

[owners.golang]
sync = false
pin = false
ssh = false
on_create = ["go-init"]
```

Add a hook `cargo-init` in `~/.config/roxide/hooks/cargo-init.sh` to init cargo when creating test rust repository:

```bash
# ~/.config/roxide/hooks/cargo-init.sh

cargo init
```

Add a hook `go-init` in `~/.config/roxide/hooks/go-init.sh` to init golang when
creating test golang repository:

```bash
# ~/.config/roxide/hooks/go-init.sh

go mod init ${REPO_NAME}

touch main.go
cat << EOF > main.go
package main

import "fmt"

func main() {
    fmt.Println("Hello world!")
}
EOF
```

You can add more scaffolding as needed.

## Shell Support

Now we support `zsh` and `bash`, it is recommended to use `zsh` for a better experience.

To enable completion and autojump, you need to add roxide init script to your shell profile:

```bash
source <(ROXIDE_INIT="$(basename $SHELL)" roxide)

# Optional, use `zz` to implement quick jump.
alias zz="rox home"
```

Then you can use `rox` command to implement autojump.

For example, use the following command to jump to roxide project:

```bash
rox home github fioncat roxide
```

## Usage

You can use `<Tab>` to complete command at any time!

Search repos and jump:

```bash
rox home
```

Search repos under a remote and jump:

```bash
rox home github
```

Search repos under a owner and jump:

```bash
rox home github fioncat
```

Global fuzzy matching and jump:

```bash
rox home rox # Will jump to repo whose name contains 'rox'
```

Jump to the last accessed repo:

```bash
rox home -
```

Jump to the last accessed repo under a owner:

```bash
rox home github fioncat -
```

Jump to the repo by url/ssh:

```bash
rox home https://github.com/fioncat/roxide.git
rox home git@github.com/fioncat/roxide.git
```

Search and remove a repo from disk:

```bash
rox rm repo github fioncat
rox rm repo github fioncat roxide
rox rm repo rox
```

Remove all repos under an owner:

```bash
rox rm repo -r github fioncat
```

For more commands and usages, please see: `rox -h`.
