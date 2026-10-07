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

`fmt` / `clippy` stay local closeout unless added to CI later.

## CI (currently disabled)

- `.github/workflows/rust.yml` is currently **disabled_manually** in GitHub Actions (not renamed or deleted). Re-enabling is an optional follow-up.
- When enabled, the workflow has:
  - `test` on `ubuntu-latest`: `cargo build --verbose` then `cargo test --verbose`
  - `native` on a dedicated runner (push/workflow_dispatch only, not PRs): release build then `./scripts/ci-hook.sh` (honors `BTCMON_CI_HOOK`)
- Do not put hostnames, LAN paths, or deploy targets in this repository.
