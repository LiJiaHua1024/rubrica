//! Built-in document shown when no file is given.
//!
//! Not a marketing page: every block here exists to make one engine behaviour
//! visible, so that opening the app with no arguments is itself a regression test
//! you can read. Mixed-script runs carry no typed spaces around the Latin words,
//! because that is the case the glue model has to solve on its own.

pub const DOCUMENT: &str = r#"# Rubrica

一份**专注排版的** Markdown 阅读器。正文由 DirectWrite 测量，断行由自己的 Knuth–Plass 求解器决定，整个过程没有 webview。

## 全局断行

贪心算法逐行填满，然后把剩下的参差不给任何补偿。全局断行会同时权衡整段的所有合法断法，选择总代价最小的一组，所以每一行的拉伸量都接近——下面这段没有任何手工调整的空格，行尾依然是齐的。排版的质量通常不在单行上显出来，而是在连续十几行的节奏里；一行难看也许看不出来，十行参差一眼就能看见，而读者的眼睛并不会明确指出它不满的理由，它只是更早疲劳。

## 中西文混排

引擎在 `Han` 与 `Latin` 的边界自动插入约四分之一 em 的可调间距，作者不需要手动加空格：使用Rust编写、Direct2D绘制、以及Microsoft YaHei渲染中文。这些边界上的 glue 既能拉伸也能压缩，因此在两端对齐时它们会和中文ideograph之间的缝隙一起承担富余量。

- 中文ideograph之间使用可压缩 glue，所以没有空格的段落也能对齐
- 西文单词只在 UAX #14 允许的位置断行
- 行首禁则由 Unicode 表提供，不再另立一份容易漂移的清单

## 代码与标题

标题和代码块一律左对齐。把标题拉伸到整个栏宽是排版错误，所以 `BreakOptions::ragged` 会为这类块放弃两端对齐：

```rust
let opts = BreakOptions::new(column);
opts.ragged = true; // headings, code
```

> 引用块用左侧竖线标出层级，嵌套两层仍然可读。
>
> 段间距按正文行高的倍数给出，而不是固定像素，这样字号变化时节奏不会散掉。

## 度量

中文需要比西文更大的行高：ideograph 是方形的，西文靠下伸部之间的空隙分行，中文没有这个空隙。同一字号下，正文行高取 1.7 与 1.95 的差别就在这里。

1. 栏宽上限约 36 em
2. 正文 13.5 磅，约合 96 dpi 下的 18 像素
3. 大字号标题反向收紧字距

## 任务清单

勾选框由符号字体承担：正文里没有 U+2610 这个字形时，逐字的回退链会把它交给符号字体，而不是留下一个空心方块。

- [ ] 未完成的项目显示一个空盒
- [x] 已完成的项目显示一个打勾的盒

## 数学排版

行内公式 $\int_0^1 x^2 dx = \frac{1}{3}$ 与相邻的中文共用一条基线：它的盒子带着自己的升部与降部，所以这一行会被撑高，而不是把公式压进正文已有的行距里。

$$\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}$$

$$f(x) = \begin{cases} 1 & x \ge 0 \\ -1 & x < 0 \end{cases}$$

$$\begin{pmatrix} a & b \\ c & d \end{pmatrix} \begin{pmatrix} x \\ y \end{pmatrix} = \begin{pmatrix} ax + by \\ cx + dy \end{pmatrix}$$

上面每一个数字都取自字体自带的 MATH 表：分数线落在 axisHeight 上，分子按 numShift 上移，根号与括号由 glyph assembly 拼出需要的高度，而不是把一个字形纵向拉长。多行的公式也一样——行距取 mathLeading，整块以 axisHeight 为轴居中，所以矩阵和它旁边的分数线同高。

## 脚注

引用标记只把数字抬高，让它坐在它所属的那个词上方，而不是成为句子里的一个词：全局断行的代价函数[^dy]决定了每一行的富余量，重复引用同一个脚注得到的是同一个数字[^dy]。

[^dy]: 上标和公式的下标是同一套机制：渲染的每一段文字本来就带着自己的 drop，取负就是向上，所以行高会把抬起来的部分算进升部，不会被上面一行切掉。

[^margin]: 这一条没有任何引用指向它，仍然排在文末：作者写下的文字不该因为少了一个标记就消失。
"#;
