# Commands without the extra frame

Short commands should be easy to scan beside their explanation. Keep the authored language, exact command and Copy action visible without spending a separate row on the header.

## Inspect the workspace

Show the current directory.

```sh
pwd
```

Check the workspace before making changes.

```bash
git status --short
```

## Keep larger examples readable

A longer command keeps a separate header whenever its source and controls cannot share a readable strip.

```bash
cargo test --workspace --all-targets --locked -- --include-ignored
```

A multi-line script keeps its ordinary code pane and original line breaks.

```sh
printf 'Checking the workspace\n'
git status --short
```

## Literal examples

Other languages retain their code pane rather than being mistaken for shell commands.

```rust
let ready = true;
```
