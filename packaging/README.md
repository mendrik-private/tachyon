# Linux packaging

The desktop identity is `dev.mineral.Markdown`. A distributor should install:

- the `mineral-markdown` release binary under `bin/`;
- `dev.mineral.Markdown.desktop` under `share/applications/`;
- `dev.mineral.Markdown.metainfo.xml` under `share/metainfo/`; and
- the icon tree under `share/icons/hicolor/`; and
- the MIT and Apache-2.0 license texts under `share/licenses/mineral-markdown/`.

The binary is Wayland-only by product contract. The bundled fonts and their
licenses are compiled into the executable from `assets/fonts/`.

HTML previews keep the bundled Spline fonts as their default and use installed
fonts through Fontconfig for additional scripts and emoji. Distributions need
fonts covering the languages they support; no font files are downloaded from
documents. The native multilingual validation uses installed Noto CJK, Arabic
and Color Emoji fonts. After installing or changing fonts, restart Mineral to
rebuild its process-local font collection and retained previews.

The application icon is the supplied lightning/Markdown PNG, preserved at its
original resolution. GNOME scales it to the requested launcher size. Its icon
name and desktop filename match the Wayland application ID.

Build and stage a complete installation under an explicit prefix:

```sh
cargo build --release --locked --bin mineral-markdown
packaging/install.sh /tmp/mineral-stage/usr
```

The installer rejects an empty prefix and `/`; it never selects a system or
user prefix implicitly. Validate the staged desktop integration with:

```sh
desktop-file-validate /tmp/mineral-stage/usr/share/applications/dev.mineral.Markdown.desktop
appstreamcli validate --no-net /tmp/mineral-stage/usr/share/metainfo/dev.mineral.Markdown.metainfo.xml
file /tmp/mineral-stage/usr/share/icons/hicolor/scalable/apps/dev.mineral.Markdown.png
```

AppStream currently reports the optional `url-homepage-missing` warning. No
public project homepage has been chosen, so the metadata intentionally avoids
publishing an invented URL.
