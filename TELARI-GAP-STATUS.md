# TELARI-GAP-STATUS.md —— Rubrica × Telari 差距补齐台账

> 权威差距依据：`TELARI-GAP.md`（2026-10-16 调研，对照 Telari 0.6.3）。第五节"已领先"不计入目标。
> 本文件是唯一进度权威：会话上下文只是缓存；恢复只看本文件。事件发生（完成子步骤、跑完闸门、换子 agent）时立刻更新，不攒批。
> 建账：2026-10-16（冷启动）。最近更新：2026-10-16 G1 完成并提交（master 二轮复核通过）；当前条目切至 G2（待启动子 agent）。

## 0. 用户侧操作（照此执行）

- 暂停：等当前工具调用结束，按 Esc 中断当前轮次即可；直接关 TUI 也安全——进度在文件里，不在上下文里。
- 恢复：打开同一会话（`/sessions`）继续；上下文脏了就开新会话，把原始提示词再粘一次，master 检测到本文件会自动接上。
- 最短恢复指令：`继续 goal-driven 流程：读 TELARI-GAP-STATUS.md 恢复进度，按恢复区的"下一步动作"接着做，不要从头开始。`
- 硬约束：绝不 push；一个条目一个提交；不 amend / rebase / reset 已提交历史；不修改 `TELARI-GAP.md` 调研正文。
- 用户未提交的无关改动：`.freebuff/`（未跟踪目录）、`TELARI-GAP.md`（未跟踪，证据基线）——保留不动，已记账。

## 1. 恢复区（子 agent 运行时由子 agent 实时维护；master 复核后刷新）

- 当前条目：**G2 HTML 的语义处理**（进行中，待启动子 agent；先核实现状再动手）
- 上一条目结论：**G1 Mermaid 已完成**（2026-10-16 master 第二轮复核：四闸门亲跑全绿——test 248+36+79+117+53+4+7+64+19、clippy 默认+`--features pdf`、release build、`git diff --check`；抽查 `fixtures/mermaid-tree.md` → `2 fence(s), 2 drawn, 15 element(s), 12 link(s), 24 shape(s), 18 label(s)`、kind 显示 `tree view`；`mermaid-flow.md` 场景 338.6 x 732.1pt、计数不变、`bounds` 全绿；`mermaid-broken.md` 拒绝文案已含 tree view → 判定达成，本地提交 `feat: draw mermaid fences as diagrams`）。窗口/`--peek`/PDF 观感属需人工确认，已记录。
- 当前条目内的下一步动作：启动 G2 后台子 agent（先回源码核实 HTML/`<img>` 现状与差距文档说法是否一致，再按 §3-G2 验收口径动手；G2 未提交改动与后续台账编辑随 G2 提交）。
- 后台子 agent sessionID：`待填`（G2；G1 的两个子会话均已完成，不复用）
- 未提交改动清单：G1 全部改动已提交；本节及之后的台账编辑随 G2 提交。
- 已知阻塞：无
- 基线状态：2026-10-16 `cargo test --workspace --locked` 全绿（冷启动基线，master 亲跑）
- 分工约定：子 agent 拥有当前条目（§3.x 明细 + 本恢复区 + §5 日志）的写权，实时落盘；master 拥有状态总表（§2）、复核结论与 commit。一次只有一条 `进行中`，同一时刻只有一个写代码的子 agent。

## 2. 条目状态总表

状态取值：`未开始` / `进行中` / `已完成` / `无需改动` / `阻塞待决策`。
证据规则：`已完成` 必须附可复现证据（改动文件与符号、测试名、验证命令与结论），写在 §3 对应明细块，表内只放索引。

### 第一节 文件打开就显示不对的（优先级最高）

| # | 条目 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1 | Mermaid 图表渲染 —— 最大空白 | 已完成 | §3.1（提交 `feat: draw mermaid fences as diagrams`） |
| 2 | HTML 的语义处理 | 未开始 | — |
| 3 | `x^2^` / `H~2~` 上下标语法 | 未开始 | — |
| 4 | `==highlight==` 荧光标记 | 未开始 | — |

### 第二节 引擎能力已有、只是没暴露成选项

| # | 条目 | 状态 | 证据 |
| --- | --- | --- | --- |
| 5 | 主题系统 | 未开始 | — |
| 6 | 表格与行内代码的样式选项 | 未开始 | — |
| 7 | 排版预设的可编辑深度 | 未开始 | — |
| 8 | 字距滑块与窄栏左对齐策略 | 未开始 | — |
| 9 | 语法高亮的语言覆盖 | 未开始 | — |

### 第三节 阅读交互的小件（表格行）

| # | 条目 | 状态 | 证据 |
| --- | --- | --- | --- |
| 10 | 图片单击 → 大图查看器（pinch 缩放、全屏、分享、GIF 动画；⌥点击看原图） | 未开始 | — |
| 11 | 跨单元格 / 整表选择，复制保留表格形状、可直接粘进电子表格 | 未开始 | 待核实（见 §4） |
| 12 | 侧栏文件名搜索（按名字或扩展名过滤，`txt`、`notes txt` 这类查询都认） | 未开始 | — |
| 13 | 拖选到窗口上下边缘自动滚页 | 未开始 | 待核实（见 §4） |
| 14 | 空窗口起始页列最近文件、最近文件夹、收藏文件夹 | 未开始 | — |
| 15 | Space 翻页：两行重叠、按行对齐、底边不切半行、按住或减动效直达 | 未开始 | — |
| 16 | 设置里有快捷键清单页（只读，列出整个菜单栏键位） | 未开始 | — |

### 第四节 产品面（差距最实在）

| # | 条目 | 状态 | 证据 |
| --- | --- | --- | --- |
| 17 | 系统级 Quick Look 扩展（macOS 专属） | 未开始 | — |
| 18 | 分发与更新（子项 a–d 见 §3.18） | 未开始 | — |
| 19 | 平台与商业配套（子项 a–d 见 §3.19） | 未开始 | — |

#### 子项拆解

