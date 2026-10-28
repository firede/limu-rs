# limu-rs

English | [简体中文](README.zh.md)

limu-rs is a diagram layout engine in Rust. It reads Mermaid and leaves the drawing to your
program, which can draw in a window, in a terminal, as SVG, or in a style of its own.

Most of limu-rs is ported from [Pele](https://github.com/obsidianmd/pele), the Mermaid
renderer made by the Obsidian team. Pele writes SVG. limu-rs stops one step earlier, at the
scene, which is the laid-out diagram as data, and its backends draw the scene. Its tests
compare its output with Pele's, number for number. It is the Rust sibling of
[limu](https://github.com/firede/limu), which does the same in Go.

> [!WARNING]
> limu-rs is being built and can't lay out a diagram yet. Its API will change in any 0.x
> release.

## Crates

| Crate | What it does |
|---|---|
| `limu` | Parses diagrams and lays them out as scenes. |
| `limu-scene` | The scene, its style, the drawing every backend shares, hit testing and SVG. A backend depends on it alone. |
| `limu-tiny-skia` | Draws a scene into a bitmap. |
| `limu-coregraphics` | Draws a scene with Core Graphics, and measures text with Core Text. |
| `limu-gpui` | Draws a scene in a GPUI window. |
| `limu-ascii` | Draws a scene as text, for terminals. |

The core crates, `limu` and `limu-scene`, use the standard library alone, with serde behind
an optional feature.

## Development

```sh
./scripts/verify.sh
```

This downloads the fixtures the tests compare with, once, and runs rustfmt, clippy, the docs
and the tests. The fixtures are written from Pele by
[pele-oracle](https://github.com/firede/pele-oracle). [docs/design.md](docs/design.md)
explains how limu-rs is built and how it's checked against Pele, and
[AGENTS.md](AGENTS.md) has the rules for changing it.

## License

MIT. See [LICENSE](LICENSE), which also includes Pele's license.
