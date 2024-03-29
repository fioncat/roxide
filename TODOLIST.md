# TODOList

## v0.10.1

- [x] In `url`, expand the alias name.
- [ ] Support completion for `keyword` and `repo name`.
- [ ] Support `@[n]`, to select the `nth` visited repo. Usage: `rox home @3`, `rox home github @5`. The `n` can be defaulted to `5`.

## v0.10.0

- [x] Use `Cow`, to further reduce memory `clone` overhead.
- [x] Change the data structure of the database to HashMap to optimize query, update database version to `v3`.
- [x] Rebuild `snapshot` command, don't affect workspace.

## v0.9.0

- [x] `copy` command, copy current repo to another remote/group.
- [x] `home` support clone url (both HTTPS and SSH).
- [x] `home` support use keyword to do fuzzy searching on remote, usage: `rox home github xxx -s`
- [x] Support `labels`, the `sync`, `remove`, etc will use them to filter repos.
- [x] Update database to `v2` to support `labels`.
- [x] Workflow now support running commands in `docker` and `ssh`.
- [x] Rebuild config style, embed all config in one file.
- [x] Rebuild code, better hint.

## v0.8.0

- [x] `info` command, show global info.
- [x] `check` command, to check:
  - git version, must >= 2.20.
  - fzf must be exists.
  - shell type must be one of bash/zsh.
  - Config, metadata, workspace directories must be exists and have read/write permission.
- [x] `sync-rule` command, use pre-defined rules to sync repos.
- [x] add `--dry-run` flag for `sync`.

## v0.7.0

- [x] Rebuild code, add Repo Selector module for common usage.
- [x] Alias, implement it in api module.
- [x] Rebuild `roxide sync` command, sync branches and tags.
