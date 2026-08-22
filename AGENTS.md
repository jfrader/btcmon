# Repository agent notes

## Linear workflow

- Track project work in Linear, project **Btcmon**: https://linear.app/gurisitosgames/project/btcmon-a16ce0025cb1
- New ideas are added as Linear issues. Agents pick up issues, log the work being done on each issue (status, notes, dates), and move completed issues to Done.
- Read the `linear-workflow` skill (global: `~/.config/opencode/skills/linear-workflow/SKILL.md`) before creating or updating any issue.

## Verify

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

GitHub-hosted CI is `ubuntu-latest` only. Any machine-local deploy is a runner hook (`BTCMON_CI_HOOK`), not a workflow job. Do not put hostnames, LAN paths, or deploy targets in this repository.