- 18a 应用内检查更新：更新前可看 release notes；Labs 里可收 release candidate
- 18b 安装器：一步装好（Telari 拖拽 DMG 的 Windows 对应物）
- 18c 包管理器收录：Homebrew cask 的 Windows 对应物（winget / scoop）
- 18d 应用内"发送反馈"菜单、隐私政策 / 许可协议菜单项
- 19a iOS / iPadOS 版单独售卖
- 19b 官网中英双语（`/zh/`），应用界面语言数
- 19c 免费层 + Pro 买断、5 台设备、14 天试用、激活码离线激活、Terms / Refund / EULA
- 19d 首页定位（macOS 13+、Free public beta、正式版转付费等对外信息）

## 3. 推进顺序与条目明细

### 推进顺序（默认遵循 TELARI-GAP.md 第六节；偏离处写明理由）

1. G1 Mermaid（第六节第 1 条）
2. G2 HTML 语义 → G3 `x^2^`/`H~2~` → G4 `==mark==`（第六节第 2 条）
3. G5 主题系统 → G6 表格/行内代码样式（第六节第 3 条）→ G7 排版预设可编辑 → G8 字距/窄栏 → G9 高亮语言覆盖
4. G12 侧栏搜索 → G10 图片预览 → G11 表格结构化复制（第六节第 4 条给出的顺序）→ G13 边缘自动滚页 → G14 收藏文件夹 → G15 Space 翻页 → G16 快捷键清单页
5. G17 Quick Look（先评估平台边界再定案）→ G18 分发与更新（第六节第 5 条）→ G19 平台与商业

调整理由：第六节未点名 G7–G9、G13–G16、G17、G19。插入位置按"同类相邻、先依赖后被依赖、先小后大"：G7/G8 与 G5/G6 同属配置暴露面，且 G5 的磁盘 TOML 设施可被 G7 复用（故 G5 先于 G7）；G9 属引擎覆盖度，紧随其后；G13–G16 为阅读交互同类相邻；G17 是分发体验的上游评估项，放在 G18 前。

### G1 明细（Mermaid 图表渲染）

