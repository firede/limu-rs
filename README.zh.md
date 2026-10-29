# limu-rs

[English](README.md) | 简体中文

limu-rs 是一个用 Rust 写的图表布局引擎。它读取 Mermaid，绘制交给你的程序。程序可以画在窗口
里、终端里，可以输出 SVG，也可以画成自己的风格。

limu-rs 的大部分代码移植自 Obsidian 团队的 Mermaid 渲染器
[Pele](https://github.com/obsidianmd/pele)。Pele 直接输出 SVG，limu-rs 则早一步停下，停在
场景（scene），也就是以数据表示的排布结果，再由后端绘制场景。limu-rs 的测试把它的结果和
Pele 的逐个数值比对。它是 [limu](https://github.com/firede/limu) 的 Rust 版本，limu 用 Go
做同样的事。

> [!WARNING]
> limu-rs 正在构建中，还不能排布图表。0.x 版本之间 API 随时可能变化。

## crate

| crate | 作用 |
|---|---|
| `limu` | 解析图表，排布成场景。 |
| `limu-scene` | 场景、样式、各后端共用的绘制、命中测试和 SVG。后端只依赖它。 |
| `limu-tiny-skia` | 把场景画成位图。 |
| `limu-coregraphics` | 用 Core Graphics 绘制场景，用 Core Text 测量文字。 |
| `limu-gpui` | 在 GPUI 窗口里绘制场景，用 GPUI 的文字系统测量文字。 |
| `limu-ascii` | 用制表符把场景画成文字，给终端用。 |

核心的 `limu` 和 `limu-scene` 只用标准库，serde 是可选的 feature。

## 开发

```sh
./scripts/verify.sh
```

它会下载一次测试比对用的 fixture 和 golden 图用的字体，然后运行 rustfmt、clippy、文档和全部测试。fixture 由
[pele-oracle](https://github.com/firede/pele-oracle) 从 Pele 生成。limu-rs 的构建方式以及它
如何与 Pele 比对，见 [docs/design.md](docs/design.md)，修改代码的规则见
[AGENTS.md](AGENTS.md)。limu-gpui 是单独的 workspace，用 `./scripts/verify-gpui.sh` 检查，
在 `crates/limu-gpui` 里运行 `cargo run --example viewer` 可以看到各后端共用的样例。
`cargo run -p limu-gallery` 会生成一个页面，把每个后端的 golden 文件和场景的 SVG 并排放在
一起，并标出后端测试画得与 golden 不同的样例。地址后加 `?changed` 打开时只显示这些样例。

## 许可

MIT，见 [LICENSE](LICENSE)，其中也包含 Pele 的许可。
