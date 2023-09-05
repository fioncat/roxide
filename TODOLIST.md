# TODOList

## v0.8.0

- [x] `info` command, show global info.
- [ ] `check` command, to check:
  - git version, must >= 2.20.
  - fzf must be exists.
  - shell type must be one of bash/zsh.
  - Config, metadata, workspace directories must be exists and have read/write permission.
- [x] `sync-rule` command, use pre-defined rules to sync repos.
- [ ] add `--dry-run` flag for `sync`.

## v0.7.0

- [x] Rebuild code, add Repo Selector module for common usage.
- [x] Alias, implement it in api module.
- [x] Rebuild `roxide sync` command, sync branches and tags.
