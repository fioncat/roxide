# TODOList

## v0.9.0

- [ ] `copy` command, copy current repo to another remote/group.
- [x] `home` support clone url (both HTTPS and SSH).
- [ ] `home` support use keywork to do fuzzy seaching on remote, usage: `ro home github xxx -s`

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
