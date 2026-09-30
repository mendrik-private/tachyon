# Plan the release notes

This project note keeps the launch checklist, a small implementation detail, and the review table together. Tachyon renders the Markdown directly while preserving the source as a portable text file.

## Today

- [x] Confirm the scope with the maintainers
- [x] Draft the installation steps
- [ ] Capture the final application screenshots
- [ ] Publish the release announcement

> [!TIP]
> Keep decisions close to the work they affect. A short note is easier to maintain than a separate status system.

## Preview command

```sh
python3 -m http.server --directory target/docs-site
```

Build the guide with `scripts/build-docs.sh` before serving it. The build and link checker catch missing assets before publication.

## Review

| Area | Owner | Status |
| :--- | :--- | :--- |
| Installation | Mina | Ready |
| First document | Rowan | In review |
| Troubleshooting | Sol | Drafting |

## Decision log

1. Publish the guide with the application release.
2. Keep examples short enough to copy and adapt.
3. Verify the packaged binary on a clean Linux environment.

The result should read like a useful project note first and a feature sample second.
