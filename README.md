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
> limu-rs is young, and its API will change in any 0.x release.

## Using it

limu-rs needs Rust 1.95 or later. Add `limu`, and the backend you draw with.

```sh
cargo add limu
cargo add limu-tiny-skia
```

```rust
let scene = limu::layout("flowchart LR; a --> b", &Default::default())?;
let svg = limu::scene::svg::render(&scene, &Default::default());
```

[limu.firede.com](https://limu.firede.com) has the guides, the examples of every type and a
playground, for limu in Go and for limu-rs in Rust. [docs.rs](https://docs.rs/limu)
documents every name.

## Crates

| Crate | What it does |
|---|---|
| `limu` | Parses diagrams and lays them out as scenes. |
| `limu-scene` | The scene, its style, the drawing every backend shares, hit testing and SVG. A backend depends on it alone. |
| `limu-tiny-skia` | Draws a scene into a bitmap. |
| `limu-coregraphics` | Draws a scene with Core Graphics, and measures text with Core Text. |
| `limu-gpui` | Draws a scene in a GPUI window, and measures text with GPUI's text system. |
| `limu-ascii` | Draws a scene as text in box-drawing characters, for terminals. |

Every backend takes its options with a look, the fonts it draws in, and the icons, as
`limu_scene::Icons`, and images it can draw. `limu_ascii::supports` says which diagrams can be
drawn as text.

The core crates, `limu` and `limu-scene`, use the standard library alone, with serde behind
an optional feature.

## Examples

Each reads a Mermaid diagram from standard input.

```sh
echo 'flowchart LR; a --> b' | cargo run -p limu --example svg > diagram.svg
echo 'flowchart LR; a --> b' | cargo run -p limu-ascii --example terminal
echo 'flowchart LR; a --> b' | cargo run -p limu-tiny-skia --example png -- font.ttf diagram.png
```

`svg` writes SVG, measuring text as Arial. `terminal` lays the diagram out for a terminal's
cells and draws it in box-drawing characters, or in ASCII with `--ascii`. `png` lays the
diagram out with a font you give and draws it in that font.

## Development

```sh
./scripts/verify.sh
```

This downloads the fixtures the tests compare with and the fonts the golden images are drawn
in, once, and runs rustfmt, clippy, the docs and the tests. The fixtures are written from
Pele by [pele-oracle](https://github.com/firede/pele-oracle).
limu-gpui is a workspace of its own, checked by `./scripts/verify-gpui.sh`, and
`cargo run --example viewer` in `crates/limu-gpui` shows the samples every backend draws.
`cargo run -p limu-gallery` writes a page that sets them side by side, the golden file of
each backend next to the scene's SVG, and shows where a backend's tests drew a sample
otherwise. Open it with `?changed` after its address to see only those.
`./scripts/playground.sh --serve` builds a page at http://localhost:8000 that lays out any
diagram as you type it, and shows it as SVG, as terminal text, or as its scene.
[docs/design.md](docs/design.md) explains how limu-rs is built and how it's checked against
Pele, and [AGENTS.md](AGENTS.md) has the rules for changing it.

## License

MIT. See [LICENSE](LICENSE), which also includes Pele's license.
