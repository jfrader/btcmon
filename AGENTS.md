# Repository agent notes

## Linear workflow

- Track project work in Linear, project **Btcmon** (private; reference issues only by bare ID, e.g. `GURI-1234`, never by URL).
- New ideas are added as Linear issues. Agents pick up issues, log the work being done on each issue (status, notes, dates), and move completed issues to Done.
- Read the `linear-workflow` skill (installed in your local agent's global skills) before creating or updating any issue.

## Verify

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs on GitHub-hosted runners (x86_64 and ARM64).