- **验收口径**：` ```mermaid ` 围栏不再按普通代码块渲染，而是画成图——流程图（形状、箭头变体、子图、自环、边标签）优先，随后 sequence / state / ER / class / pie / mindmap / tree view；线条与文字跟随亮/暗主题；导出 PDF 中图的文字是可选中的真字形；画不出来（语法错误/不支持）→ 降级为代码块并给出可读原因，绝不 panic；超大图有规模上限不卡死；`--report` / `--shapes` 给出图元计数等可对比数字；新增 `fixtures/mermaid-*.md`；解析/布局纯函数单测。**架构级决策：技术方案（自绘 vs 引布局引擎）与备选必须先写进本块再动工。**
- 方案（2026-10-16 定案，含备选与理由）：
  - **决定：自绘。新增工作区成员 `crates/rubrica-mermaid`（解析 → 布局 → 场景图），零第三方依赖，另在 `rubrica-app` 的 display list 增加一个 `Op::Shape` 图元。**
  - 理由（逐条对验收口径）：① **PDF 真字形**：验收要求图的文字走与非图文本同一条字形管线、PDF 里可选中。矩阵来自布局引擎并转 SVG/DOM 的路线会先把文字变成 path 或位图，再转回字形是二次发明；自绘时图上的文字与正文同走 `FontEngine::shape_runs` → `PaintRun`（自带 face/clusters/source）→ `Op::Runs`，PDF 侧已有 `ActualText` + ToUnicode 通道（`view.rs` 的 `write_pdf`），天然满足。② **主题跟随**：现有管线里颜色是 `ColorRole` 语义角色，由 `Palette::of(dark)` 在绘制时解析（窗口 / PNG / PDF 三处共用），自绘只要把线条、文字、填充各自落到角色上，明暗自动跟随。③ **无 webview/Electron/浏览器内核**：仓库的架构前提（README 第 7 行），任何“起 headless Chromium 渲染 mermaid.js”的方案直接违背。④ **降级与上限**：解析失败要变成“普通代码块 + 可读原因”，这是 parser 的返回类型（`Result<Diagram, ParseError>`）与块上的一行错误文本，比事后从渲染引擎里回捞错误干净得多。⑤ **`--report` 可对比数字**：自绘的图元/文本计数就在场景图里，直接进 `report.rs`；引引擎时这些数只有引擎自己知道。
  - 备选 A（拒绝）：`mermaid-rs` / `mermaid-rs-renderer` 等 Rust 移植 —— 生态上没有覆盖 flowchart + sequence + state + class + ER + pie + mindmap 的成熟实现；仍是布局引擎，PDF 真字形/主题角色都要再包一层，等于引一个不可控的大依赖去换一个半成品，许可证与维护风险也不划算。
  - 备选 B（拒绝）：`layout-rs` / `graphviz-rust`（Graphviz 绑定）—— 覆盖 flowchart，但 sequence/state/class/ER/pie/mindmap 不在其中；外部 native 依赖（graphviz 运行库）破坏“单个 exe、无安装器”的分发前提（README 已知限制首条）。
  - 备选 C（拒绝）：headless Chromium/BrowserView 渲染 mermaid.js 后取位图 —— 违背“无 webview”前提，且 PDF 里图是位图，直接不满足验收。
  - 备选 D（拒绝）：把 flow 图当纯文本树（如把 `-->` 画成 `→` 的缩进列表）—— 便宜，但“形状、箭头变体、子图、自环”一条都不满足，等于不做。
  - 成本承认：自绘是四条路线里最贵的一条，但只有它在四个硬约束（无 webview、PDF 真字形、主题角色、可降级可计数）上都不打折；工作量按“流程图先做、其余图类型逐个加”分摊，每一步都保持工作树可编译、闸门可过。
- 进展：2026-10-16 读毕 `TELARI-GAP.md` §一.1/§五/§六、本台账、`README.md`、`IMPLEMENTATION.md`；核对 `rubrica-doc`（围栏 → `Block::lang`）、`rubrica-math`（`MathStore` 先例）、`rubrica-app/view.rs`（`Objects` / `layout_block` / `Op` / `write_png` / `write_pdf`）、`report.rs`、`printpdf` 的 `Op::DrawPolygon`（支持 bezier，够画圆角/圆/菱形）。方案已定案，开始写代码。
- 进展（2026-10-16 续）：子步骤 1 的 crate 骨架完成——`crates/rubrica-mermaid` 零第三方依赖，已登记为 workspace 成员与 `workspace.dependencies` 项。`parse.rs`：图头识别（`flowchart`/`graph` + TB/TD/BT/LR/RL、`sequenceDiagram`、`stateDiagram-v2`、`classDiagram`、`erDiagram`、`pie`、`mindmap`）、front matter 与 `%%` 注释、`title` 指令、形状表（矩形/圆角/胶囊/子程序/圆柱/圆/双圆/菱形/六边形/平行四边形/梯形/非对称/起止圆盘）、箭头变体（`-->`、`---`、`-.->`、`==>`、`<-->`、`o--`、`x--`，`|label|` 与 `-- label -->` 两种边标签）、子图、自环、行内 HTML 实体与 `<br>`、类/ER 成员表与关系标签、mindmap 缩进树、sequence 的 participant/message/note/fragment、pie 切片；上限 `MAX_SOURCE`/`MAX_ELEMENTS`/`MAX_EDGES`/`MAX_LABEL` 超限报错不画；所有失败路径返回 `ParseError { line, message }`。`layout.rs`：分层（rank/order barycenter/place）、垂直与水平路由及泳道避让、箭头（Arrow/Circle/Cross）与两端头、自环、面板（子图边框 + 标题）、sequence 生命线与消息/注记/块框、pie 扇区与图例、场景 `scale`/`ink_box`/`primitives`/边界 `finish`；文字只带 `TextRole` + `Ink` + 尺寸，不绑定字体与颜色（留给 app 侧按主题成形与着色）；多 figure 路径约定 `ShapePath.closed` 只闭合第一 figure（子程序内线用 `MoveTo` 开新 figure，app 侧渲染必须遵守）。
- 进展（2026-10-16 续 2）：`rubrica-doc` 围栏接入完成——`Block` 新增 `diagram: Option<rubrica_mermaid::Diagram>` 与 `diagram_error: Option<String>`；`Builder::close` 在代码块文字修剪后、对 `lang == "mermaid"` 的围栏跑一次 `rubrica_mermaid::parse`：成功挂图（块仍是 `BlockKind::Code`，绘图/选择/复制路径不动），失败保留原代码块文本并把 `ParseError::describe()`（`line N: 原因`，行号相对围栏内容）写进 `diagram_error`。`crates/rubrica-doc/Cargo.toml` 增加 `rubrica-mermaid` 依赖。测试：`diagram_tests::a_mermaid_fence_keeps_its_source_and_gets_a_diagram`、`a_mermaid_fence_that_cannot_be_read_stays_code_with_its_reason`、`a_fence_that_only_looks_like_mermaid_is_not_read_as_one`。
- 证据（2026-10-16 续 2）：`cargo test -p rubrica-doc` → 36 通过 0 失败；`cargo test --workspace --locked` 全绿（248+36+79+117+42+4+7+64+19，0 失败）；`cargo clippy --workspace --all-targets --locked -- -D warnings` 无警告。
- 进展（2026-10-16 续 3，app 侧接线定案，先落盘再写码）：读毕 `view.rs` 的全部 `Op` 触点（`op_top` 545 / `op_bottom` 568 / resize 预览 3970 / note 截图 10199 / `draw_document` 10360 / `write_png` 11639 / `translate_pdf_op` 11809 / `op_intersects_band` 11833 / `write_pdf` 12154）与 `peek.rs`、`report.rs` 的 Op 匹配点，以及 `MathStore`/`Objects`/`Ctx`/`Prepared`/`Blk` 的接线形状。定案：
  - **display list**：不新增每形状一个 op，而是**一张图一个 `Op::Diagram`**（`{ left, top, right, bottom, shapes: Vec<DiagramShape>, runs: Vec<PaintRun> }`，坐标都是设备像素）。理由：Scene 的绘制顺序是 z 序（面板→盒→边→标签），而 display list 的 band 二分要求 `op_top` 非递减；每形状一个 op 会二选一（要么乱序使整页每帧全画，要么排序破坏 z 序）。合成一个 op 后图内部保持场景顺序，图的两个端点（顶/底）让 band 搜索仍然精确；`op_top`/`op_bottom` 直接读存好的端点，O(1)。PDF 侧图跨页时整图逐页发出、由每页既有 clip 裁切（与既有 `Op::Image` 同策）。
  - **`Op::Diagram` 文字**不走 `Op::Runs`，避免污染 `--report` 的行统计/对齐统计；report 的 diagram 计数与字形 census 直接读 `DiagramStore`（`Entry` 记住 base `FaceRequest`，report 侧用同一 `role_request` 重新成形计数）。
  - **`DiagramStore` 仿 `MathStore`**：`intern(font, source, &Diagram, size, &FaceRequest) -> usize`，key =（源码文本、size 量化、FaceRequest）；Scene 按自然尺寸（点）缓存，绘制时按 `fit = min(1, column / scene.width)` 缩放，越宽只会缩不会裁。
  - **字体与墨色**：图内文字统一用文档 body 脸的 `FaceRequest` 按 `TextRole` 调权重（Node/Edge 400、Section 600、Title 700），测量与绘制走同一个 `role_request`，所以盒宽与墨宽同源；`Ink` → `ColorRole` 映射：Stroke/Arrow→Muted、Text/Fill→Text、Backdrop→OnAccent、Slice(i)→`[Accent,String,Type,Keyword,Number,Comment,Muted,Faint][i%8]`，亮暗由 `Palette::of(dark)` 在绘制时解析（窗口/PNG/PDF 三处同源）。
  - **笔画宽度**：`size * 0.10` 为基准（下限 0.6pt），由 `Stroke::{Thin,Normal,Heavy}`（×0.7/×1.0/×1.4）乘出，再乘 fit 与 k。
  - **降级**：`diagram_error` 有值时在代码块面板上方画一行 Muted 说明（`i18n` 新键 `DiagramError`，`{reason}` 占位，11 语言），随后照常走代码块路径；`diagram` 有值但 store 查不到时同样退回代码块（安全降级，不 panic）。
- 进展（2026-10-16 续 3 补充）：PDF 形状编码核实（读 printpdf 0.12.8 源码 `serialize.rs::polygon_to_stream_ops`/`line_to_stream_ops`）：闭合图形走 `Op::DrawPolygon`（多 ring 后统一补 `h`，Fill/Stroke/FillStroke 三种 mode），开放 figure 必须走 `Op::DrawLine { is_closed: false }` 逐 figure 发；cubic 一律编码为 `(handle1 bezier=true, handle2 bezier=true, end bezier=false)` 三元组。D2D 侧（windows 0.62.2）用 `ID2D1Factory::CreatePathGeometry` + `ID2D1SimplifiedGeometrySink::{BeginFigure,EndFigure}`（base 接口方法，经 `From<&ID2D1GeometrySink>` 取）+ `ID2D1GeometrySink::{AddLine,AddBezier}`；首 figure 按 fill 取 FILLED/HOLLOW，`MoveTo` 开的内线一律 HOLLOW 且 `EndFigure(OPEN)`，与 `ShapePath.closed` 约定一致。`ensure_brushes` 的 17 角色表已覆盖图用到的全部角色，窗口侧不需要按需建刷子。
- 进展（2026-10-16 续 4，app 侧接线落盘并过闸门）：`crates/rubrica-app/src/diagram.rs`（`DiagramStore`/`Entry`/`DiagramShape`/`DiagramInk`/`Row`、`stroke_base`、`body_face`、`role_request`、`measure_line`、`scene_path`、`paint`、`rows()`/`each_label()`/`dump`/`outside()`、`diagram_geometry`、`paint_diagram_shapes`）；`view.rs` 全部触点（`Op::Diagram` 臂：`op_top`/`op_bottom`/resize 预览/note 截图/`draw_document`/`write_png`/`translate_pdf_op`/`op_intersects_band`/PDF；`pdf_emit_runs` 抽出共用、`pdf_shape_ops` 闭合 `DrawPolygon`/开放 `DrawLine`、`shift_shape_path`；`Objects`/`Ctx`/`Blk`/`Prepared`/`View` 各带 `diagrams`/`diagram`；`layout_block` 图早退发 `Op::Diagram`；`layout_diagram_error` 降级原因行）；`i18n.rs` 加 `Key::DiagramError`（11 语言）；`report.rs` 图标签字形并入主 census/notdef、`diagrams` 汇总行（fences/drawn/elements/edges/shapes/labels/failed + 每图一行 + `!! not drawn: 原因`）、`outside()` 越界检查、`--shapes` 经 `DiagramStore::dump` 转储；`peek.rs` 预览画形状 + 逐 run 字形（pan 平移）。
- 证据（2026-10-16 续 4）：`cargo test --workspace --locked` 全绿（app 248、doc 36、其余 79/117/42/4/7/64/19，0 失败）；`cargo clippy --workspace --all-targets --locked -- -D warnings` 与 `cargo clippy -p rubrica-app --all-targets --features pdf --locked -- -D warnings` 均无警告；默认与 `--features pdf` 两配置 `cargo check` 干净。clippy 首轮报 `diagram.rs` 两处 `let_unit_value`（windows 0.62.2 把 D2D1 的 `FillGeometry`/`DrawGeometry` 映射为 C API 的 `void`，返回 `()`）：去掉 `let _ =` 后复跑全绿，未用 `#[allow]`。
- 进展（2026-10-16 续 5，fixtures 落盘 + 实测三处修正）：新增 `fixtures/mermaid-flow.md`（10 种形状、8 种箭头、两种边标签、子图、自环、第二条 LR 围栏）、`mermaid-sequence.md`（participant/两种消息/note/loop/alt-else）、`mermaid-state.md`（`[*]` 起止、`: label` 转移、`state "..." as w`）、`mermaid-class.md`、`mermaid-er.md`、`mermaid-pie.md`、`mermaid-mindmap.md`、`mermaid-broken.md`（未闭合 `[` 与不支持的 `gantt` 两条必须降级），`fixtures/README.md` 加命令与说明。实测发现并修掉三处：①`report.rs` 的 `code` 行不再把已画出的图围栏计为 "set plain"（只算真正按代码排的块，失败的才计）；②`parse.rs` 括号类错误行号从 `line 0` 改为语句真实行（`node_at` 给 line 0 的错误盖当前行；`open_lines` 记录 subgraph 起始行，未闭合报该行；两处单测加 `err.line == 2` 断言）；③state 图支持 `A --> B : label` 转移标签（此前报 "`...` is not a link between nodes"；`chain` 在 states 下把 `:` 后同行为边标签，`states_are_rounded_boxes...` 测试加标签断言）。
- 证据（2026-10-16 续 5）：`--report --width 900` 各 fixture 均出图且 `bounds` 全部 "every shape and label inside its scene"：flow `2 fence(s), 2 drawn, 26 element(s), 22 link(s), 73 shape(s), 29 label(s), 0 not drawn`（[0] 284.9×708.7pt 66/26，[1] 196.2×50.4pt 7/3）；sequence `1/1, 2 element(s), 6 link(s), 29 shape(s), 11 label(s)`；state `6 element(s), 6 link(s), 21 shape(s), 7 label(s)`；class `2/2/7/4`；er `2/1/5/3`；pie `4/0/8/12`；mindmap `7/6/19/7`；broken `3 fence(s), 1 drawn, 2 not drawn`，原因 `line 2: `[` is missing its `]`` 与 `line 1: `gantt` is not a diagram this reader draws (...)`，失败 fence 仍计入 `code` 行 "2 block(s) set plain: mermaid"。`cargo test --workspace --locked` 全绿（248+36+79+117+42+4+7+64+19，0 失败）；默认与 `--features pdf` 两配置 clippy 全绿。
- 进展（2026-10-16 续 6，改动前后对比 + 观感实测）：①"前"数字在 HEAD `3b1ba05` 的临时 worktree（`g1-before`，对照完已删除）用旧二进制跑同一 fixtures，"后"用当前二进制；②PNG 观感（`--export-png` 亮/暗各导出后人工查看）：flow 主链与全部形状/箭头/自环/两种边标签/子图面板均画出，pie 暗色切片与图例换色 → 主题跟随成立；③PDF 实测（`--features pdf --export-pdf`）：解压内容流得 103 条 `ActualText`（含图内标签 `Rectangle`、`Stadium`、`Circle`、`A subgraph`、`label with pipes`），图文字是可选真字形、与非图文字同管线；④`--shapes` 实测（pie）：`scene 216.6 x 302.8pt at 13.5pt 8 shape(s), 12 label(s)`，切片 `fill Slice(0..3)`、`stroke Thin(0.94)`（=13.5×0.10×0.7），每标签带 ink/坐标；⑤窗口 D2D 与 PDF 观感需人工确认；⑥已知粗糙（需人工确认）：多组件混排时两条边标签会相碰、子图面板 padding 可能压住相邻组件盒子（flow fixture 第二簇可见），建议复核时决定是否单开打磨子步骤。
- 证据（2026-10-16 续 6）数字对比（同 fixture、`--width 900`；"前"=HEAD 二进制代码块 / "后"=当前二进制图；前无 `diagrams` 行，后无对应 code 行）：
  | fixture | 前 blocks/lines/height | 前 code | 后 blocks/lines/height | 后 diagrams |
  | --- | --- | --- | --- | --- |
  | flow | 6 / 31 / 803pt；Consolas 329+Segoe 263+Georgia 10=602 glyphs | 2 plain | 6 / 6 / 1055pt；Segoe 428+Georgia 10 | 2 fence, 2 drawn, 26 elem, 22 link, 73 shape, 29 label |
  | sequence | 3 / 17 / 463pt | 1 plain | 3 / 2 / 475pt | 1/1, 2/6/29/11 |
  | state | 2 / 9 / 280pt | 1 plain | 2 / 1 / 346pt | 1/1, 6/6/21/7 |
  | class | 2 / 11 / 322pt | 1 plain | 2 / 1 / 256pt | 1/1, 2/2/7/4 |
  | er | 2 / 9 / 280pt | 1 plain | 2 / 1 / 171pt | 1/1, 2/1/5/3 |
  | pie | 2 / 6 / 216pt | 1 plain | 2 / 1 / 424pt | 1/1, 4/0/8/12 |
  | mindmap | 2 / 9 / 280pt | 1 plain | 2 / 1 / 417pt | 1/1, 7/6/19/7 |
  | broken | 7 / 13 / 435pt | 3 plain | 7 / 13 / 499pt | 3 fence, 1 drawn + 2 not drawn（原因见上） |
