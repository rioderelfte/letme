# letme

[![CI](https://github.com/rioderelfte/letme/actions/workflows/ci.yml/badge.svg)](https://github.com/rioderelfte/letme/actions/workflows/ci.yml)

Run `letme test` in any project and it runs the right test command, whether
that means `cargo test`, `npm run test`, `vendor/bin/pest`, or all of them.

I work on a lot of projects in parallel: Rust, PHP, TypeScript, some with
Justfiles, some without. Every one has its own convention, and remembering
whether this particular repo tests with `vendor/bin/pest`, `composer test`,
or `npm run test:unit` costs a little time and focus, twenty times a day.

## Usage

`letme` maps whatever tooling it finds to a fixed set of canonical commands:

`install`, `test`, `e2e`, `lint`, `typecheck`, `format`, `build`, `clean`

```sh
letme test            # run the detected test command
letme lint test       # chain commands; stops on the first failure
letme lint --fix      # auto-fix what the linter can fix
letme format --check  # check formatting without writing
letme te              # unambiguous prefixes work too
letme ok              # your own alias, see Configuration
letme clean -i        # confirm each command before it runs
```

In a chain, commands that don't resolve for the current project are skipped
with a note. A single command that doesn't resolve is an error.

Run `letme` without arguments to see what it detected and what each command
would actually execute:

```console
$ letme
Detected ecosystems:
  • JavaScript (npm)

Available commands:
  letme clean
    → rm -rf node_modules [ecosystem script, npm]
  letme e2e
    → npm run test:e2e (playwright test) [ecosystem script, npm]
  letme format
    → npm run format (prettier --write .) [ecosystem script, npm]
  letme install
    → npm install [ecosystem script, npm]
  letme lint
    → npm run lint (eslint .) [ecosystem script, npm]
  letme test
    → npm run test (vitest run) [ecosystem script, npm]
  letme typecheck
    → npm run typecheck (tsc --noEmit) [ecosystem script, npm]

Aliases:
  letme ok → format, lint, typecheck, test
```

### Doctor

`letme doctor` runs quick health checks on the project: are dependencies
installed and newer than the lockfile, is there a `.env` when the project
ships a `.env.example`, are the required binaries on the PATH.

```console
$ letme doctor
  ✗ node_modules              missing → npm install
  ✗ .env                      missing → cp .env.example .env
```

The checks are file-based (existence and mtime comparisons), so they are fast
but not exhaustive. `doctor` always runs on its own: it can't be chained with
other commands or used in an alias, and it doesn't match by prefix.

## How detection works

Detection runs in three tiers. A more specific tier wins over the ones below
it, per command:

1. **Task runners.** A `Justfile` recipe, mise task, or nx target named like a
   canonical command overrides everything else for that command. If the same
   name exists in several task runners, Justfile wins over mise, and both win
   over nx.
2. **Ecosystem scripts.** Scripts from `package.json` (npm, yarn, or pnpm,
   picked by lockfile) and `composer.json`. Names match exactly or by prefix
   (`test`, `test:unit`), and the script content is inspected as a
   cross-check: a script named `test` that actually runs `playwright test`
   is classified as `e2e`, so `letme test` never kicks off a slow e2e suite
   by accident.
3. **Conventions.** `Cargo.toml`, plus known binaries in `vendor/bin` (pest,
   phpunit, phpstan, php-cs-fixer) and `node_modules/.bin` (vitest, jest,
   eslint, oxlint, prettier, biome, tsc, playwright, cypress).

If a project mixes ecosystems (say Rust plus JavaScript), `letme test` runs
the test commands of both.

`lint --fix` and `format --check` are detected the same way, from tasks or
scripts named like `lint:fix` or `format:check`. Without one, letme rewrites a
recognized plain script instead (`prettier --write .` becomes
`prettier --check .`). A variant never comes from a lower tier than its plain
command, so `format` and `format --check` always use the same formatter.

## Installation

Not on crates.io yet; install from the repo (needs a Rust toolchain):

```sh
cargo install --git https://github.com/rioderelfte/letme
```

## Configuration

Optional, lives at `~/.config/letme/config.toml`:

```toml
[aliases]
ok   = ["format", "lint", "typecheck", "test"]  # what I run before every commit
t    = ["test"]                                 # make the ambiguous "t" prefix work
ci   = ["lint", "typecheck", "test", "build"]
tidy = ["lint --fix", "format"]                 # an entry can carry its command's flag
```

Aliases may reference other aliases, and their names take part in prefix
matching. Colors can be themed with palette files; see
[docs/theming.md](docs/theming.md).

### Disabling commands per project

To keep a detected command from running in a specific repo, drop a
`.letme.local.toml` next to the code:

```toml
disable = ["format"]
```

Disabling `lint` also disables `lint --fix`.

## Status

I built this for my own daily work and it is early. Things to know:

- Supported ecosystems are the ones I use: JS/TS, PHP, Rust, plus Justfile,
  mise, and nx. Others may follow.
- Unix only for now.
- Detection looks at the current directory only. There is no walking up to
  the project root yet; monorepo awareness is limited to nx workspaces run
  from their root.
- `letme` executes what it detects. In a repo you don't trust, look at the
  info view before running anything.

## Why the name?

Because the commands read like sentences: `letme test`, `letme build`,
`letme clean`. You say what you want to do, and it figures out the rest.
