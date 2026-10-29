# Design

> This is a draft. It says how limu-rs is meant to be built before any of it is, and parts of
> it will change as the first diagrams are ported. Where it says "is", read "is to be".

This document is for people who work on limu-rs. It says what each stage owns, how the
crates are split, how a scene is drawn, and how the port is checked against Pele.

limu-rs reads Mermaid diagrams and lays them out in Rust, and backends draw what it lays
out. Most of it is ported from [Pele](https://github.com/obsidianmd/pele), the Mermaid
renderer of the Obsidian team, as [limu](https://github.com/firede/limu) ports it to Go.
limu-rs is a project of its own. It follows Pele, takes limu's port as a guide to how Pele
reads, and keeps limu's words for things where they serve, but it is free to change them.

## Stages

```
text ──parse──▶ model ──layout──▶ Scene ──backend──▶ SVG, a bitmap, a window, a terminal
                         ▲                  ▲
                 layout::Options        style::Look
         (measurer, font size, width,  (theme, overrides, and the
          padding, grid, routing:       backend's own drawing:
          where things are)             how they look)
```

Anything that changes where things are, or how large they are, belongs to layout, and
changing it lays the diagram out again. Everything else is style, which is settled only
when a scene is drawn, so changing it only redraws. Layout never decides a color, and style
never moves anything.

## Crates

| Crate | What it holds | Depends on |
|---|---|---|
| `limu-scene` | What a backend needs and nothing more: the scene, style, the types of laid-out text and the `Measurer` trait, the default drawing over a `Canvas`, hit testing, and the SVG writer. | `std` |
| `limu` | Parsing and layout of every diagram type, and what they share. It re-exports `limu-scene`. | `limu-scene` |
| `limu-tiny-skia` | Draws a scene into a tiny-skia pixmap, for PNGs and for the golden images of the default drawing, and measures with the fonts it draws in. | `limu-scene`, tiny-skia, rustybuzz |
| `limu-coregraphics` | Draws a scene into a `CGContext` the host gives, and measures with CoreText. | `limu-scene`, objc2's Core Graphics and Core Text |
| `limu-gpui` | Draws a scene in a GPUI window, and measures with GPUI's text system. | `limu-scene`, gpui-fast |
| `limu-ascii` | Draws a scene as text in a grid of cells, for terminals and TUIs, and measures in cells. | `limu-scene` |

`limu-scene` and `limu` use the standard library alone. A backend that needs another crate
is a crate of its own.

The workspace forbids unsafe code. Core Graphics and Core Text can't be called without it,
so limu-coregraphics denies it instead and allows it in one module, which wraps each call
in a function that can't misuse it and says why the call is sound. The crate is empty on
platforms other than Apple's, so the workspace builds everywhere. Both core crates have an optional `serde` feature that serializes the
scene and the style, so a host can carry a scene to another process or language.

The split puts what changes rarely apart from what changes often. The scene is a contract
between layout and backends, and it should change only when that contract does. Layout
changes each time limu-rs follows a new Pele. A backend depends on `limu-scene` alone, so it
neither compiles the parsers nor moves with every release of `limu`. A backend that draws
by role and wants the diagrams' role constants depends on `limu` too, or matches the role's
name.

`limu` has a feature for each diagram type, all on by default. Text of a type whose feature
is off is an unsupported diagram, as text of no type is. This takes the place of Pele's
entries and lazy loading, which split its bundle for the web.

## What limu-rs follows of Pele

limu-rs is for native hosts, so it follows the part of Pele that does not depend on a
browser.

- **Ported and checked to the bit.** The diagrams' parsers and layouts (`src/diagrams`),
  the layered, compound and lane layouts (`src/layout`), laying out labels and Pele's
  measurer of Arial's advances (`src/text`), the geometry of shapes, edges, clusters and
  titles (`src/svg/shapes.ts`, `edges.ts`, `cluster.ts`, `title.ts`), front matter,
  directives, detection and errors (`preprocess.ts`, `detect.ts`, `core.ts`, `errors.ts`),
  and JavaScript's reading of text, links and YAML (`src/util`).
- **Followed in meaning.** How Pele writes SVG (`src/svg/builder.ts`, `root.ts`) matters
  only in what it draws, which the scene and the default drawing must keep. The tokens of
  its theme and its choice of a color on a fill (`theme.ts`, `ink.ts`) are followed in
  `style`.
- **Not ported.** Mounting in a page and zooming (`mount.ts`, `zoom.ts`) are a host's. What
  they solve has a native form: a host that lays a diagram out again when its width changes
  sets `max_width`, and one whose fonts change lays it out again with the new measurer.
  Lazy loading (`lazy.ts`, `src/entries`) is the diagram features. The website is Pele's.
  Math in labels, which Pele typesets through a function the host gives, is not ported
  yet, as limu has not ported it. A label with `$$` in it is laid out as text.

Native hosts need some things Pele has no part in, and these are limu-rs's own: a measurer
the host gives, a grid to snap to that may differ across and down, hit testing, a scene's
title and description for the platform's accessibility, and drawing without SVG.

## The scene

A `Scene` is a list of items in the order they are drawn, with the diagram's type, its
size, the font size of text with no size of its own, and its title and description for
assistive technology.

An `Item` holds what every item has, and its `kind` says what sort of thing it is. Only an
edge has something no other kind has, its route and the markers at its ends, so only
`Kind::Edge` carries anything.

```rust
pub struct Item {
    pub kind: Kind,
    pub role: Role,              // "flowchart.node"
    pub shape: Option<Shape>,    // "diam"
    pub id: Option<String>,      // the id the diagram's text gave it
    pub class: Vec<String>,      // the classes its author gave it
    pub tags: Vec<String>,       // what the diagram says it is beyond its role
    pub bounds: Rect,
    pub parts: Vec<Part>,
    pub label: Option<Placed>,
    pub picture: Option<Picture>,
    pub style: Style,            // the diagram's own
    pub author: Style,           // what classDef and style lines set
    pub link: Option<String>,
    pub tooltip: Option<String>,
}

pub enum Kind { Node, Group, Edge(Edge), Text, Mark }

pub struct Edge { pub route: Vec<Point>, pub ends: [Option<Marker>; 2] }
```

The split was settled from what limu's scenes hold across every render fixture. Shapes,
parts, labels and pictures turn up on more than one kind, so they are fields of every item.
A text item's bounds are the box the diagram sets its label in, as an edge label's patch,
or else the box its label is drawn in, so that they always say where the item is.

- A `Part` is an `Outline` and the `Painting` it takes from the item's style. An outline is
  a path, or a box whose corners may take the style's radius. A box with the theme's radius
  stays a box, so the radius stays a matter of style. In limu a part has both a path and a
  box, and its box means nothing when its path is set.
- An edge's ends are `Option<Marker>`. There is no empty marker type, and `MarkerType` is
  an enum of every marker the diagrams draw, each with its trim and its outline. The table
  is in `limu-scene`, since a backend draws the outlines.
- `Placed` is a label laid out (`Label`, lines of spans, already measured) with the point it
  is drawn about, its alignment and its rotation. An item has at most one label. Something
  with more than one, as a sequence diagram's frame with its kind and its condition, is
  several items, with the shape first.
- `Role` and `Shape` are names, not enums, since there are many and they are what overrides
  and hosts key on. Each diagram's module in `limu` exports its roles as constants.
- Paths have four kinds of step (move, line, quadratic and cubic curves) and can close. An
  arc becomes cubic curves as it is added, one for each quarter turn or less.

Every number in a scene that `limu` gives is finite. Where Pele would draw NaN, layout gives
an error instead.

`Scene::snap` moves everything onto a grid, which may differ across and down, as the cells
of a terminal do. In the path of an edge, a node, a group or a text item, each control point
of a curve moves with the point it leans on, so a rounded corner still turns from one line
into the next. A path of curves alone, such as a circle, and the path of a mark, such as an
icon, move as a whole.

A backend with a look of its own draws an item from its role, shape, bounds and, for an
edge, its route, and ignores its parts.

### Painting

A part's `Painting` says which of the style's colors it takes, so a part keeps its role
when a theme changes them. The paintings are limu's (`FillAndStroke`, `StrokeOnly`,
`FillOnly`, `Solid`, `Hollow`, `Shade`, `Dot`), and they are reviewed with the rest of the
scene's words before the first release.

## Style

Style has three levels, from the least a host does to the most.

1. A **theme** holds the colors and measures every diagram takes its look from, by token.
   The tokens are `Bg`, `Fg`, `Muted`, `Line`, `Surface`, `SurfaceAlt`, `Border`, `Accent`,
   `Critical`, eight series, a corner radius and a stroke width.
2. **Overrides** lay a `Style` for a role over the diagram's own style for its parts.
3. A backend's **own drawing** replaces how it draws an item, and can still call the
   default.

A part's style cascades from the theme's measures, through the diagram's own style for the
part and the overrides for its role, to what the author set in the diagram's text. Each
field of a `Style` is an `Option`, and `None` takes what is under it. The fill and stroke
opacities cascade apart from the paints, as CSS cascades fill-opacity apart from fill.

A fill the author sets can be dark, and text and lines drawn in the theme's colors would be
lost on it. Pele picks a color for what lies on such a fill, the theme's foreground or
background, whichever stands out more, and limu-rs keeps that choice in style, since it
changes no position. A diagram says only what lies on which fill, and the style picks the
color when the scene is drawn.

`Look::resolve` settles a part's style into what a backend draws with. Each color it gives
is an `Ink`, the color settled and the paint it came from. A native backend draws the
color. The SVG writer writes a theme's paint as a CSS variable with the color as its
fallback, so a page can restyle a diagram without drawing it again.

## Drawing

limu draws a scene twice over, once in its svg package and once for MyGo, and each has its
own copy of how parts, markers, labels and pictures are drawn. limu-rs writes that once.

`limu-scene` has a `Canvas` trait of the few things a backend does itself, and a default
drawing that draws an item with them.

- Fill a path and then stroke it, in one call, so the SVG writer writes one element for
  both. A pen has a width, a dash, a cap and a join, and corners are mitred to SVG's
  initial limit. A box and a marker's part are paths unless the canvas draws them its own
  way, as the SVG writer writes a `<rect>` and gives markers a class.
- Draw a run of text, its spans one after another from where it is anchored, in their
  faces and at their size, filled or stroked as a halo. The default drawing draws a
  label's halo under all of its lines before it draws the text.
- Begin and end a layer drawn at an opacity. A canvas that has no layers says so, and the
  default drawing then lays the opacity on each color, as limumygo does.
- Begin and end a turn about a point, for a label that is rotated.
- Begin and end the scene and each item, so the SVG writer can write the document, and an
  item's group, classes, link and tooltip. Native canvases ignore them.
- Draw an icon by name and an image by address. A canvas that can't draws nothing, and
  labels left room only for the icons the measurer was told of.

The default drawing places a label's lines from its point, alignment and rotation, turns
boxes into paths where a canvas has no box of its own, and draws markers from their
outlines in the color of the edge's line. Every backend draws the same scene the same way,
as far as its canvas lets it, and the differences that remain belong to the canvas.

A host that wants its own look for some items gives `draw_with` a hook. The hook receives
the item, its resolved style and a `Painter`, whose `item`, `parts`, `ends`, `label` and
`picture` draw the default's pieces, and whose canvas it may draw on itself. The hook
draws inside the item and inside the layer of its opacity, so the style it is given is
always opaque. The SVG writer is a canvas like any other, and its hook writes markup of
its own through the writer.

`limu-ascii` is the other kind of backend. A grid of cells can't fill a curve, so it draws
from roles, shapes, bounds and routes, in box-drawing characters, and lays the diagram out
with a measurer that counts cells. What the scene and layout must give it for that, such as
the height of a line in cells, is settled when it is built.

## Hit testing

`Scene::item_at` gives the index of the item at a point, the one drawn last of those
there. An item is
there when its bounds or its label's box hold the point, and an edge, or an item drawn only
in lines, when one of its lines passes within a slop of the point or one of its markers
holds it. It lives in `limu-scene`, so every host that shows a scene can point into it.

## Parsing and layout

```rust
let diagram = limu::parse(src, &ParseOptions::default())?;
let scene = diagram.layout(&layout::Options::default())?;
// or, in one call:
let scene = limu::layout(src, &layout::Options::default())?;
```

`parse` takes what Pele's parse takes, the limits for the text and configuration beneath
the diagram's own. Configuration is a `Value`, JSON's kinds of value with an object's keys
in the order they were written, as JavaScript keeps them. A `Diagram` is `Send` and `Sync`,
and laying it out changes neither its model nor its configuration, so a host lays one
diagram out as often as it likes, from as many threads as it likes.

The models of the diagrams are private for now. A host gets from a diagram its type, as
limu-rs and as Mermaid name it, its title and its configuration, and the rest from the
scene. A model is made public when a host needs it, so that the public API stays what hosts
use.

`layout::Options` holds what changes where things are: the measurer and the font size, the
icons the host can draw, the width the host has and whether a diagram may turn to fit it,
the padding about a diagram (8 unless the host or a treemap gives one, as Pele's), a grid
to snap to, how edges are routed, which parts are set in the monospace face, which links
and images are let through, and the present and the time zone of a gantt chart.

The standard library knows no time zones, so the time zone is a trait the host implements,
with UTC and fixed offsets built in. A host that wants the machine's zone, as JavaScript
reads dates in it, implements it with a crate such as jiff.

Errors are a `limu::Error`, with a code (unsupported diagram, syntax, semantic or limit), a
message, the diagram's type where it is known, and the place in the text as written, its
line and column counted as Pele counts them, in UTF-16 code units.

## Porting from Pele

Parsing, layout and the geometry of shapes are ported from Pele function for function,
keeping Pele's names in Rust's case (`layoutLabel` is `layout_label`), so the two can be
read side by side. A function ported from Pele names its source in its comment ("as Pele's
layoutLabel does"). limu's port is the guide to how Pele reads in a language with types,
and where limu-rs is built otherwise, `docs/design.md` says so.

Where JavaScript's meaning affects what is measured or placed, the port keeps it.

- Strings are counted in UTF-16 code units. `\s`, `trim` and `toLowerCase` work as in
  JavaScript, with Unicode's spaces and the final sigma.
- Regular expressions are written out by hand, with the backtracking they would do.
- `Math.round` rounds halves up, and `Math.hypot`, the sines and the logarithms are V8's.
- Sorts are stable, and maps and sets iterate in insertion order.
- Numbers are written as JavaScript's `String(n)` writes them wherever they are compared
  as text.

Rust makes two of limu's chores unneeded. It never fuses a product into the sum it is in
unless the code asks with `mul_add`, as JavaScript never does, so a product needs no care
to stay apart. And where V8's fdlibm functions are fused, as clang builds them for arm64,
the port writes `mul_add`, which rounds once on every platform. So limu-rs computes what V8
on arm64 does on any machine, where limu matches it on arm64 alone.

A `&str` is always UTF-8, so text that comes as bytes is decoded as a browser decodes it,
each maximal subpart of an ill-formed sequence replaced with one U+FFFD, at the one entry
that takes bytes. A `&str` cannot hold a lone surrogate, which JavaScript's strings
can, so a `\u` escape of one in JSON or in a quoted YAML scalar reads as U+FFFD. Where
Pele cuts a string in UTF-16 code units between the halves of a pair, as a sequence
diagram's links are cut, the half kept reads as U+FFFD too.

A link's host name that is not ASCII is written in Punycode as a browser's URL parser
writes it, but only after the mappings std can make. Full widths and case are mapped, and
spaces, controls, format characters and private use are refused. The other mappings of
UTS #46, as ß and ligatures, need Unicode's tables, and a code point not yet assigned is
not refused. No case of the fixture depends on them.

Matching Pele to the last bit is how the port is checked, not what limu-rs is for. The
port goes as far as the function JavaScript calls, as with V8's sine. It doesn't copy a
platform's C library to match its last bit, as with `Math.pow`. Where such a difference
can't be avoided, it is noted, and the cases it affects are left out of the comparison.

## Checking against Pele

The tests compare with fixtures written from Pele's own sources by
[pele-oracle](https://github.com/firede/pele-oracle), which grew out of limu's oracle. Its
suites run Pele's TypeScript with Node over their cases and write what Pele makes of them,
the model it parses, the numbers its layouts give, and what its SVG draws, read back as one
line per thing drawn. Nothing in a fixture depends on the language of the port that reads
it.

pele-oracle publishes the fixtures of each version of Pele it follows as a release. limu-rs
pins one release, by its tag and its checksum, in `fixtures.toml`. `scripts/fixtures.sh`
downloads it once into `target/`, and the tests then run offline. The fixtures are never
committed, so the repository doesn't grow each time they are written anew. A change to the
suites is tried before it is released by pointing `LIMU_FIXTURES` at a local checkout's
output.

`fixtures.toml` also pins the fonts limu-tiny-skia's golden images are drawn in, Liberation
Sans and Mono, which are as wide as Arial and Courier New. `scripts/fixtures.sh` downloads
them with the fixtures and checks their checksum, so the images come out the same wherever
the tests run, and no font is committed. The images are compared with a little room, as
tiny-skia's arithmetic differs slightly from one processor to another.

The tests read the fixtures through `limu-fixtures`, a crate of the workspace that is never
published. It and the crates it reads JSON and gzip with are dev-dependencies, which a host
never builds.

A fixture of drawings holds the lines of its first cases in full and a digest of the lines
of each case after them, and a test compares the digest of the lines it writes from its
scene. Numbers are compared exactly. A difference in the last bit is a difference in the
port.

The port follows Pele 1.12.0, commit `8aff183`. Each fixture records the version and commit
it was written from, and the tests fail on a fixture of another. To follow a newer Pele,
pele-oracle writes the fixtures anew from it, limu-rs pins the new release, and what Pele's
diff changed is ported.

## Checking backends

A backend has no oracle. The scene it draws is checked against Pele already, and so is the
default drawing, through the SVG writer, which is a canvas like any other. What is left to
a backend is to draw a canvas's few calls well, and how that comes out in pixels depends on
its rasterizer, its fonts and its GPU, which no two backends share to the bit.

Every backend instead draws the same samples, from `limu-samples`, a crate of the workspace
that is never published. Some samples are built by hand, each to try one thing a canvas
does, such as caps, joins and dashes, every marker, every painting, faces, alignments and
turns of text, halos, layers at an opacity, curves and the nonzero rule, icons and images.
The rest are one diagram of each type, laid out by limu from an example of Pele's
documentation. Text in them is measured as Arial, so every backend draws the same scene.

Each backend compares its drawings with golden images of its own, written anew when a
change is meant and looked at before they are committed. limu-tiny-skia's are drawn in the
Liberation fonts and compared with a little room. A backend that draws pixels in another
way, as CoreGraphics does, compares with its own images where it runs, and also with
limu-tiny-skia's with more room, which catches a drawing that is wrong rather than
slightly different, such as one turned upside down. limu-ascii's golden files are text.

## Where limu-rs differs from limu

limu-rs keeps limu's words for things unless there is a reason to change them, and each
change is listed here with its reason.

- The scene is split from layout into a crate of its own, and the types of laid-out text
  and the `Measurer` trait live with it, so a backend that measures depends on it alone.
- Only an edge's route and ends belong to its kind, in `Kind::Edge`, so a backend never
  reads a route that means nothing. A part is a path or a box and never both. An edge's
  ends are optional markers rather than markers of an empty type, and `MarkerType` is an
  enum. A label is placed as one value with its point, alignment and rotation.
- A text item's bounds are never empty. Where the diagram gives its label no box of its
  own, they are the box the label is drawn in, where limu leaves them empty.
- A journey diagram's section label on its own is `journey.section-label`, apart from the
  section's box, `journey.section`. limu gives both one role, so an override could not
  tell them apart.
- A swimlane diagram's items have roles of their own (`swimlane.node`, `swimlane.edge`…),
  where limu gives them flowchart's, so that a host can style one without the other.
- A C4 element outside the system has its kind and "external" as two tags (`system_db`
  and `external`), where limu writes one (`external_system_db`). A host then matches every
  database, or everything outside, with one selector.
- Unset fields of a style are `None`.
- Backends share one default drawing over a `Canvas`, where limu's svg package and
  limumygo each have their own. Hit testing moves from limumygo to the scene.
- A draw hook draws inside the item's group, where limu's svg hook writes the group
  itself, so every item keeps its classes and its link whatever a host draws.
- The SVG writer writes each run of a label as a `<text>` of its own, where limu writes
  one `<text>` for the whole label. A browser places them alike.
- A grid to snap to may differ across and down.
- The diagrams' models are private.
- The time zone is a trait with UTC by default, as the standard library knows no zones,
  where limu reads the machine's.
- Fixtures are downloaded from pele-oracle's releases rather than committed.
- The text a parser is given is tied to where it came from by a value that lasts as long
  as the parse, on the parsing thread, as Pele keeps it in a variable of its module. An
  error found in that text is then placed in the text its author wrote. limu keys the
  texts by their addresses in a map that every thread shares, which Rust's strings, moved
  and borrowed as they are, do not allow.
- A class diagram's static member is underlined by the face of its label's span, which
  backends already underline in the text's color, where limu draws a line of its own under
  it.
- A use case diagram follows the host's routing, as limu's does, where Pele's draws its
  own curves.