- 证据（2026-10-16）：`cargo test -p rubrica-mermaid` → 42 通过 0 失败（解析 27，含 `every_shape_bracket_is_read`、`arrow_heads_at_both_ends_are_recorded`、`a_diagram_past_its_ceilings_is_refused_rather_than_drawn`、`no_input_can_make_the_parser_panic`；布局 15，含 `boxes_stay_off_each_other_and_on_the_page`、`an_arrow_head_lands_on_the_edge_of_the_box_it_points_at`、`a_self_loop_leaves_its_box_and_comes_back_to_its_top`、`a_subgraph_is_a_panel_behind_its_members`、`a_sequence_puts_its_participants_over_their_lifelines`、`a_pie_is_a_slice_per_value_with_a_legend_under_it`、`scaling_the_scene_moves_its_ink_and_its_type_together`、`a_cycle_does_not_stop_the_ranking`、`no_diagram_of_the_corpus_comes_out_with_missing_arithmetic`）。`cargo test --workspace --locked` 全绿（248+33+79+117+42+4+7+64+19，0 失败）；`cargo clippy --workspace --all-targets --locked -- -D warnings` 无警告。
- 进展（2026-10-16 续 7，收尾）：`README.md`（Mermaid 段落 + crate 表加 `rubrica-mermaid` 并修正引擎 crate 描述）、`IMPLEMENTATION.md`（验收清单加 Mermaid 条目、验证节加无头验证条目、工作日志加 2026-10-16 条目、Verification boundary 加人工检查与已知打磨项）、`fixtures/README.md` 已更新；新增 `.gitattributes`（`*.rs whitespace=cr-at-eol`）——`view.rs` 在 HEAD 就是 CRLF（15243 CR），默认规则把每个新增行的 CR 当尾随空白，`git diff --check` 原本会全红；给 `*.rs` 设 cr-at-eol 后为绿且不影响真尾随空白的检查（未改任何文件行尾、未动 git config）。收尾闸门：`cargo build --release --locked` 通过；`git diff --check` 绿；`cargo test --workspace --locked` 全绿（248+36+79+117+42+4+7+64+19，0 失败）；默认与 `--features pdf` 两配置 clippy 全绿。
- 遗留（2026-10-16 master 复核后，条目退回进行中，剩余两块已交续作子 agent）：
  ① **treeView**：Telari changelog 0.6.1 原文 "Telari now draws sequence, state, ER, class and pie diagrams, mindmaps and tree views"，对应 Mermaid v11.14+ 的 **TreeView** 图类型（`treeView` / `treeView-beta` 头；缩进或盒线输入；引号标签；`/` 结尾目录加粗；`##` 描述；`:::highlight`）。前作只把 mindmap 按树布局，**不等价**，需实现；
  ② **排版粗糙（要可测不变量，不靠肉眼）**：`mermaid-flow.md` 第二簇两条边标签相碰、子图面板 padding 压相邻盒。要求：边标签（含 backdrop）两两不重叠、子图面板含全部成员且不与非成员相交，做成布局不变量测试（语料 = `fixtures/mermaid-*.md`）；
  ③ master 复核已核实：四闸门亲跑全绿、`--report` 抽查与台账数字一致（flow `2 fence(s), 2 drawn, 26 element(s), 22 link(s), 73 shape(s), 29 label(s), 0 not drawn`；broken `3 fence(s), 1 drawn ... 2 not drawn` 且原因两行正确）；
  ④ 已记录的边界（有意为之，不再扩）：`classDef`/`style`/`click` 等样式指令"接受并跳过"（见 `parse.rs`），配色走主题角色，不追 Telari 自定义样式的像素级一致；icon 注解按 Mermaid 默认 `showIcons=false` 忽略。
  ⑤ **master 第二轮复核通过（2026-10-16）**：treeView 前后数字、flow 新场景尺寸与计数、broken 拒绝文案均抽查一致；四闸门全绿；G1 判定完成，本地提交 `feat: draw mermaid fences as diagrams`。
