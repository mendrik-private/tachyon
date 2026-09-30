# Release checklist

Complete this checklist before running `scripts/release-github.sh`. The launcher
does not change versions, update documentation, or commit files.

1. Review every user-visible change since the previous release. Update the
   relevant pages under `docs/user-guide/`, including instructions, shortcuts,
   expected results, limitations, and screenshots that no longer match the app.
2. Update `crates/markdown-app/Cargo.toml`, the `markdown-app` entry in
   `Cargo.lock`, and the newest release in
   `packaging/io.github.mendrik_private.Tachyon.metainfo.xml` to the same
   version. Use an ISO 8601 release date in AppStream metadata.
3. Run `scripts/build-docs.sh` and inspect the generated guide in
   `target/docs-site/`. Run `scripts/check.sh` and the desktop/AppStream
   validation commands documented in `packaging/README.md` when application or
   packaging inputs changed.
4. Commit the complete release, push `main`, and wait for the Documentation
   workflow's GitHub Pages deployment for that exact commit to succeed. Keep a
   link to the workflow run or deployed guide with the release record.
5. From a clean `main` checkout at that commit, run
   `scripts/release-github.sh`. It verifies the version metadata, confirms HEAD
   is on `origin/main`, verifies the exact commit has a successful
   `github-pages` deployment, refuses an existing local or remote version tag,
   creates an annotated tag, and pushes it.
6. Follow the Release workflow. It repeats the tag, version, main-ancestry, and
   Pages checks; rebuilds the guide; runs the application checks; builds the
   release binary; and creates the Linux x86_64 archive. Its final job stages
   assets on a draft, then automatically publishes the release after every gate
   succeeds. It marks semantic prerelease versions as GitHub prereleases and
   refuses to modify an already-published release.

The workflow can resume an existing draft for the same tag by replacing its
assets and publishing it after all gates pass. Delete an unwanted draft before
retrying if its notes or ownership need manual correction. Never move or reuse a
published release tag; prepare a new version instead.
