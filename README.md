# roxide

roxide is a git repositories manager CLI, it is easy to use, and very fast!

roxide has good support for terminal use, including autocompletion and fuzzy search based on [fzf](https://github.com/junegunn/fzf).

**Now roxide does not support Windows system**

## Install

Install roxide directly:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/fioncat/roxide/HEAD/install.sh)"
```

If you have Rust installed, you can build it from source:

```bash
git clone https://github.com/fioncat/roxide.git /path/to/roxide
cd /path/to/roxide
make install
```

## Config

Please save config file to `$HOME/.config/roxide.toml`, or you can use command `roxide config` to create and edit config file directly.

See: [config.toml](config/config.toml).

## Shell Support

Now we support `zsh` and `bash`, it is recommended to use `zsh` for a better experience.

To enable completion and autojump, you need to add roxide init script to your shell profile:

```bash
source <(roxide init $(basename $SHELL))

# Optional, use `zz` to implement quick jump.
alias zz="rox home"
```

Then you can use `cmd` in config file to autojump, default is `rox`.

For example, use the following command to jump to roxide project:

```bash
rox home github fioncat/roxide
```

## Usage

You can use `<Tab>` to complete command at any time!

Search global repos and jump:

```bash
rox home -s
```

Search repos under a remote and jump:

```bash
rox home github -s
```

Search repos under a owner and jump:

```bash
rox home github fioncat/
```

Global fuzzy matching and jump:

```bash
rox home rox # Will jump to repo whose name contains 'rox'
```

Fuzzy matching under remote and jump:

```bash
rox home github rox # Will jump to whose name contains 'rox' in github
```

Search and remove a repo from disk:

```bash
rox remove github fioncat/
rox remove github fioncat/roxide
rox remove rox
```

Remove all repos under an owner:

```bash
rox remove -r github fioncat/
```

For more commands and usages, please see: `rox -h`.