- 进展（2026-10-16 续 8，续作子 agent：treeView）：`parse.rs` 新增 `DiagramKind::TreeView`（label `"tree view"`）与 `Diagram::Tree { items: Vec<TreeItem>, title }`；`parse_tree`/`tree_line`/`tree_item` 按行读：盒线行以最后一个分支字符（`├└┣┗`）的列位置为深度、纯缩进行以前导空格的列数为深度（tab 展开 4 空格），`open` 栈记录每层开口的列与项号，混排也认；引号名、`##` 描述（取行尾）、`:::highlight`（仅此一类生效，未知类忽略）、`icon(...)`（读后丢弃）；错误行号指向原始输入行；上限沿用 `MAX_ELEMENTS`/`MAX_LABEL`。`layout.rs` 新增 `tree_view()`：行式布局（step 1.6em），父行到子行画肘形连接线（vertical + stub），目录（`/` 结尾）用 `TextRole::Section`（weight 600）并保留 `/`，文件用 `Node`（400）；`Ink::Accent`/`Ink::Muted` 两个新墨色角色（app 侧映射 `ColorRole::Accent`/`Muted`），`Label.italic` 让描述走斜体（`place_label`/`DiagramMeasure` 加 italic 参数，测量与绘制同源，app 的 `label_request`/`intern`/`paint`/`dump`/report census 全走它）；`--report` kind 显示 `"tree view"`，`scene.elements`=行数、`scene.edges`=有父的行数；降级文案列表改为 "(it reads flowchart, sequence, state, class, ER, pie, mindmap and tree view)"。接线全绿。
- 证据（2026-10-16 续 8，treeView）：新增 `fixtures/mermaid-tree.md`（缩进含 4 层深嵌套 + 盒线重型两段围栏，含引号名、`##`、`:::highlight`、`icon`）并更新 `fixtures/README.md`；"前"（HEAD `3b1ba05` 临时 worktree 旧二进制、同 fixture 绝对路径）`blocks 6 / lines 24 / height 657pt`、`code: 2 block(s) set plain: mermaid`、无 diagrams 行；"后"（当前二进制）`blocks 6 / lines 7 / height 693pt`、`diagrams: 2 fence(s), 2 drawn, 15 element(s), 12 link(s), 24 shape(s), 18 label(s), 0 not drawn`（[0] tree view 10 elem/9 link 220.3 x 247.3pt 18/12；[1] 5 elem/3 link 161.2 x 127.2pt 6/6）、`bounds: every shape and label inside its scene`；worktree 对照后已删除。`cargo test -p rubrica-mermaid` 42 → 53（7 个新解析测试、2 个新布局测试、corpus 列表汇入 treeView）。
- 进展（2026-10-16 续 8，排版不变量）：`fixtures/mermaid-*.md` 语料不变量测试两条（`layout.rs`：`include_str!` 读 9 个 fixture、测试内抽围栏、`mermaid-broken` 的失败围栏按预期跳过）：`every_edge_label_of_the_corpus_sits_clear_of_every_other_one`（所有 backdrop 矩形两两不相交）、`every_subgraph_of_the_corpus_holds_its_members_and_stays_off_the_rest`（面板含全部成员、与非成员盒及任何非自身标签不相交）。首跑复现 flow 第一围栏两处病灶：k–l 与 s–t 两条边标签 backdrop 相碰（`[56.2,143.8]x[62.45,72.75]` vs `[68.7,207.3]`）、面板压住 t 盒（`[133,194]x[59.6,157.6]` vs `[123,147]x[78.6,102.6]`）。修复三处，全部做成布局规则而非调参：①同一 rank 边界跨 ≥2 个带标签的边时，`edge_room()` 按每名一条槽预留间隙（gap 加 `(n-1)*(名高+0.6em)`），放置期按槽位把名字码进各自高度带（单名时位置不变，既有断言 `an_edge_label_sits_on_its_line_with_a_backdrop` 仍绿）；②新增 `settle_panels()`（路由前结算）：面板 padding 压到的非成员盒沿 rank 同侧整体让位（连同更外侧的盒一起推，避免 sibling sweep 推回），边标签会压到面板时改为整组整体平移（名字必须留在自己线上），循环至多 8 轮、每轮后重跑 `separate`；③sequence 的 fragment 标签原本与它覆盖的第一条消息同一行，现在该行预留 tag 高度条带（`strip`），标签落在条带里。修复后两条不变量测试在全部 9 个 fixture（含 broken）上通过。
- 证据（2026-10-16 续 8，排版不变量）：`cargo test --workspace --locked` 全绿（248+36+79+117+**53**+4+7+64+19，0 失败）；默认与 `--features pdf` 两配置 clippy 全绿；`--report --width 900`：flow 仍 `2 fence(s), 2 drawn, 26 element(s), 22 link(s), 73 shape(s), 29 label(s), 0 not drawn`，场景 284.9 x 708.7pt → 338.6 x 732.1pt（面板右移 261.0 起点 + 两标签分层 baseline 104.5/123.4），sequence 318.8pt → 366.8pt（tag 条带），全部 fixture `bounds` 全绿。

