# Working on limu-rs

limu-rs reads Mermaid diagrams and lays them out in Rust, and backends draw what it lays
out. Read docs/design.md first. It says what each crate owns and why style is kept out of
layout.

## Rules

- `limu-scene` and `limu` use the standard library only, and `serde` behind their optional
  feature. A backend that needs another crate is a crate of its own.
- Layout never decides a color, and style never moves anything. A setting that changes the
  size of anything belongs in `layout::Options`.
- `limu-scene` holds what a backend needs and nothing more. Whatever only layout uses
  belongs in `limu`.
- Keep the public API to what hosts use. A type or function a host has no need of is
  `pub(crate)`.
- `./scripts/verify.sh` passes before a change is done.

## Porting from Pele

- Port function for function, keeping Pele's names in Rust's case, so the two can be read
  side by side. A function ported from Pele names its source in its comment ("as Pele's
  layoutLabel does"). limu's Go port is a guide to how Pele reads, not the source.
- Keep JavaScript's meaning wherever it affects what is measured or placed. That means
  strings counted in UTF-16 code units, `\s` and `trim` with Unicode's spaces, `Math.round`
  rounding halves up, V8's `Math.hypot` and trigonometry, stable sorts, and maps and sets
  iterating in insertion order. Write regular expressions out by hand, with the
  backtracking they would do.
- Rust never fuses a product into a sum on its own. Where V8's code is fused, as clang
  builds it for arm64, write `mul_add`. Don't write `mul_add` anywhere else.
- Every ported stage is checked against Pele, with the fixtures of the pele-oracle release
  `fixtures.toml` pins. Run `./scripts/fixtures.sh` once to download them. To test fixtures
  that are not released yet, set `LIMU_FIXTURES` to a local pele-oracle's output.
- Compare numbers exactly. A difference in the last bit is a difference in the port.
- Matching Pele to the last bit is how the port is checked, not what limu-rs is for. Go as
  far as porting the function JavaScript calls. Don't write code whose only use is to match
  a platform's library to the last bit. Where such a difference can't be avoided, note it
  and leave the cases it affects out of the comparison.
- The port follows Pele 1.12.0, commit 8aff183.
- Where limu-rs is built otherwise than limu, say so in docs/design.md, under "Where
  limu-rs differs from limu", with the reason.

## Writing

Comments and documents are plain and exact. Say what a thing does and why, in full
sentences, without selling it. In documents, write short, natural sentences, and avoid
semicolons and lists led by a colon in running prose. Commit messages say what changed, in
the imperative.
