# Agent Instructions

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **Run quality gates** (if code changed) - Tests, linters, builds
2. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
3. **Clean up** - Clear stashes, prune remote branches
4. **Verify** - All changes committed AND pushed
5. **Hand off** - Provide context for next session

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