### G2 明细（HTML 的语义处理）

- **验收口径**：`<sub>`/`<sup>` 画成真上下标；`<b>`、`<span>` 等格式标签剥掉、文字接上；`<td>`、`<br>` 分隔标签掉字不粘字（`12 34`；中英文之间不插空格）；块级 HTML（`<table>`、`<details>`）以源码样式完整显示、纯包裹标签不显示；`<img>` 可显示（含 Typora 的 `style="zoom:87%"`），有"Show HTML image tags"开关。
- 证据：—

### G3 明细（`x^2^` / `H~2~`）

- **验收口径**：Pandoc 规则渲染真上下标；字号与升降取数学字体的 MATH 常量，与脚注引注号共用一套尺寸；代码/代码块内不生效。
- 证据：—

### G4 明细（`==highlight==`）

- **验收口径**：`==text==` 渲染为荧光标记；明暗主题各有合适样式；设置里可关；代码/代码块内不生效。
- 证据：—

### G5 明细（主题系统）

- **验收口径**：用户主题文件（TOML）目录；`based_on` 继承；`[chrome]` 侧栏/标签条/分割线一起换色；坏文件只跳过坏键或整文件跳过并给原因；亮色与暗色各一个独立槽；切换只重绘不重排；"Copy as My Theme…"；存盘即生效。内置主题集可比 Telari 少但明暗两套齐备。
- 证据：—

