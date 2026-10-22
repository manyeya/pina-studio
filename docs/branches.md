# Branches and releases

## Branches

- **`main`** is what's been released. Nothing is committed to it directly: it moves only when a
  release is merged in from `dev` (or a hotfix from `fix/…`), and each of those is tagged.
- **`dev`** is where finished work comes together. It always builds and its tests pass, so a
  release can be cut from it at any time.
- **Work branches**, one piece of work each, branched from `dev` and merged back into it:
  - `feat/<what>`: something new (`feat/windows`, `feat/shared-modulators`).
  - `fix/<what>`: a bug fixed.
  - `refactor/<what>`: the code changed without changing what Pina Studio does.
  - `chore/<what>`: docs, scripts, tooling.
  - `wip/<what>`: unfinished work set aside, to be picked up and finished on its own branch.

Branch names are short, lower case, words joined by hyphens.

## Making a change

1. `git switch dev && git pull`, then `git switch -c feat/<what>`.
2. Make the change, with its tests. Add what someone using Pina Studio (or an agent driving it)
   will notice to **Unreleased** in `CHANGELOG.md`, under Added, Changed or Fixed.
3. Before merging: `cargo fmt`, `cargo clippy -p pina -p pina-engine --all-targets` with nothing to
   say, and `cargo test -p pina -p pina-engine` passing.
4. `git switch dev && git merge --ff-only feat/<what>` (rebase onto `dev` first if it moved), push
   `dev`, and delete the branch.

Commit messages say what changed for the person using it, in plain words: the first line on its
own, then why, if it isn't obvious. They don't name other products, and they don't credit tools.

## Releasing

1. On `dev`: set the version in `Cargo.toml` (`[workspace.package]`), and in `CHANGELOG.md` turn
   **Unreleased** into `## [x.y.z] - YYYY-MM-DD`, with a new empty Unreleased above it.
2. `git switch main && git merge --ff-only dev`, then `git tag vx.y.z` and push `main` and the tag.
3. The downloads: the tag's push builds the Windows installer and zip and puts them on its
   release (`.github/workflows/windows.yml`); add the Mac disk image from
   `scripts/release-mac.sh` (it lands in `dist/`). Every push to `dev`, `main` and `feat/*`
   also builds the Windows downloads, kept with the run, to try before a release.

The version follows semantic versioning: the third number for fixes, the second for features, the
first for changes that break songs or commands made with an earlier version.

## Hotfixes

A fix that can't wait for the next release: branch `fix/<what>` from `main`, merge it into `main`
(tagged as a patch release) and into `dev`.
