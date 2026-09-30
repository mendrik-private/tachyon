#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
book_root="$repo_root/docs/user-guide"
cookbook_source="$repo_root/docs/markdown-layouts.md"
logo_source="$repo_root/assets/branding/tachyon-logo.png"
output_root="$repo_root/target/docs-site"

command -v mdbook >/dev/null 2>&1 || {
    echo "mdbook is required; install it with: cargo install mdbook --version 0.5.4 --locked" >&2
    exit 1
}
command -v ruby >/dev/null 2>&1 || {
    echo "ruby is required to prepare and check the documentation site" >&2
    exit 1
}

test "$(mdbook --version)" = "mdbook v0.5.4" || {
    echo "mdbook 0.5.4 is required for reproducible documentation builds" >&2
    exit 1
}

required_sources='book.toml
src/SUMMARY.md
src/index.md
src/install.md
src/first-use.md
src/search.md
src/editing.md
src/adaptive-layouts.md
src/technical-content.md
src/saving-and-recovery.md
src/appearance-and-accessibility.md
src/shortcuts-and-troubleshooting.md
src/assets/screenshots/PROVENANCE.md
src/assets/screenshots/tachyon-project-note-wide.png
src/assets/screenshots/tachyon-project-note-narrow.png
src/assets/screenshots/tachyon-project-note-dark.png
theme/tachyon.css'

printf '%s\n' "$required_sources" | while IFS= read -r source; do
    test -f "$book_root/$source" || {
        echo "Required guide source is missing: docs/user-guide/$source" >&2
        exit 1
    }
done
test -f "$cookbook_source" || {
    echo "Required guide source is missing: docs/markdown-layouts.md" >&2
    exit 1
}
test -f "$logo_source" || {
    echo "Required guide asset is missing: assets/branding/tachyon-logo.png" >&2
    exit 1
}

staging_root=$(mktemp -d "${TMPDIR:-/tmp}/tachyon-docs.XXXXXX")
cleanup() { rm -rf -- "$staging_root"; }
trap cleanup EXIT HUP INT TERM

cp -R "$book_root/." "$staging_root/"
mkdir -p "$staging_root/src/assets" "$staging_root/src/includes"
cp "$logo_source" "$staging_root/src/assets/tachyon-logo.png"

ruby - "$cookbook_source" "$staging_root/src/includes/markdown-layouts.md" <<'RUBY'
source, target = ARGV
markdown = File.read(source)
repository = "https://github.com/mendrik-private/tachyon"
markdown.gsub!(/\]\(\.\.\/([^)]+)\)/) do
  path = Regexp.last_match(1)
  view = path.end_with?("/") ? "tree" : "blob"
  "](#{repository}/#{view}/main/#{path})"
end
File.write(target, markdown)
RUBY

ruby "$repo_root/scripts/check-doc-site.rb" --self-test
rm -rf -- "$output_root"
mdbook build "$staging_root" --dest-dir "$output_root"
ruby "$repo_root/scripts/check-doc-site.rb" "$output_root"