### G6 明细（表格与行内代码样式）

- **验收口径**：三线表（booktabs）与网格两套可切，切换不动行高列宽和阅读位置；导出 PDF 跨页的每一段自带完整表框；行内代码 Capsule / 纯字体+颜色两式（默认后者）。
- 证据：—

### G7 明细（排版预设可编辑深度）

- **验收口径**：预设落盘为可编辑文件；可逐项改 `marker_inset`、`list_marker_gap`、`list_bullet_size`、`quote_bar_gap`、`code_capsule` 等布局项；可按扩展名（如 `.txt`）绑定预设；存盘即生效；窗口 / 导出 / peek 三处同步。
- 证据：—

### G8 明细（字距滑块与窄栏左对齐）

- **验收口径**：letter spacing 暴露为用户设置；正文栏窄于阈值（约 20 字符，随字号折算）时整页改左对齐；触发宽度与开关都在设置里。
- 证据：—

### G9 明细（语法高亮语言覆盖）

- **验收口径**：覆盖面达到 Telari 宣称量级（213 种语言 + 常见别名）；高亮色按明暗各一套、类间距 ≥ 7.5 L*；代码块右上角显示语言名；Tab 每 4 字符对齐。引入高亮引擎属依赖决策：先写方案与备选。
- 证据：—

### G10 明细（图片查看器）

- **验收口径**：图片单击 → 应用内查看器（缩放/平移/适应/看原图，ESC 关闭；GIF 动画尽力）；Windows 无系统 Quick Look，"分享/系统集成"部分作为平台差异标注；命中检测/缩放换算等纯函数有测试；窗口交互标"需人工确认"。
- 证据：—

### G11 明细（表格选择与结构化复制）

- **验收口径**：先核实"无 cell 级选择"是否属实（§4）；做到 cell / 跨单元格 / 整表选择，复制保留表格形状（TSV），可直接粘进电子表格；纯函数 + 复制文本 golden 测试。
- 证据：—

### G12 明细（侧栏文件名搜索）

- **验收口径**：侧栏按名字或扩展名过滤，`txt`、`notes txt` 这类查询都认；纯函数测试。
- 证据：—

### G13 明细（拖选到边缘自动滚页）

- **验收口径**：先核实是否真的没有（§4）；拖选到窗口上下边缘自动滚页，速度随越界距离变化；纯函数测试 + 动效标"需人工确认"。
- 证据：—

### G14 明细（收藏文件夹）

- **验收口径**：可收藏/取消收藏文件夹并持久化；空窗口起始页列最近文件、最近文件夹、收藏文件夹；持久化纯函数测试。
- 证据：—

### G15 明细（Space 翻页）

- **验收口径**：Space 翻页带两行重叠、按行对齐、底边不切半行；按住连续翻页或减动效直达；先核实现有垂直页栈模型（§4），目标滚动计算做成纯函数测试，动效标"需人工确认"。
- 证据：—

### G16 明细（快捷键清单页）

- **验收口径**：设置里有只读快捷键清单页；内容从键位定义源生成、不得手抄漂移；测试覆盖全部动作。
- 证据：—

### G17 明细（系统级 Quick Look）

- **验收口径**：macOS 专属，Windows 仓库内不可完成；评估 Windows 对应物（Explorer 预览扩展）是否值得做，预期 `阻塞待决策` 并写明外部条件（平台/需人拍板的架构选择）。
- 证据：—

### G18 明细（分发与更新）

- **验收口径**：a) 应用内检查更新可运行：查 release、显示 release notes、下载并验证、提示重启；RC/Labs 通道开关。b) 安装器：一步安装、可卸载、可复现产出（CI 或本地可构建脚本）。c) winget/scoop 清单文件就绪（提交属外部动作，写明）。d) 反馈菜单（GitHub issue 入口）+ 隐私政策/许可协议菜单项。不得用"只写了文档"假装完成。
- 证据：—

### G19 明细（平台与商业配套）

- **验收口径**：a–d 逐子项定案：仓库内能落地的落地；不能落地的写明外部条件（苹果开发者账号、官网、商业决策），标 `阻塞待决策`；已有能力（如 11 种界面语言）核对后标 `无需改动` 附证据。
- 证据：—

## 4. 事实核查记录（"待确认"条目的核实结论）

> 规则：动手前回源码/运行核实；若差距其实不存在，标 `无需改动` 并附证据，不造重复轮子。

- G11 表格 cell 级选择：**待核实**（差距文档注明"待确认"）。核实点：`crates/rubrica-app/src/tables.rs`、剪贴板复制路径。
- G13 拖选边缘自动滚页：**待核实**（差距文档注明"待确认"）。核实点：选择/拖拽处理路径。
- G15 Space 翻页模型差异：**待核实**（差距文档只描述"模型不同"）。核实点：Space 键现行为与页栈语义。

