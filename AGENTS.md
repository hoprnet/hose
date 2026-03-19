# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

````bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
`

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
````

5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

## Development Environment (devenv)

This project uses [devenv](https://devenv.sh/) for reproducible dev environments via Nix.

- **Enter the shell**: `cd` into the project (direnv activates automatically) or run `devenv shell`
- **Toolchain**: Rust, Node.js 22, protobuf, SQLite, Deno, and treefmt are provided — do NOT install them manually
- **Pre-commit hooks**: `treefmt` and `clippy` run automatically on commit

### Available Scripts

| Command          | Description                                                |
| ---------------- | ---------------------------------------------------------- |
| `hose-dev`       | Build ReScript + start dev server (HTTP :8080, gRPC :4317) |
| `hose-res-build` | Compile ReScript modules → `static/js/`                    |
| `hose-res-watch` | Watch ReScript files and rebuild on change                 |
| `hose-gen`       | Send synthetic OTLP traces to local instance               |

### Key Environment Variables

- `PROTOC` — set automatically to the Nix-provided protobuf compiler
- `RUST_LOG` — defaults to `info,hose=debug` when using `hose-dev`

**CRITICAL RULES:**

- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