## 5. 事件日志（倒序，追加）

- 2026-10-16 master：G1 第二轮复核通过——四项闸门亲跑全绿（mermaid 53）；release 二进制抽查 `mermaid-tree.md`（2 drawn、kind `tree view`、bounds 全绿）、`mermaid-flow.md`（338.6 x 732.1pt、计数不变）、`mermaid-broken.md`（拒绝文案含 tree view）；G1 标 `已完成` 并本地提交 `feat: draw mermaid fences as diagrams`；恢复区切至 G2。
- 2026-10-16 续作子 agent（`ses_ebba1c625ffedQ3erfhgcjuTEq`）：G1 剩余两块完成落盘——①treeView 第九图类型（解析/布局/app 接线/降级文案/`fixtures/mermaid-tree.md`/前后 `--report` 数字）；②排版不变量：新增两条语料测试（`fixtures/mermaid-*.md` 全量，边标签 backdrop 两两不碰、subgraph 面板含成员且不碰非成员盒与标签），首跑复现 flow 病灶后修三处（同边界多标签按槽分层并加宽 rank 间隙、`settle_panels` 让盒与整组让开面板、sequence fragment 标签独立条带）；README/IMPLEMENTATION/fixtures/README 已更新；台账续 8 记录证据。下一步：收尾闸门复跑 + master 复核。
- 2026-10-16 续作子 agent（`ses_ebba1c625ffedQ3erfhgcjuTEq`）：接手 G1 剩余两块；复跑 `cargo test --workspace --locked` 基线全绿；核对 `rubrica-mermaid` 现有 8 图类型与 `--report` 接线，开始任务 A（treeView 解析/布局/接线）与任务 B（排版不变量测试 + flow 第二簇修复）。
- 2026-10-16 master 复核 G1 主体：四项闸门亲跑全绿；`--report` 抽查属实；核实 Mermaid TreeView 为独立图类型（v11.14+，官方语法）+ Telari changelog 原文 "mindmaps and tree views" → 判定 G1 退回进行中；剩余工作（treeView + 排版不变量）写入 §3-G1 遗留并启动续作子 agent。
- 2026-10-16 子 agent：G1 收尾完成——README/IMPLEMENTATION/`.gitattributes` 落盘，四道闸门全绿（test / 双配置 clippy / release build / `git diff --check`；CRLF 文件 view.rs 用 `*.rs whitespace=cr-at-eol` 处理，未改行尾）；§1 改为「等待 master 复核」，简报已出。
- 2026-10-16 子 agent：改动前后 `--report` 数字对比完成（HEAD `3b1ba05` 临时 worktree 旧二进制 vs 当前，对照表入 §3-G1 续 6）；PNG 亮/暗观感与 PDF `ActualText`（103 条，含图内标签）实测通过；已知粗糙（多组件标签相碰/面板压盒）记入遗留待人工确认；下一步 README/IMPLEMENTATION 与收尾闸门。
- 2026-10-16 子 agent：新增 8 个 `fixtures/mermaid-*.md` 并实测，发现并修三处（report `code` 行不再误计已绘围栏、未闭合括号/subgraph 行号由 `line 0` 改为真实行、state 图支持 `A --> B : label`）；全部 fixture 出图且 bounds 全在框内，工作区测试与两个配置 clippy 全绿；下一步改动前后 `--report` 数字对比与 PDF/PNG 实测。
- 2026-10-16 子 agent：G1 子步骤 2「app 接线」落盘并过闸门——`diagram.rs`（`DiagramStore`/D2D 几何/绘制）、`view.rs`（`Op::Diagram` 全触点、PDF 形状编码、图早退与降级行）、`i18n.rs`（`Key::DiagramError` 11 语言）、`report.rs`（计数/census/`--shapes`/越界检查）、`peek.rs`（预览绘制）；工作区测试全绿、默认与 `--features pdf` 两个配置 clippy 全绿（修掉 `let_unit_value` 两处，无 `#[allow]`）；下一步 fixtures 与 `--report` 数字对比、降级与 PDF 实测。
- 2026-10-16 子 agent：app 侧接线定案并落盘 §3-G1（一张图一个 `Op::Diagram`（z 序与 band 排序两全）、`DiagramStore` 仿 `MathStore`、`Ink`→`ColorRole` 与 `role_request` 权重表、笔画基准 `size*0.10`、失败行 `i18n::Key::DiagramError`、PDF 形状编码按 printpdf 源码核实）；开始写 app 代码。
- 2026-10-16 子 agent：`rubrica-doc` 围栏接入完成（`Block.diagram` / `Block.diagram_error`，失败降级为代码块 + 可读原因），36 单测全绿，工作区两道闸门全绿；下一步 `rubrica-app` 的 `Op::Shape` 与 `DiagramStore` 接线。
- 2026-10-16 子 agent：G1 子步骤 1 的解析器 + 布局器完成——`crates/rubrica-mermaid` 42 单测全绿，`cargo test --workspace --locked` 与 `cargo clippy --workspace --all-targets --locked -- -D warnings` 两道闸门全绿；下一步接 `rubrica-doc` 围栏识别与降级。
- 2026-10-16 子 agent：G1 方案定案（自绘，新建 `crates/rubrica-mermaid` + `Op::Shape` 图元；备选 A–D 与拒绝理由写入 §3-G1）；开始子步骤 1 基础设施。
- 2026-10-16 启动 G1 后台子 agent，sessionID `ses_ebc0774d8ffemi2NtyQYH7bbIs`；master 本轮结束，等完成通知再复核。
- 2026-10-16 建账：通读 `TELARI-GAP.md`；`git log`/`git status` 核对（HEAD `3b1ba05`，工作区仅 `.freebuff/`、`TELARI-GAP.md` 未跟踪）；`cargo test --workspace --locked` 基线全绿；建立本台账；启动 G1 后台子 agent。
