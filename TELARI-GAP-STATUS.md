# TELARI-GAP-STATUS.md —— Rubrica × Telari 差距补齐台账

> 权威差距依据：`TELARI-GAP.md`（2026-10-16 调研，对照 Telari 0.6.3）。第五节"已领先"不计入目标。
> 本文件是唯一进度权威：会话上下文只是缓存；恢复只看本文件。事件发生（完成子步骤、跑完闸门、换子 agent）时立刻更新，不攒批。
> 建账：2026-10-16（冷启动）。最近更新：2026-10-17 G5 复核通过并提交；当前条目切至 G6（待启动子 agent）。

## 0. 用户侧操作（照此执行）

- 暂停：等当前工具调用结束，按 Esc 中断当前轮次即可；直接关 TUI 也安全——进度在文件里，不在上下文里。
- 恢复：打开同一会话（`/sessions`）继续；上下文脏了就开新会话，把原始提示词再粘一次，master 检测到本文件会自动接上。
- 最短恢复指令：`继续 goal-driven 流程：读 TELARI-GAP-STATUS.md 恢复进度，按恢复区的"下一步动作"接着做，不要从头开始。`
- 硬约束：绝不 push；一个条目一个提交；不 amend / rebase / reset 已提交历史；不修改 `TELARI-GAP.md` 调研正文。
- **运行约束（用户 2026-10-17 强调）**：验证一律用无头模式（`--report`、`--report --shapes`、带导出参数的 CLI），**绝不裸启动 GUI 进程**——连 `--version` 这类未识别参数都会把窗口/托盘拉起来；用户可能正在用 Rubrica，托盘单实例会被顶掉。
- 用户未提交的无关改动：`.freebuff/`（未跟踪目录）、`TELARI-GAP.md`（未跟踪，证据基线）——保留不动，已记账。

## 1. 恢复区（子 agent 运行时由子 agent 实时维护；master 复核后刷新）

- 当前条目：**G6 表格与行内代码的样式选项**（进行中，待启动子 agent）
- 上一条目结论：**G5 主题系统 已完成**（2026-10-17 master 复核：四闸门亲跑全绿——test 277+36+116+117+53+4+7+64+19、clippy 默认+`--features pdf`、release、`git diff --check`；抽查：默认态 21 文件 × `--report`/`--shapes`/`--source` 全 0 差异、无 `theme` 行；`--list-themes` 6 内置 + 槽默认；`--theme` 名字/跨外观/缺失主题的报错文案与退出码逐条正确；坏文件电池（APPDATA 重定向）6 built-in / 11 read / 11 not read、原因逐条正确（坏 UTF-8、非法 appearance、>64KiB、无 name、撞内置名、重名、跨外观、环、链>8、缺 base、坏键诊断）；默认 PNG md5 `9631ca9b…` 与 before 逐字节相同、`Mono Dark` PNG `35fc496a…`；像素清点 Mono Dark (38,38,38)=463600、Solarized Light (253,246,227)=463598、默认 0；`--theme` 下 notes.md 几何与默认逐字节相同（只重绘不重排）；PDF 冒烟：两主题文本层逐字节相同、内容流取色 = 两套 palette → 判定达成，本地提交 `feat: colour the reader from theme files`）。需人工确认项（菜单/对话框/`[chrome]` 观感、存盘即生效窗口实测、Telari 逐字键名）已记录。
- 更早条目结论：**G4 `==highlight==` 荧光标记 已完成**（2026-10-17 master 复核：四闸门亲跑全绿——test 256+36+116+117+53+4+7+64+19、clippy 默认+`--features pdf`、release build、`git diff --check`；抽查：after 二进制与 `target/release` md5 同为 `44259aed…`；`highlight.md` 前后数字逐条吻合（后 15/35/1128pt、`highlight: 22 span(s) in 6 block(s) -> 23 band(s) drawn`、复制 1961/29 breaks/2 tabs、RTL 4 chars；前 15/38/1200pt、复制 2049）、`--no-highlights` 输出与 before 逐字节相同；`--shapes` 探针逐位一致（band left=143.61 top=52.45 width=160.38 height=22.50；cell 2 带 left=197.05 width=92.33/83.70 top=54.52/100.42）；全量 21 文件对比仅新 fixture `highlight.md` 自身有差（report/shapes 各 1）、其余 20 个前后逐字节相同、`--source` 21 文件 0 差异；PNG 像素实测亮带 (250,230,107)=15169、暗带 (82,71,23)=15169、`--no-highlights` 双色均 0 → 判定达成，本地提交 `feat: draw ==marks== as a highlight`）。需人工确认项（长 run 字面偏离、链接/上标内 `==` 偏离、明暗观感、cell 行级缓冲、`==x^2^==` 带断两段）已记录。
- 当前条目内的下一步动作：启动 G6 后台子 agent（先回源码核实现状、方案写进 §3-G6 再动手；验收：三线表（booktabs）与网格两套可切、切换不动行高列宽与阅读位置；导出 PDF 跨页的每一段自带完整表框；行内代码 Capsule / 纯字体+颜色两式（默认后者）——注：G5 已落下 `code_bg` 等主题色，行内代码样式与其衔接口径由子 agent 核实后定案）。
- 后台子 agent sessionID：`待填`（G6）
- 未提交改动清单：G5 全部改动已提交；此后台账编辑随 G6 提交；未跟踪项仍只有 `.freebuff/` 与 `TELARI-GAP.md`（保留不动）。
- 已知阻塞：无
- 基线状态：2026-10-16 `cargo test --workspace --locked` 全绿（冷启动基线，master 亲跑）；G2 收尾复跑 250+36+86+117+53+4+7+64+19、0 失败
- 分工约定：子 agent 拥有当前条目（§3.x 明细 + 本恢复区 + §5 日志）的写权，实时落盘；master 拥有状态总表（§2）、复核结论与 commit。一次只有一条 `进行中`，同一时刻只有一个写代码的子 agent。

## 2. 条目状态总表

状态取值：`未开始` / `进行中` / `已完成` / `无需改动` / `阻塞待决策`。
证据规则：`已完成` 必须附可复现证据（改动文件与符号、测试名、验证命令与结论），写在 §3 对应明细块，表内只放索引。

### 第一节 文件打开就显示不对的（优先级最高）

| # | 条目 | 状态 | 证据 |
| --- | --- | --- | --- |
| 1 | Mermaid 图表渲染 —— 最大空白 | 已完成 | §3.1（提交 `feat: draw mermaid fences as diagrams`） |
| 2 | HTML 的语义处理 | 已完成 | §3.2（提交 `feat: read html by what its tags mean`） |
| 3 | `x^2^` / `H~2~` 上下标语法 | 已完成 | §3.3（提交 `feat: read ^carets^ and ~tildes~ as scripts`） |
| 4 | `==highlight==` 荧光标记 | 已完成 | §3.4（提交 `feat: draw ==marks== as a highlight`） |

### 第二节 引擎能力已有、只是没暴露成选项

| # | 条目 | 状态 | 证据 |
| --- | --- | --- | --- |
| 5 | 主题系统 | 已完成 | §3.5（提交 `feat: colour the reader from theme files`） |
| 6 | 表格与行内代码的样式选项 | 进行中 | §3.6（子 agent 待启动） |
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
- 现状核实（2026-10-16，源码逐条核对 `crates/rubrica-doc/src/lib.rs` + 既有测试，差距文档 §一.2 的说法需要修正）：
  - 差距文档称 "`Event::Html | Event::InlineHtml` 一律按 `InlineStyle::DELIMITER` 当文本画（`lib.rs:366`）——整块 HTML 会铺满版面"：**不成立**。该行（现 `lib.rs:379`）在 `Document::source` 里，是**源码视图**路径，本就应当逐字显示，不动。阅读视图（`parse_with`）对 `Event::Html` 走 `set_html`/`html_pieces`：块内文字保留、标记剥掉、`<img>`→图元、`<h1-6>`→标题、`<hr>`→分隔线、`<script>` 等非散文元素整段隐藏（测试 `a_raw_html_block_keeps_its_words_and_loses_its_markup`、`htmls_block_level_tags_end_the_line_they_arrive_on`、`an_img_tag_becomes_the_figure_it_names`、`a_scripts_words_are_not_prose`）。
  - "HTML 图片不显示"：**部分不成立**。整块 HTML 里的 `<img>` 已有图片对象（上列测试）；缺的是 ① 行内 `<img>`（句子中间的 `<img …>`，`Event::InlineHtml` 只认 `<br>`，其余直接丢）② Typora 的 `style="zoom:87%"` 完全不读（`attr` 只取 src/alt）。
  - "`<sub>/<sup>` 不成上下标"：**成立**（未知行内标签一律丢，`InlineStyle` 只有 `SUPERSCRIPT`，位 0..7 已用满，加 `SUBSCRIPT` 需把 `u8` 放宽到 `u16`）。
  - `<b>/<i>/<em>/<strong>/<span>`：标签已剥、文字已接（`a_br_tag_breaks_the_line_like_a_backslash` 第 3 段），但**不带语义**：`<b>` 不加粗、`<i>` 不斜体、`<code>` 不是行内代码，块内 HTML 路径同样全平。
  - `<td>12</td><td>34</td>`：行内路径直接粘成 `1234`（丢标签不补边界）；块路径把 `td/th/tr` 当块级结束符 → `12`、`34` 两个块，与验收"读作 12 34、CJK 之间不插空格"都不符。`<br>` 已是强制换行（保持）。
  - 块级 HTML：现在一律剥标签取字（`<table>` 会变成一串 `12 34` 文字），无"完整构造成源码样式显示"的概念；`<div>` 包裹已不显示其标签（保持，验收要求的"纯包裹标签不显示"成立）。
  - 开关：不存在；设置（菜单）里也没有入口。
- 方案（2026-10-16 定案，含取舍）：
  - **行内语义**：`InlineStyle` 放宽到 `u16` 并新增 `SUBSCRIPT`；`tag_name` 的映射表：`b/strong`→`STRONG`、`i/em`→`EMPHASIS`、`code`→`CODE`、`del/s/strike`→`STRIKETHROUGH`、`sup`→`SUPERSCRIPT`、`sub`→`SUBSCRIPT`；`span` 与未知标签只丢标记。开/闭标签用**计数器**（`[u8; 16]`，按位计）而不是栈：作者写错嵌套或漏闭合时最坏是样式少一层，不会粘住后半篇。行内事件与块内 `Fragments` 两条路径共用同一张映射表。
  - **上下标度量**：复用既有 `SUPERSCRIPT`（0.7em、抬升 0.62×block），`SUBSCRIPT` 同尺寸 0.7em、下沉 0.2×block（沿用 `ResolvedStyle.raise` 的正负号：负值下滑，绘制端 `dy = -raise` 已同时管 ascent 与 descent，表格单元路径同样按 `ascent.max(r.ascent + raise)` 计算）。与脚注引注号共用一套尺寸（同一 0.7em）。
  - **分隔标签**：`td/th/tr` 为"分隔标签"（`<br>` 保持换行）：`Builder` 攒一个待补边界，在下一段文字落笔时决定——两侧都是 CJK（`rubrica_type::classify::Role::of`，与排版引擎同一分类器）则不插空格，否则补一个空格；已有空白或行首行尾不补。块路径同步：`td/th/tr` 从 `BLOCK_LEVEL` 移到分隔集合，不再切成两个块。
  - **块级源码显示**：块内出现**完整的** `<table>…</table>` 或 `<details>…</details>`（同块内开闭配对）→ 整块原文以 `BlockKind::Code` + `lang=Some("html")` 呈现（源码样式、可高亮、可复制）；纯包裹标签（孤立的 `<div>`、`<div>…Markdown…</div>` 这种只有行内内容的包裹）仍只留文字。开闭配对判定在 `Fragments` 扫标签时顺带记录（不解析、不重排 HTML）。
  - **`<img>`**：`ObjectKind::Image` 增 `html: bool` 与 `zoom: Option<u16>`（百分比，整数，Eq 友好）；`style` 里的 `zoom:NN%` 解析进 `zoom`；行内 `<img>` 与块内 `<img>` 都出图元；app 侧 `ImageStore::fit_zoomed`：自然尺寸 × zoom% 后再按栏宽收缩（沿用既有相对路径/alt/尺寸上限设施）。
  - **开关**：菜单「文本阅读」子菜单加 `Show HTML image tags` 复选项（复用 `MenuTextReading` 的位置），命令 `Command::HtmlImages(bool)`，持久化到 `HKCU\Software\Rubrica\HtmlImages`（缺省=开，`DefaultHtmlImages` 语义），11 种界面语言加 `Key::MenuHtmlImages`。**关掉时** `<img>` 按**无标签的替代文字**呈现（alt 原文，空 alt 什么都不留）——这是取舍点，记为**需人工确认**（另一候选：整段按纯文本原样显示标签）。
  - **报告**：`--report` 增加 `images` 汇总行（图元数、其中来自 `<img>` 的数、带 zoom 的数、逐图放置尺寸）与 `--no-html-images` CLI，前后对照有可复现数字。范围外（有意）：`<a>` 不变成链接、行内完整 `<table>` 不转源码、`width/height` 属性不读、`class/style` 其余样式不读。
- 证据（2026-10-16，实现与全部验收 1–7；复核入口）：
  - **实现落点**（"为什么"注释随代码）：
    - `rubrica-doc/src/lib.rs`：`InlineStyle` 由 `u8` 放宽到 `u16` + `SUBSCRIPT`（bit 8）；`OpenTags`（`[u8;16]` 按位计的开标签计数器，`open` 返回"是否首个开启"、`close` 返回"是否已全关"）；`Builder.html_style`/`html_tags`/`separate` 与 `style()`（作者标记与读者标记 OR）；`Event::InlineHtml → inline_html()`（`<br>`、`<img>`、分隔标签、样式映射、未知标签丢弃）；`set_html` 处理 `Piece::{Text{text,style}, Image{src,alt,zoom}, Source}`，`push_source` 把完整构造发成 `BlockKind::Code`+`lang=Some("html")`；`put`（消费 `separate`，旧体改名 `write`）；`Fragments` 重写（`words`/`tag`/`flush`/`paragraph_end`/`shows_a_whole_element`）；`WHOLE = [table, details]`；`SEPARATORS = [td, th, tr]` 移出 `BLOCK_LEVEL`（27→24 项）；`inline_style()` 映射 `b/strong→STRONG`、`i/em→EMPHASIS`、`code→CODE`、`del/s/strike→STRIKETHROUGH`、`sup→SUPERSCRIPT`、`sub→SUBSCRIPT`；`zoom_percent()` 读 `style="zoom:87%"`（1..=1000）；`boundary_wants_space()` 用 `rubrica_type::classify::Role::of`（与断行同一分类器）。
    - `rubrica-app`：`view.rs` 设置字段 `show_html_images` + 菜单「文本阅读」里 `check(Command::HtmlImages(!s.show_html_images), Key::MenuHtmlImages, s.show_html_images)`、`Command::HtmlImages(bool)`（记录+重排）、parse 两处与导出两处接 `html_images`、`Objects::intern` 走 `ImageStore::fit_zoomed`；`images.rs` `fit`→`fit_zoomed(path, column, zoom)`（缩放后再按栏宽收缩，只缩不放）；`theme.rs` `resolve_at` 加 `SUBSCRIPT` 分支（0.7em、`raise = -0.2×block`，与 `SUPERSCRIPT` 的 0.7em 同尺寸；两 bit 同开按上标处理=一次错嵌套而非双重变换）；`settings.rs` `show_html_images()`/`html_images_of()`/`record_show_html_images()`（`HKCU\Software\Rubrica\HtmlImages`，缺省开、非 0 都算开）；`i18n.rs` `Key::MenuHtmlImages` 11 语言；`main.rs` `--no-html-images` + `--report` 的 `html_images`；`report.rs` `Options.html_images` + `images` 汇总行 + 脚注 census 修正；`peek.rs` 预览 parse 接开关。
  - **单测**：`rubrica-doc/tests/model.rs` 新增 7 个（`a_sub_or_sup_tag_is_the_position_it_names`、`the_formatting_tags_keep_their_words_and_lose_their_markup`、`a_separator_tag_parts_two_fields_with_one_space`、`a_whole_construction_is_read_as_the_source_it_was_written_as`、`an_img_tag_keeps_its_name_and_the_size_its_author_gave_it`、`turning_html_images_off_leaves_their_names_and_not_their_boxes`、`a_mis_nested_tag_costs_one_word_and_not_the_passage_after_it`；helpers `run_of`/`style_of`），model 79→86；`rubrica-app/src/theme.rs` `a_subscript_is_small_sunk_and_the_same_size_as_its_raised_twin`；`settings.rs` `the_html_images_switch_is_on_until_it_is_turned_off`（缺省开/0=关/垃圾值=开 + 注册表往返）。
  - **验收①（真上下标、与脚注引注号同尺寸）**：`--report --shapes --width 900 fixtures/html.md` 的 glyph run——`H<sub>2</sub>O` 的 "2"：`left=680.45 width=6.79 baseline=135.03`（段基线 131.43 之下 3.6dip = 2.7pt = 0.2×13.5pt）；`mc<sup>2</sup>` 的 "2"：`left=149.82 width=6.79 baseline=154.66`（段基线 165.82 之上 11.16dip = 8.37pt = 0.62×13.5pt）；同段引注号 "1"：`left=366.12 width=6.79 baseline=154.66`——与上标 run 位移、宽度逐位相同（同尺寸证据）。
  - **验收②（格式标签映射、未知标签纯文字、标签不显示）**：`the_formatting_tags_keep_their_words_and_lose_their_markup` 一条覆盖 `b`/`strong`/`i`/`em`/`code`/`del` 的映射与 `span`/`u`/`mark` 的"留字丢形"（`InlineStyle::EMPTY`），并断言文字里不出现 `<`/`>`；`a_raw_html_block_keeps_its_words_and_loses_its_markup` 断言块路径同为 `STRONG` 段；fixture 另含 `<a>`（只留字）与 `<mark>` 实例，散文段落里无标签字形（标签只在 ④ 的源码块与 ⑦ 的代码内原样出现）。
  - **验收③（分隔标签）**：单测 `a_separator_tag_parts_two_fields_with_one_space`（块路径 `<tr><td>12</td><td>34</td></tr>` → `12 34`、`中`/`文` → `中文`、`12`/`字` → `12 字`、句中央 `<td>` 同样补空格；行内路径 `cells <td>12</td><td>34</td> end` → `cells 12 34 end`）；fixture 报告行 10 片段 `numbers, while 数据字…` + 行 11 `keeps the line bre…`（`<br>` 换行、CJK 相邻无空格）；`<br>` 的行内路径（`a_br_tag_breaks_the_line_like_a_backslash`）与表格单元路径（`a_cells_own_text_holds_its_link_its_citation_and_its_break`）两条既有测试保持全绿。
  - **验收④（完整构造成源码、纯包裹标不显示）**：完整 `<table>`/`<details>` → `BlockKind::Code` + `lang=html`；fixture 报告 `code: 3 block(s), 3 named a language, 3 of those a language this build reads`，行 17–24 全部 Consolas（`<table>`…`</details>` 原文）；孤立 `<div>` 只有文字（报告行 12–14）；before 二进制同一 fixture 把表拆成 `a`/`b`/`c`/`d` 四个单字母块并给 `<summary>` 与正文粘成 `Kept wholeA disclo…`——正是本条目要修的两类错误。
  - **验收⑤（`<img>` 显示、zoom 先缩放后适配）**：fixture 报告 `images: 4 figure(s), 3 from an <img> tag, 1 with a zoom`——`html-fig.png`（240×120px=180×90pt）两条（行内 `<img>` 与 Markdown `![]()` 并排只留字差异）；`html-wide.png`（720×300px=540×225pt）无 zoom：`486.0 x 202.5pt`（自然宽 540pt>栏宽 486pt，被栏宽收缩）；同文件 `zoom 87%`：`469.8 x 195.8pt`（=540×0.87，先按作者尺寸缩放后已放进栏宽、不再放大也不再缩）——把 "先缩放再适配" 的两种分支都钉在数字上；相对路径经 `ImageStore::resolve`（既有设施，含 `%XX`），alt 经 `ObjectKind::Image.alt`（开关关闭时的替身，见⑥）。
  - **验收⑥（开关）**：菜单「文本阅读」子菜单 `Show HTML image tags`（默认勾选）+ `HKCU\Software\Rubrica\HtmlImages`（缺省=开）+ `Key::MenuHtmlImages` 11 语言（声明 + 11 个 match 臂，`Key` 是穷尽 match，缺语言不编译）；`--report --no-html-images fixtures/html.md`：`images: 1 figure(s), 0 from an <img> tag, 0 with a zoom`、行 28 变成 `a wide figure`（alt 文字上纸）、复制文本 1902 chars（+0 html 图元）。**关掉时按无标签 alt 文字处理记"需人工确认"**（另一候选为原样显示标签）。
  - **验收⑦（代码内原样、`Document::source` 逐字、不回退）**：fixture 的 ```html 围栏与行内 `` `<sub>` `` 在报告里仍是 Consolas 字样；`--report --source fixtures/html.md` 前后二进制**逐字节相同**（61 行、1975pt，`H<sub>2</sub>O`、`<strong>`、`<img src=…` 全部按字画出）；16 个既有 fixture（chemistry/defs/delims/math_ledger/notes/rtl/reading/mermaid-×9）`--report --width 900` 前后二进制输出**逐一逐字节相同**（含 notes.md），mermaid 与全部既有能力零回退。
  - **前后数字（`fixtures/html.md`，`--report --width 900`；"前"=`%TEMP%/opencode/rubrica-before-g2.exe`，"后"=当前 release）**：前 `blocks 21 / lines 33 / height 1637pt`、`code: 1 block(s)`、无 images 行、复制 1807 chars / 3 对象；后 `blocks 18 / lines 36 / height 1719pt`、`code: 3 block(s)`、`images: 4 figure(s), 3 from an <img> tag, 1 with a zoom`、复制 1955 chars / 4 对象；`--no-html-images` 后 `blocks 18 / lines 38 / height 1290pt`、`images: 1 figure(s), 0 from an <img> tag`、复制 1902 chars / 1 对象。`fixtures/README.md` 已加命令与说明；`README.md`（"What it reads" 的 raw HTML 句 + 菜单句）、`IMPLEMENTATION.md`（验收清单 G2 条目 + 工作日志）同步。
  - **收尾闸门（2026-10-16，全绿）**：`cargo test --workspace --locked`（250+36+86+117+53+4+7+64+19，0 失败）；`cargo clippy --workspace --all-targets --locked -- -D warnings`；`cargo clippy -p rubrica-app --all-targets --features pdf --locked -- -D warnings`；`cargo build --release --locked`；`git diff --check`。无新增 `#[allow]`。
  - **需人工确认**：①开关关闭时按"无标签 alt 文字"呈现（另一候选：原样显示标签），空 alt 什么都不留——取舍已按阻力最小路径落盘；②窗口/`--peek` 观感（`<sub>` 下沉 0.2em、行内 `<img>` 与文字基线的衔接、菜单勾选与持久化的现场手感）属桌面人工检查，注册表读写已有单测；③范围外（有意，不扩）：`<a>` 不变成链接、行内完整 `<table>` 不转源码、`width/height` 与其余 `style` 属性不读、`<td>` 分隔只补一个空格。
- **master 复核通过（2026-10-17）**：四闸门亲跑全绿；`fixtures/html.md` 前后数字（后 18/1719、`images 4/3/1`；`--no-html-images` 18/1290、`1/0/0`；前 21/1637、无 images 行）、`--source` 前后逐字节相同、18 个既有 fixture 报告 0 差异、三条 `--shapes` glyph run（sub 680.45/135.03、sup 149.82/154.66、引注 366.12/154.66）全部抽查一致 → G2 判定完成，本地提交 `feat: read html by what its tags mean`。

### G3 明细（`x^2^` / `H~2~`）

- **验收口径**：Pandoc 规则渲染真上下标；字号与升降取数学字体的 MATH 常量，与脚注引注号共用一套尺寸；代码/代码块内不生效。
- 现状核实（2026-10-17，源码 + 探针 crate `%TEMP%/opencode/g3probe` + pandoc 3.10 对照，约 60 例）：
  - 本仓库解析选项为 `STRIKETHROUGH|TASKLISTS|TABLES|MATH|FOOTNOTES`（`lib.rs:462`）。此配置下：`^` 从不参与解析，`x^2^`、`H~2~`、`中文~下~标` 都以纯文本到达 `Builder`（`~`/`^` 的 run 不能 intraword 开合）；`~~del~~` 由 pulldown 自己解析成 Strikethrough（保留）。
  - **单 `~` 被偷**：`a ~b~ c`、`~a~b~`、`~~s~~ and ~x~` 里的单词边界单 `~` 对会被 pulldown 当删除线（`Start/End(Strikethrough)`，`event_range` 是**整个 span** 而非分隔符 run：`a ~b~ c` 起止都是 `2..5`；run 长度需从 range 两端向 `input` 计数）。`ENABLE_SUBSCRIPT`/`ENABLE_SUPERSCRIPT` 已存在但仅 intraword 开合、且吞掉 `~~`，与 Pandoc 不符，不用。
  - Pandoc 3.10 实测（`-t native`）：`x~2~y`/`H~2~O`/`e~x~tra` 均 intraword 成立（Pandoc 无 flanking 限制，`a ^b^`、`^a^`、`~lead~ word` 都成立）；`x^ a ^`、`x^a b^`、`~ a~`、`~a ~` 因未转义空白不成立；`x^a\ b^`→`Sup["a NBSP b"]`（`\ `＝转义空格→U+00A0）；`\^`/`\~` 不参与配对（`a\~b\~c`→字面，`\~~b~~`→`~`+Sub[b]+`~`）；`x^2\^3^`→`Sup["2^3"]`（转义字符留字面在内容里）；`x~a^b~c^`→`Sub[a^b]`+`c^`（跨字符的**交叉**配对被拒、被吞的“另一字符”开启点作废）；`a^b c^d^`→`a^b`+`c`+`Sup(d)`（先试闭、闭不成再当开启点）；`x^`c`^`→`Sup[Code]`（可跨代码 span），但 `x^`a b`^` 因空白被拒；`x^2^^`、`x^2`、`a^` 未配对保持字面；`~~a~b~~`、`x~~~~y`、`a~~~b~~~c`、`x^^2^^` 在 Pandoc 里是退化读法（空上下标/吃掉多余 `~`/`^`）——**不模仿**（见偏离）。
  - Cambria Math（upem 2048）：`ScriptPercentScaleDown=73`（→0.73em）、`SuperscriptShiftUp=750`（→0.3662em）、`SubscriptShiftDown=418`（→0.2041em）。现 `theme.rs` 硬编码 0.7/0.62/0.2，将改为读表取值（缺表/非正值回退）。`SuperscriptShiftUpCramped=615` 存在但选非 cramped 值（普通文本基线，与 `<sup>`/引注号同一套，需人工确认）。
- 方案（2026-10-17 定案，含回退与偏离）：
  - **落点**：新增 `crates/rubrica-doc/src/scripts.rs`（后缀式扫描，仿 `emphasis.rs`；复用其 `Text`/`style_range`/`merge`/`pieces`/`join`/`shift_range`，改为 `pub(crate)`）。在 `Block::close` 的 `split_definitions` 循环里 `relax_emphasis` 之后逐个 piece 运行，表格 cell 同样跑（与 emphasis 同点）。
  - **`Builder` 单 tilde 解偷**：`Tag::Strikethrough` 起止分隔符 run（从 `event_range.start` 向右、`event_range.end` 向左在 `input` 计数）都恰为 1 个 `~` 时，不压 `STRIKETHROUGH`（`strike_literal: Vec<bool>` 栈配对起止），起止各把 `~` 作字面文字写回；`~~`（run≥2）与其余 Strikethrough 行为不变。安全前提：pulldown 只有在起止 run 均未转义时才产 Strikethrough（`\~a\~`、`~a\~` 探针均无事件），故写回的两个 `~` 都该参与配对；起止 `~` 的 `sources` 由 `sources()` 的 `find` 得到（闭括号会映射到开括号源字节，两处都是未转义 `~`，奇偶判定结论相同）。
  - **scripts pass 规则**（最终）：
    1. 只认**长度恰为 1** 的 `^`/`~` run；跳过含 `InlineStyle::CODE` 的 span 内、含 U+FFFC 的 OBJECT 内、源码中反斜杠奇数为转义的 run（用 `source_at` + 反斜杠奇偶，与 emphasis 同法）。`BlockKind::Code` 整块跳过（同 emphasis）。
    2. 左到右贪心配对（每字符一个栈）：先试“闭最近的同字符开启点”，成功条件=内容（open.end..close.start）满足：(a) 无未转义空白——空白仅当“前一个字节是文本反斜杠且该反斜杠在源码中奇偶为偶（即它转义了这个空格）”时放行；(b) 不含 U+FFFC；(c) 不含任何**未转义的长度≥2** 的 `^`/`~` run；(d) 不含已被其他配对消费掉的分隔符字节（防交叉）。闭不成就把该 run 当开启点压栈。
    3. 配对成功→ `style_range(内容, SUPERSCRIPT|SUBSCRIPT)`；把该区间内**另一字符**栈上的开启点作废（Pandoc 同）；内容中的 `\ `（转义空格）切除反斜杠并替换为 U+00A0（三条 1 字节 `SourceSpan` 都指向空格源字节，保持 `source_at` 有界）。
    4. 未配对 run、run≥2、被拒配对一律字面保留（不删除）。
  - **偏离（有意，需人工确认）**：`~~a~b~~`/`a ~~ b ~~ c`/`x~~~~y`/`a~~~b~~~c`——Pandoc 会吃到空上下标或多吞 `~`；本实现 run≥2 一律字面（`~~a~b~~` 维持 pulldown 删除线读法）。`x^^2^^`/`x^a^^b^` 同理保持字面（Pandoc 产 `Superscript []`）。`x^$a$^`（对象在内容里）保持字面（Pandoc 为 Sup[Math]；本实现对象 span 只有 OBJECT 位，抬升无着落，宁字面不乱抬）。`x^`a^b`^` 跨含 `^` 的代码 span 会读成 Sup[Code]（Pandoc 在此裂成 `Sup["`a"]`+`` b`^ ``，本实现更整）。
  - **app 侧度量**：`theme.rs` 加 `ScriptMetrics { scale, raise, sink }`（默认 0.7/0.62/0.2）与 `Theme::learn_scripts(&FontEngine)`：按 `fonts.math[0]` 再 `[1]` 取脸 → `math.rs` 新 `pub(crate) fn script_constants(font, family) -> Option<(f32,f32,f32)>`（`ScriptPercentScaleDown/100`、`SuperscriptShiftUp/upem`、`SubscriptShiftDown/upem`，非正→None，值钳在 0.05..=1.0 / 0..=2.0em）；`resolve_at` 的 SUP/SUB 分支改用 `self.scripts`（引注号、`<sup>/<sub>`、`^`/`~` 三者同源）。
  - **调用点**：`view.rs::new_view`、`report.rs`、`peek.rs`（字体创建后）、`export_png`/`export_pdf`（建 theme + 字体后）、以及 6 处 `profiles::load(...).apply(&mut self.theme)` 之后（profile 可改 `fonts.math`）。
  - **报告**：`--report` 在文档含 SUPERSCRIPT/SUBSCRIPT span 时新增 `scripts` 行（raised/lowered run 数 + MATH 来源与数值），其余 fixture 报告不受影响；`--shapes` 的 glyph run 数字用于钉尺寸位移。
  - **fixtures**：新增 `fixtures/subsuper.md`（`H~2~O`、`x^2^`、intraword、CJK、`\ ` 转义、`\^`/`\~`、未配对、`~~del~~`、行内代码/围栏内不生效、与脚注同段），更新 `fixtures/README.md`；`README.md`/`IMPLEMENTATION.md` 同步。
- 证据（2026-10-17，全部无头；改动未提交，等 master 复核）：
  - **实现落点（`rubrica-doc`）**：新 `crates/rubrica-doc/src/scripts.rs`（后缀扫描 + 每字符一栈的贪心配对；复用 `emphasis.rs` 的 `Text`/`style_range`/`merge`/`pieces`/`join`/`shift_range`——改 `pub(crate)`；`apply` 按 `range.start` 排序后 `merge`，spans/sources/objects/actions 同步搬家）。`lib.rs`：`Scripts{raised,lowered}` 与 `Block.scripts`（表格 cell 计入所属 block）；`Block::close` 的 `split_definitions` 循环中 `relax_emphasis` 之后对正文与每个 cell 各跑 `relax_scripts`；`Builder` 的 `strike_literal` 解偷——`Tag::Strikethrough` 起止分隔符 run 皆恰为 1 个未转义 `~` 时不压 `STRIKETHROUGH`、起止各写回字面 `~`（`~~` 维持删除线）。
  - **app 落点**：`math.rs::script_constants`（读 `SCRIPT_PERCENT_SCALE_DOWN/100`、`SuperscriptShiftUp`、`SubscriptShiftDown`；非正→None；scale 钳 0.05..=1.0、raise/sink 钳 0..=2.0）；`theme.rs::ScriptMetrics`（默认 0.7/0.62/0.2）+ `Theme.scripts`/`scripts_from` + `learn_scripts(&FontEngine)`（按 `fonts.math` 依序取脸，找不到则整体回退默认并清空来源；与公式布局同脸）+ `resolve_at` 的 SUP/SUB 分支改用 `self.scripts`——引注号、`<sup>/<sub>`、`^`/`~` 三者同源；调用点：`view.rs` 9 处（`new_view`、6 处 `profiles::load(...).apply` 之后、`export_png`、`export_pdf`）、`report.rs`、`peek.rs` 各 1 处。
  - **报告口径**：`report.rs` 的 `scripts` 行只在 `Block.scripts` 计数>0 时打印——不是"文档含 SUP/SUB span"（否则含引注/`<sup>` 的 `html.md`、`notes.md` 也会多一行）；行内为 raised/lowered 计数、三个数值、来源脸名（无表时打印 "the design's own numbers"）。19 个 fixture（glob 含 `README.md`）+ `reading.txt`，前后各 20 份 `--report` 与 `--report --shapes`：仅 `html.md`（1 行 extent 630.4→630.7pt）、`notes.md`（位移全部 ≤0.3pt；`--width 900` 下 8 行、默认栏宽 18 行——计数随栏宽变，性质一致）、新 `subsuper.md` 有差异，且差异全部是引注/`<sup>/<sub>` 改用 MATH 值（验收要求本身）；`--source` 视图三个文件前后逐字节相同。
  - **前后二进制**：前 = HEAD `8bc9167` 临时 worktree 构建 `%TEMP%/opencode/rubrica-before-g3.exe`（2 312 192B，先以 `notes.md` 与 G2 记录核对一致）；后 = 当前源码 `rubrica-after-g3.exe`（2 346 496B；与 `target/release/rubrica-app.exe` md5 同为 `474e3105e576e66a9d35c0d6e5388121`）。
  - **fixture 数字**（`fixtures/subsuper.md`，13 blocks / 25 lines / 911pt 前后相同，故证据靠下列行）：
    - 前：无 `scripts` 行；Segoe UI census 947 字形、copy 1448 字符、strike 2 span（thinnest 0.67pt）。
    - 后：`scripts: 5 raised, 5 lowered run(s); 0.730em scale, 0.366em raise, 0.204em sink from the MATH table of Cambria Math`；Segoe UI 929、copy 1428、strike 4 span（thinnest 0.49pt——下标把删除线拆开，见待确认）。
  - **`--shapes` 几何**（探针 `%TEMP%/opencode/g3shapes.md` = `H~2~O x^2^ e~x~tra 2^10^ z[^1] mc<sup>2</sup>`）：后：正文基线 69.00；下标 `2` 宽 7.08、基线 72.67（沉 3.67pt＝0.204em）；上标 `2`、引注 `1`、`<sup>2</sup>` 宽均 7.08（≈0.73×正文 `2` 的 9.70）、基线均 62.41（升 6.59pt＝0.366em）。前：引注 `1`/`<sup>2</sup>` 宽 6.79（=0.7em）、基线 61.06（升 11.16pt＝0.62em）。即"与脚注引注号共用一套尺寸/位移"由同一 `ScriptMetrics` 保证。
  - **测试**：`cargo test --workspace --locked` → 251+36+99+117+53+4+7+64+19 全通过、0 失败（`rubrica-doc` 86→99：`tests/model.rs` 新增 13 个——`an_ascii_formula_reads_as_its_scripts`、`a_single_tilde_at_a_word_boundary_is_a_subscript_and_not_a_deletion`、`the_whitespace_of_a_script_must_be_escaped_and_an_escaped_one_is_not_a_break`、`an_escaped_mark_pairs_with_nothing_and_is_printed`、`a_mark_inside_code_is_the_authors_own_character`、`a_script_reads_across_emphasis_and_a_link`、`a_pair_does_not_reach_across_a_mark_that_another_pair_already_spent`、`a_stretch_of_two_marks_is_left_as_it_was_written`、`a_struck_word_inside_a_script_keeps_both_meanings`、`cjk_words_carry_scripts_as_their_ascii_twins_do`、`a_table_cell_reads_scripts_as_its_prose_does`、`a_script_and_a_citation_share_one_paragraph_and_one_size`、`no_run_of_marks_can_make_the_scripts_pass_panic_or_read_past_the_text`（后者为全组合 10^4 语料 + 千长串/200 层嵌套 + span/source/object/action 覆盖与有界不变量）；app 250→251：`theme.rs::script_metrics_are_learned_from_the_formula_face_or_fall_back_together`）；实现期间由测试抓出并修掉 2 个真 bug（整段被转义 piece 被误判为 run；replacement span 未排序致 NBSP 未并入邻 span）。
  - **闸门**：`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo clippy -p rubrica-app --all-targets --features pdf --locked -- -D warnings`、`cargo build --release --locked`、`git diff --check`——本轮收尾结果见 §5 最新一条。无新增 `#[allow]`，畸形输入有界（不 panic、不越界）。
  - **主动偏离 Pandoc（需人工确认）**：① 长度≥2 的 run 一律字面——Pandoc 的 `~~a~b~~`/`x~~~~y`/`a~~~b~~~c`/`x^^2^^` 是退化读法（空上下标/多吞标记），本实现维持 `~~` 删除线读法或字面；② `x^$a$^` 保持字面（Pandoc 为 Sup[Math]；本实现对象 span 只有 OBJECT 位，抬升无着落，宁字面不乱抬）；③ `x^`a^b`^` 读成 Sup[Code]（Pandoc 裂成 `Sup["`a"]` + 余段字面；本实现更整）。
  - **需人工确认的观感项**：① `~~a ~b~ c~~` 中下标把删除线拆成 4 段（thinnest 0.49pt）——横线在脚本处断开的观感；② 上标取非 cramped 的 `SuperscriptShiftUp`（750/2048），未用 `SuperscriptShiftUpCramped`（615），与引注/`<sup>` 同套普通文本基线；③ 窗口/`--peek`/PDF 目视（无头未验，按惯例留给桌面人工）。
- **master 复核通过（2026-10-17）**：四闸门亲跑全绿（test 251+36+99+117+53+4+7+64+19；clippy 默认+pdf、release、`git diff --check`）；抽查：`subsuper.md` 前后报告逐条一致、`--shapes` 探针几何逐位吻合（沉 3.67＝0.204×18dip、升 6.59＝0.366×18dip、宽 7.08＝0.73×9.70；前 6.79/61.06＝0.7em/0.62em）、`--source` 三文件逐字节相同、20 fixture `--report` 与 `--shapes` 全量对比各仅 3 文件有差且全部由引注/`<sup>` 改用 MATH 度量所致、复制语义以强调探针对照确认一致（语法标记不入复制文本，非回归）；修正本节 notes.md 行数表述（width 900 下 8 行）→ G3 判定完成，本地提交 `feat: read ^carets^ and ~tildes~ as scripts`。

### G4 明细（`==highlight==`）

- **验收口径**：`==text==` 渲染为荧光标记；明暗主题各有合适样式；设置里可关；代码/代码块内不生效。
- 现状核实（2026-10-17，源码 + 探针 crate `%TEMP%/opencode/g4probe` + pandoc 3.10 `-f markdown+mark -t native` 对照 42 例）：
  - **pulldown 侧**：`Options` 里只有 SUP/SUB/GFM/WIKILINKS/DEFINITION_LIST 等位，**没有** `==` 的扩展；本仓库选项组合（`STRIKETHROUGH|TASKLISTS|TABLES|MATH|FOOTNOTES`）下探针 42 例全部把 `=` 当**纯文本**（唯一特例是 `==\n...` 行首三连以上可能成 setext 下划线，但那已是标题语法，不在正文里）。`a == b`、`x==y==z`、`` `a == b` `` 都原样到达 `Builder`：G4 与 G3 同构 —— 扫描层在 `Block::close` 的 `split_definitions` 循环里做，不动 pulldown。
  - **Pandoc `mark` 扩展**（`+mark`；源码 `Markdown.hs`：`mark = inlinesBetween markStart markEnd`，`markStart = string "==" >> lookAhead nonspaceChar >> notFollowedBy (char '=')`，`markEnd = try (string "==")`，`inlinesBetween` 的 `innerSpace = try (whitespace <* notFollowedBy' end)` 是关键）：`==text==`→`Span("mark")`；**开标记只要求"后随非空白"**（`x ==a== y` 成立、段首成立；标记前不作任何要求）；**一个 `==` 只被紧跟其后的下一个 run 闭合、且闭合点前必须接文字**（`== a ==`、`==a ==`、`== a==`、`==text ==`、`== text==` 全字面；`a == b == c` 字面是因为每个 `==` 后都是空格；`==a == b==` 字面——中间 run 让候选卡死；`==a ==b== c==`→字面 `==a `+mark(`b`)+字面 ` c==`，尾 run 不能闭合已消费的 pair）；`==two words==` 成立（只有**首尾**是边界）；空内容字面（`== ==`、`====`）；转义 `\==` 不参与配对且反斜杠被吃掉（`\==text==`→字面 `==text==`）；跨代码 span 成立（`==\`code\`==`→mark[Code]）；对象（`$x$`）可被包；**软换行在内容内成立**（`==a\nb==`→mark[a, SoftBreak, b]；早前"因换行不成立"的记录系误测，2026-10-17 复测更正）；`====text====`/`===text===`→Pandoc 把长 run 拆成字面 `==`+mark，长 run 还会挡配对（`==a ==== b==`/`==a ====== b==` 字面、`==a==== b`→mark(a)+字面 `==`）；`x====y`/`a ====== b` 字面。
  - **fixture 现状**：全仓 `fixtures/*.md` 只有 `fixtures/mermaid-flow.md:20` 的 `n ==> o`（mermaid 箭头语法，在 mermaid 围栏内——那围栏是 `BlockKind::Code`，G4 不动它，回归对比会逐字节验证）。
  - **app 侧现状**：`InlineStyle` 只剩 bit 9 起可用（0..8 已用满：EMPHASIS..SUBSCRIPT），加 `HIGHLIGHT` = `1 << 9` 不冲突；显示列表有现成的 `Op::Rect`（`ColorRole` 填色）与 `bars` 通道（删除线横线、公式横线，按 baseline 相对坐标 → 在 `Op::Runs` 之后发），但**行内高亮要画在文字下面**，`bars` 的"后发"顺序会把底纹盖在字上——需在行循环里单独收一条"高亮带"并在 `Op::Runs` **之前**发；`ColorRole` 没有底色角色（Surface=代码面板，用它会不可读）。
- 方案（2026-10-17 定案，含偏离与取舍）：
  - **落点**：新增 `crates/rubrica-doc/src/highlight.rs`（`==` 成对扫描，仿 `emphasis.rs`/`scripts.rs`；复用 `emphasis::{Text, style_range, merge}` 与 `source_at`）。`Block::close` 的 `split_definitions` 循环里，`relax_emphasis` 之后、`relax_scripts` **之前**跑（高亮标记先被吃掉，脚本看到的文本更短；实测两序结果一致，写明顺序只为可复现）。表格 cell 同样跑（归到所属 block 的计数，与 `Scripts` 同策）。
  - **配对规则**（2026-10-17 按 Pandoc 源码逐条对齐；修正了方案初稿把开标记规则记反的一处——初稿写"开启点前不得是空白"，Pandoc 实际读的是后侧）：只认**长度恰为 2** 的 `=` run（`===`/`====` 一律字面，与 G3 的 `run≥2` 同哲学）；run 不得落在代码 span 内、不得被反斜杠转义。左到右一趟：拿一个 `==` 先试闭合栈顶未消费的 `==`（**必须紧跟它**——两者之间不得再有 run，任何中间的 `==` 都让这对候选作废；内容非空且**末字符非空白**），闭合不成则当开启点（**后随字符必须存在且非空白**；开标记前不作要求）。配对成功 → `style_range(内容, HIGHLIGHT)`，两个 `==` run 用 `emphasis::cut` 从文本里删除（marker 是语法，删掉才与 pulldown 自己消费的强调一致；复制文本里不留 `=`）。对象与文字同权：`==$x$==` 的高亮 span 覆盖 U+FFFC 占位符，与 Pandoc 的 `mark[Math]` 一致（不做对象特判）。
  - **偏离（有意，需人工确认）**：① 长 run 一律字面、不参与任何配对——Pandoc 会从长 run 边缘切出 `==` 成 mark（`====text====`、`==a====`），长 run 还会挡掉外层候选（`==a ==== b==` 字面）而这里挡不掉；② 被 Pandoc 其它内联单元吞掉的 `==`（链接文本/autolink/上标等；代码 span 已被这里排除）在这里是普通 run，交叉族因此与 Pandoc 不同（`==a [b==c](u) d==`、`==a ^b==c^ d==`：Pandoc 整段成 mark，这里 `==` 在其内部先成对）。③ 记录而非偏离：`==$x$==` 可被包（与 Pandoc 一致）；`==x^2^==` 两序一致（高亮先跑 → `^2^` 在 highlight 内被脚本抬升，反序同）。需要人工确认的是 ①②。
  - **开关**：菜单「文本阅读」子菜单在 **Show HTML image tags** 旁加 **Show `==text==` as a highlight** 复选项（最终文案；`Key::MenuHighlights`，11 语言），命令 `Command::Highlights(bool)`，持久化 `HKCU\Software\Rubrica\Highlights`（缺省=开），无头开关 `--no-highlights`。**关掉时按字面 `==text==` 呈现**（parse 期就不跑 pass：`ParseOptions.show_highlights`，与 `html_images` 同一条路），这是验收要求的读法，也为正对照数字服务。
  - **明暗两套样式**：新增 `ColorRole::Highlight`；`Palette` 加 `highlight: Rgb` 一字段（`ink(Highlight)` 返回它），亮色 `(0.98, 0.90, 0.42)`（暖黄荧光笔，纸 0.977 上可辨、文字 0.13 在它上面仍 ≥4.5:1），暗色 `(0.32, 0.28, 0.09)`（深琥珀，夜纸上不刺眼、文字 0.855 仍 ≥4.5:1）；单测对比度（与既有调色板对比度测试同一函数）。**窗口/PNG/PDF 三处同一角色**（PDF 走既有 `Op::Rect`→`DrawRectangle` 填色，PNG 走既有 `Op::Rect` 分支）。明暗自不必说：`Palette::of(dark)` 一处分叉。
  - **几何（实现定案）**：`BAND_TOP = 0.95`、`BAND_H = 1.25`（view.rs 常量，单位 em×`st.size`）——带顶在行 baseline 上方 0.95em、带高 1.25em，比 ink 高一点、比行距矮一点，带高与 face 无关（跨 fallback 可无缝合并）；相邻同 style 的 run 合并成一条带（`merge_rule` 族）。prose 逐行在 `Op::Runs` **前**发 `Op::Rect`（文字压在带上）；**表格 cell 改行级缓冲**（`paint_row` 记 `band_at` + `row_bands`，单元格循环把带换算成绝对像素入列表，行末按 y `sort_by(total_cmp)` 后 `ops.splice`）——因为非首列 cell 的带顶会高于前一 cell 的 Runs top，直接按 cell 顺序发会破坏 `ops_sorted` 非递减不变量（违反一次整页退化为逐帧遍历全部 ops）；prose 侧无需缓冲：块间距从上行底部起算（`y += panel_bottom + space_before`），带顶 ≥ 上一行底部恒成立。
  - **顺带修的既有小 bug**：`merge_rule` 原实现 `r.w += next.w` 在多词 span 上右端短缺（按跨过的空格数累计，实测 `~~struck phrase here~~` 规则终点 369.56，实际末词到 381.56）；改为左右取并集（`left=min(x)`、`right=max(x+w)`，兼容 RTL）。合并判定（style/top/h）未动 ⇒ `strike` 报告计数与 thinnest 不变，仅几何变长——文档注释本就写明 glue 要盖住整段。
  - **报告（实现定案）**：`--report` 在 `scripts` 行后加 `highlight    : {bands} span(s) in {marked} block(s) -> {rects} band(s) drawn`（doc 侧 `Highlights.bands` 与 ops 侧 `ColorRole::Highlight` 的 Rect 双侧计数；仅确有高亮时打印，仿 `scripts` 行）。`--shapes` 增印 `band         : left=… top=… width=… height=…`。
  - **fixtures**：新增 `fixtures/highlight.md`（基本与跨行短语、段首可开与贴字 `x==y==z`、CJK 与 RTL 词、首尾空白拒绝、比较链与长 run 字面（页上写明与 Pandoc 的差异）、转义两式、包强调/链接/公式/上标/删除线、行内代码与围栏字面、表格 cell、定义列表；同页可加 `--no-highlights` 对照），同步 `fixtures/README.md`、`README.md`、`IMPLEMENTATION.md`（G4 清单项 + 工作日志）；`tests/model.rs` 加 `tracked_highlight_fixture_keeps_its_bands` 把 fixture 本身钉进回归。
- 证据（2026-10-17，全部无头；before = `%TEMP%/opencode/rubrica-before-g4.exe`（md5 `474e3105e576e66a9d35c0d6e5388121`），after = `cargo build --release --locked` 产物副本 `%TEMP%/opencode/rubrica-after-g4.exe`）：
  - **落盘清单**：`rubrica-doc`：新 `src/highlight.rs`（`relax`/`runs`/`is_escaped`/`opens`/`wraps`/`overlaps`）、`src/lib.rs`（`mod highlight`、`InlineStyle::HIGHLIGHT = 1 << 9`、`Highlights { bands }`、`Block.highlights`、`relax_highlights`、`ParseOptions.show_highlights`、`close` 调用点、`piece` 迁移）、`src/emphasis.rs`（`cut` 改 `pub(crate)`）、`tests/model.rs`（16 语义测试 + 1 fixture 测试）。`rubrica-app`：`theme.rs`（`ResolvedStyle.highlight`、`ColorRole::Highlight`、`resolve_at`）、`view.rs`（`Palette.highlight` + `ink` + `ROLES` 18 项、`AppStyle.highlight`、`intern`/`intern_object`、`BAND_TOP`/`BAND_H`、prose 行循环内联发带、`paint_row` 行级缓冲与 `splice`、`merge_rule` 并集、`View.show_highlights`、`MenuState.show_highlights`、`Command::Highlights(bool)` + 菜单项 + `record_show_highlights` + `reparse`、ParseOptions 五处：view/report/peek/export_png/export_pdf）、`settings.rs`（`show_highlights`/`highlights_of`/`record_show_highlights` 存 `HKCU\Software\Rubrica\Highlights`）、`i18n.rs`（`Key::MenuHighlights` 11 语言）、`main.rs`（`--no-highlights` 三处）、`report.rs`（`Options.show_highlights`、`highlight` 行、`--shapes` 的 `band` 行与文档头）。
  - **测试**：`cargo test -p rubrica-doc --locked` 116 通过 / 0 失败（115 + `tracked_highlight_fixture_keeps_its_bands`）；`cargo test -p rubrica-app --locked` 256 通过 / 0 失败（251 + 5），新测试名：view.rs `a_band_stands_under_its_words_and_covers_the_whole_mark`、`a_highlighted_page_leaves_the_list_in_reading_order`（钉 `ops_sorted` 不变量）、`the_highlight_switch_off_leaves_the_marks_as_written`、`a_merged_rule_covers_the_spaces_its_span_crossed`、settings.rs `the_highlight_switch_is_on_until_it_is_turned_off`；既有对比度测试加两项（文字在带上：亮 12.29 / 暗 6.68；带对纸面：亮 ≈1.20 / 暗 ≈1.84）。
  - **新 fixture 前后数字**（`fixtures/highlight.md`）：before（`--report`，与 after 加 `--no-highlights` 逐字节相同）`15 blocks, 38 lines, height 1200pt`、无 `highlight` 行、Segoe UI 1372 glyphs、复制 2049 chars；after `15 blocks, 35 lines, height 1128pt`、`highlight    : 22 span(s) in 6 block(s) -> 23 band(s) drawn`（22 对标记、23 个 Rect——`==x^2^==` 的上标让一段带分成两段，其一高 16.43 = 1.25 × 0.73em）、Segoe UI 1284 glyphs、复制 1961 chars（差 88 = 22 对 × 4 个 `=`——标记不入复制文本，与强调一致）；`right-to-left: 4 char(s) on 1 line(s)`（`==שלום==`）。
  - **几何钉住**（`--report --shapes --width 900`，release）：`probe-band.md`（`A ==marked phrase here== tail.`）→ `highlight: 1 span(s) in 1 block(s) -> 1 band(s)`，`band : left=143.61 top=52.45 width=160.38 height=22.50`（左端 143.61 = 首词起点、右端 303.99 = 末词 `here` 精确结束，跨过的两处空格在带内）；`probe-cell.md`（表格两个 cell 各一对）→ 2 带，`left=197.05 width=92.33/83.70 top=54.52/100.42 height=22.50`（覆盖各自词与词间空格）；带高一律 22.50dip = 1.25em × 18dip、带顶 = 行 baseline − 0.95em。
  - **全量前后对比**（18 个既有 fixture + `reading.txt` + `README.md`，`--report` 默认宽度 与 `--report --shapes --width 900` 两式）：before 二进制复现 `%TEMP%/opencode/g4-baseline/` 38 份基线逐字节一致（方法学自证：report 基线为默认宽度、shapes 为 width 900）；after vs before 仅 `README.*` 有差（fixture 文件本身新增了命令行与条目说明，非引擎回归），其余 18 个 fixture 全部逐字节相同（含 `mermaid-flow.md` 围栏内 `n ==> o`、`subsuper.md`、`html.md`、`rtl.md`）。`--report --source` 对全部 20 个 md/txt 文件 before/after 逐字节相同。
  - **PNG 明暗像素实测**（`--export-png --export-width 900`，同一 fixture）：亮带像素 `(250,230,107)` = 0.98/0.90/0.42、暗带 `(82,71,23)` = 0.32/0.28/0.09（与 `Palette.highlight` 逐位一致）；带内最深/最亮像素亮 `(33,34,38)`、暗 `(218,219,223)` ⇒ 文字压在带上（z 序正确）；`--no-highlights` 导出全图 0 个带色像素（亮暗两色扫描）。PDF 冒烟（debug `--features pdf`）：导出成功，pdftotext 提取到 `the marked words`、`table cells`、`===three=== and ====four====` 与围栏原文；CJK/Hebrew 在 pdftotext 下不可提取是既有子集字体局限（`subsuper.md` 同样如此），非 G4 引入。
  - **需人工确认（无头不可验）**：① 长 run 字面偏离（Pandoc 会从 `===three===`/`====four====` 切出 mark；fixture 页内已写明）；② 链接文本/上标/autolink 内 `==` 的读法偏离（`==a [b==c](u) d==`、`==a ^b==c^ d==` 等交叉族）；③ 明暗观感（窗口/`--peek`/PDF 目视）；④ 表格 cell 行级缓冲属实现细节（观感无差）；⑤ `==x^2^==` 的带在上标处断成两段（设计使然）。
- **master 复核通过（2026-10-17）**：四闸门亲跑全绿（256+36+116+117+53+4+7+64+19；clippy 默认+pdf、release、`git diff --check`）；after 二进制与 `target/release` md5 同为 `44259aed…`；抽查 `highlight.md` 前后数字（15/35/1128、`22 span(s) -> 23 band(s)`、1961/2049）、`--no-highlights` 与 before 输出逐字节相同、`--shapes` band 几何逐位（143.61/52.45/160.38/22.50 与 cell 两带 197.05/92.33/83.70）、全量 21 文件仅新 fixture 有差（其余 20 个 report/shapes 前后逐字节相同、`--source` 0 差）、PNG 双主题像素实测（15169/15169/off 0）→ G4 判定完成，本地提交 `feat: draw ==marks== as a highlight`。

### G5 明细（主题系统）

- **验收口径**：用户主题文件（TOML）目录；`based_on` 继承；`[chrome]` 侧栏/标签条/分割线一起换色；坏文件只跳过坏键或整文件跳过并给原因；亮色与暗色各一个独立槽；切换只重绘不重排；"Copy as My Theme…"；存盘即生效。内置主题集可比 Telari 少但明暗两套齐备。
- 证据：见下方「进展」两条（成品清单 + 收尾证据）；结论：**等待 master 复核**。
- 现状核实（2026-10-17，源码逐条核对 `view.rs`/`theme.rs`/`settings.rs`/`main.rs`/`report.rs`/`peek.rs` + Telari 0.6.0 官方说明原文）：
  - 差距文档 §二.5 的现状陈述**成立**：`Ctrl+D` 三态（`dark_override: Option<bool>`，view.rs:3317/6083–6097）、4 种字面（`TextFace::ALL`）、3 种栏宽（`Measure::ALL`）；配色只有 `Palette::of(dark)` 一处硬编码（view.rs:373），无任何文件配置；窗口 chrome（标签条/侧栏/状态栏）颜色同为 `Palette` 字段、不可换。
  - 配色使用面：`Palette` 17 色 + `dark`；`ColorRole` 18 角色（theme.rs:113）由 `Palette::ink`（view.rs:436）解析；窗口、PNG（`write_png`:11941）、PDF（`pdf_color`:12170）三处同源；`peek.rs` 每次 reveal 重读 `settings::load()` 后 `Palette::of(dark)`（peek.rs:1634–1641）。
  - 窗口 chrome 现用角色：侧栏底=`Surface`、侧栏文字=`Muted`（`draw_tree_panel`:8820）；标签条底=`TabStrip`、hairline=`TabInactive`、非活动标签文字=`Muted`、活动药丸底=`Accent`/文字=`OnAccent`（`draw_top_bar`:8846）；状态栏底=`TabStrip`、其上 hairline=`TabInactive`（`draw_status_bar`:9042）。→ chrome 换色的最小实现是给这些点各自一个角色、默认值取现行值，默认渲染逐像素不变。
  - 主题文件：仓库零 toml/serde 依赖（`crates/rubrica-app/Cargo.toml`、`build.rs` 的 `default-features=false` 政策）；`rubrica-doc`/`rubrica-mermaid` 均为零依赖自写解析先例；`profiles.rs` 是注册表键值（非文件），`rubrica-workspace` 无文件层可复用。
  - Telari 侧核实（telari.app/whatsnew/0.6.0/ 原文）：`~/.config/telari/document-themes/*.toml`；最小文件=`[meta]` 的 name+appearance；`based_on` 链至出厂色；`[chrome]` 设侧栏/活动标签/列分割线背景与侧栏/标签条文字（文字省略时取主题正文色、背景回退系统灰）；坏值只跳过该键、完全无效文件不进列表；亮槽命名暗主题显示 "Invalid"；"Copy as My Theme…" 整份写出；存盘即生效；Finder 空格预览跟随主题；窗口骨架随主题。**Telari 主题文件的逐字键名无公开文档**（whatsnew/changelog 只有行为描述，检索无果），故本实现自定 schema（见下）；键名逐字兼容记为范围外、需人工确认。
- 方案（2026-10-17 定案，含备选与取舍）：
  - **落点**：新建 `crates/rubrica-app/src/themes.rs`（文档配色主题；与 `theme.rs` 的排版设计职责分开、与 `profiles.rs` 的注册表排版预设互不干扰），零新依赖——**自写 TOML 子集**（表头 `[meta]`/`[colors]`/`[chrome]`、`key = "value"`、整行 `#` 注释与值后注释；行式解析，风格对齐 `rubrica-mermaid`）。备选 A（拒绝）：`toml` crate——为一个不到 30 个键的文件层引整棵树（serde、索引、错误类型），而 build.rs 曾特意把 toml/serde 从依赖树上摘掉；备选 B（拒绝）：主题存注册表——验收明确要求"用户主题文件（TOML）目录"与"存盘即生效"，注册表改值不产生文件事件，也违背 Telari 的"drop a TOML file"用法。
  - **文件 schema（自定，README 写全）**：`[meta]`：`name`（必填、非空、≤80 字符）、`appearance`（必填、`light`|`dark`、大小写不敏感）、`based_on`（可选；内置名或用户主题名，须同 appearance）；`[colors]` 14 键：`bg/text/muted/accent/code_bg/page_surface/page_shadow/on_accent/keyword/string/comment/number/ty/highlight`；`[chrome]` 9 键：`sidebar/sidebar_text/tab_strip/tab_text/active_tab/active_tab_text/tab_inactive/tab_hover/divider`；颜色接受 `#rgb`/`#RRGGBB`。省略的键沿 `based_on` 继承、链尾是出厂配色（`Palette::of`）；`[chrome]` 未写的键按主题自身颜色取默认：sidebar←code_bg、sidebar_text←muted、tab_text←muted、active_tab←accent、active_tab_text←on_accent、divider←tab_inactive（改一处 accent 药丸跟着走；默认主题因此逐字段等于现行 `Palette::of`）。`inline_code` 色不在本条目范围（Rubrica 行内代码与正文同墨，样式归 G6）。
  - **主题目录**：`%APPDATA%\Rubrica\themes\*.toml`（Windows 惯例对应 `~/.config/telari/document-themes/`）；目录不存在=只有内置主题；创建于「Copy as My Theme…」与「Open Themes Folder」时（启动不新建、不扫坏盘）。扫描有界：≤256 个文件、单文件 ≤64 KiB、非 UTF-8 整文件跳过给原因；文件按名排序、内置在前。
  - **模型**：`Appearance{Light,Dark}`；`Theme{name, appearance, based_on, origin(内置/路径), colors: 14×Option<Rgb>, chrome: 9×Option<Rgb>, skipped: Vec<String>}`；`resolve` 从 `Palette::of(appearance)` 起按 based_on 链（根→叶）逐字段覆盖，chrome 语义默认在页面色定案后结算。环/过深（>8）→整文件跳过给原因；`based_on` 跨 appearance 或指名不存在的主题→整文件跳过给原因；坏键→只跳过该键并把 `表.键 = "值"：原因` 记入该主题的 skipped（未知键、未知表同理）。
  - **新角色（默认像不变）**：`Sidebar`（默认=code_bg 值，侧栏底）、`SidebarText`（默认=muted）、`TabText`（默认=muted，非活动标签文字）、`TabActive`（默认=accent，活动药丸底）、`OnTabActive`（默认=on_accent，其文字）、`Divider`（默认=tab_inactive 值，标签条下/状态栏上 hairline）——`ColorRole` 18→24、`Palette` 17→23 色、`ROLES` 表同步；绘制点只换角色名，默认值逐字段相等 ⇒ 默认渲染不变（前后 PNG 逐像素比）。
  - **内置主题 3 对明暗齐备**：`Default Light`/`Default Dark`（零覆盖，逐字段等于现行 `Palette::of`）、`Mono Light`/`Mono Dark`（Telari 默认主题的致敬：黑白灰、只有代码保留明度差；浅纸 `#FAFAF8`/深纸 `#262626`）、`Solarized Light`/`Solarized Dark`（原方案色 base03–base3 + 8 强调色；正文 base00↔base0、注释 base01、面板 base2/base03）。编译进二进制（非文件），避免分发问题；`--list-themes` 与菜单与用户文件同列。
  - **槽位**：新注册表字符串 `HKCU\Software\Rubrica\ThemeLight`/`ThemeDark`（`settings.rs` 的 `text`/`write_text` 通道）；空/缺省=该外观内置默认（选默认时写空串回槽）。槽命名不存在或跨外观的主题→回退该外观默认 + 菜单内灰行提示（仿 `encoding_notice`，i18n `ThemeMissing`/`ThemeWrongAppearance`）；`--report` 只在"有主题在生效"（`--theme` 给出，或槽非默认/失效）时打印 `theme` 行，保证无主题时既有 fixture 的 `--report` 输出逐字节不变。
  - **只重绘不重排**：切主题/坏文件重载只走新的 `set_palette`（整张 `Palette` 相等则零工作）——清 `brushes`/`sel_brush`/`hit_brush`/`focus_brush`/`frame_brush`、重设 edit 框背景、`apply_dark_titlebar`、`InvalidateRect`；`layout_epoch` 不动、不 relayout（`Op` 只带 `ColorRole` 不带 RGB，颜色切换构造上不触布局——G1 已定的同一条性质）；`Ctrl+D` 三态路径不变。
  - **存盘即生效**：复用 `DOCUMENT_TIMER`（700ms）tick，除 `reload_if_written` 外加 `refresh_themes`：主题目录签名（文件名+长度+mtime 排序集合）变化才重读文件并重解析当前槽；不变则零工作。不新起 watcher 线程。
  - **菜单（i18n 6 新键 ×11 语言）**：主菜单加 `Colour theme` 子菜单，含 `Light theme`/`Dark theme` 两个槽子菜单（列出同 appearance 的全部主题、勾选当前）、`Copy as My Theme…`（把当前解析后主题整份写成 `<名> Copy.toml`，唯一化；写后刷新并把当前槽指到副本——对话框交互属桌面人工检查）、`Open Themes Folder`（建目录 + ShellExecute 打开）。槽失效/坏文件时在对应子菜单顶插灰行说明原因。
  - **无头 CLI**：`--theme <name|path>`（给定时由主题自身 appearance 决定明暗；与 `--dark` 冲突在无头命令中报错退出；名字/文件解析失败：无头命令报错退出、GUI 路径静默回退默认——不进文件不得阻止开窗）、`--list-themes`（打印内置+用户主题清单、来源、based_on、被跳过键与整文件丢弃原因后退出，不建窗）；`--export-png`/`--export-pdf` 的内层 `dark: bool` 改为解析后的 `Palette`（默认路径结果不变）；`TAKES_VALUE` 加 `--theme`。
  - **验收对照**：目录+坏文件→扫描与原因（`--list-themes`/`theme` 行/单测）；based_on→链解析+环/跨外观拒绝单测；[chrome]→三色角色+默认像不变（PNG 像素）；双槽→子菜单+注册表往返单测；只重绘→`set_palette` 不动 `layout_epoch`（单测钉住）；Copy as My Theme→写文件 round-trip 单测（copy→重解析→palette 全等）；存盘即生效→签名变化与重解析单测 + tick 接线；预览跟随→peek 读同一槽；内置明暗齐备→3 对。
- 进展（2026-10-17）：before 二进制已备份（`%TEMP%/opencode/rubrica-before-g5.exe`，md5 `44259aed1a28ad0e23af810134ed3f5f` = `rubrica-after-g4.exe` = HEAD `628562d` 的 release 产物）；方案落盘；`themes.rs` 写完（21 单测）；全部接线落盘并过编译：`view.rs`（`Palette` 23 色 + `PartialEq` + `settle()`、6 新 `ColorRole`、`ROLES` 24 项、`draw_tree_panel`/`draw_top_bar`/`draw_tab_overflow`/`draw_status_bar` 换角色、`View.themes/themes_stamp/theme_slots/theme_pin`、`opening_palette`、菜单（`Colour theme` 两槽子菜单 + 灰行 + Copy + Open Folder）、`Command::ColorTheme/CopyAsMyTheme/OpenThemesFolder`、`set_palette` 只重绘、tick `refresh_themes` 签名比较、`export_png/export_pdf/write_png/write_pdf/pdf_*` 改收 `&Palette`、测试 helper 与 `new_view` 三参）、`settings.rs`（`theme_slot`/`record_theme_slot`，`ThemeLight`/`ThemeDark`）、`i18n.rs`（7 新键 ×11 语言 + `slot_fault`）、`main.rs`（`mod themes;`、`--list-themes` 早退分支、`--theme` 三段（PNG/PDF 严格、report 条件行）、`TAKES_VALUE` 13 项、GUI `view::run(path, extra, theme)`）、`report.rs`（`Options.theme` + 条件 `theme` 行）、`peek.rs`（shelf+stamp 缓存、按槽解析 palette）。测试：workspace 全绿（app 277，含 themes 21 + contrast 扩展为 8 张 palette：默认对+6 内置）；clippy 默认 `--all-targets -D warnings` 全绿。Solarized 对为过 4.5:1 已加深/上调了 quiet inks 与标签色（沿用既定"对比条优先"口径，README 将注明 deviation）。下一步：前后证据与文档同步。
- 进展（2026-10-17，收尾证据；输出存 `%TEMP%/opencode/g5-evidence/`）：after release 二进制 md5 `a057aeac190a2b6b5047a858ba03581b`。**默认像不变**：21 fixture（20 md + 1 txt）× `--report`/`--report --shapes` 共 42 份与 before 逐字节相同（无 `theme` 行泄漏）；纯默认 PNG 导出与 before 逐字节相同（md5 `9631ca9b7e8304e7111fd5a603e87305`）；`--theme "Mono Dark"` PNG 背景 `(38,38,38)` 463600 px、`Solarized Light` 背景 `(253,246,227)` 463598 px、纯默认本帧 0 px。**坏文件电池**（`%TEMP%/opencode/g5-apdata/Rubrica/themes`，22 文件，`APPDATA` 重定向隔离）：`--list-themes` = 6 built-in / 11 read / 11 not read，原因逐条正确（9 层链超深、based_on 环、跨 appearance、base 不存在、坏 UTF-8、超 64 KiB、无 name、非法 appearance、撞内置名、重名、非 toml 忽略）；坏键只跳该键并附诊断行、原始值不套引号。**CLI**：`--theme "Mono Dark" --report` → `theme : Mono Dark (built-in)` exit 0；`--theme Chain` → `Chain (path), based on Good`；`--theme <路径>` → `Good (path…), based on Mono Light`；`--theme Nope` exit 1 "no theme here is named `Nope`"；`--theme "Mono Light" --dark` exit 1 "`Mono Light` is a light theme, and `--dark` asks for a dark one"；名字大小写不敏感（`MONO DARK` 导出 exit 0）。**槽位往返**（真实 `HKCU\Software\Rubrica`，用完已删净、`reg query` 无 Theme\*）：`ThemeDark="Mono Dark"` → `--list-themes` `slot dark   : Mono Dark`、`--report --dark` `theme      : Mono Dark (built-in)`、`--export-png --dark` 背景 `(38,38,38)` 906915 px；`ThemeLight="Gone"` → `slot light   : Gone (not installed)`、report `Default Light (built-in); `Gone` is not installed`；`ThemeLight="Mono Dark"` → `slot light   : Mono Dark (a dark theme)`、report 同口径；清空后 `slot light   : (the default: Default Light)` 且 report 无 theme 行（`headless.asked` 口径成立）。README.md 新增「Colour themes」一节（内置 6 主题、Solarized deviation、菜单两槽/Copy/Open Folder、文件 schema 与 14+9 键、based_on 8 层、坏文件口径、`--theme`/`--list-themes`）；IMPLEMENTATION.md 验收清单与工作日志同步。剩余：PDF 冒烟 + 收尾四闸门。
- 进展（2026-10-17，PDF 冒烟与收尾闸门）：**PDF 特性闸门抓到一处真漏**——`view.rs` 约 13127 行的 `Op::Diagram` PDF 分支没跟上 G5 的签名改动，还传 `dark`（默认特性编译不进这段代码，故此前全绿）：`pdf_shape_ops(..., palette)` 与 `pdf_emit_runs(..., palette)` 修好后 `cargo build --release --locked --features pdf` 通过。冒烟：`--export-pdf --theme "Mono Dark"`（`fixtures/chemistry.md`、`fixtures/notes.md`）exit 0，默认对照同样；`pdftotext` 两主题提取的文本层逐字节相同（notes.md 两文件同 113 B；chemistry.md 同 723 B）；自写 stdlib 脚本解压 PDF 内容流取色（`%TEMP%/opencode/pdf-colors.py`）：Mono Dark notes = `bg #262626 ×3、text #DCDCDC ×371、code_bg #1E1E1E ×1、muted #A0A0A0 描边 ×11`，默认 notes = `bg #F9F8F6 ×3、text ×371、code_bg（0.937,0.935,0.928）×1、muted（0.36,0.38,0.41）×11`——全部为对应 palette 值，证明主题确实进 PDF。**收尾四闸门全绿**：`cargo test --workspace --locked` 277+36+116+117+53+4+7+64+19；`cargo clippy --workspace --all-targets --locked -- -D warnings`；`cargo clippy -p rubrica-app --all-targets --features pdf --locked -- -D warnings`（修后）；`cargo build --release --locked`（最终默认二进制 md5 `a23a8a55d6ee078546ef2768db25ba4f`）；`git diff --check` 干净。最终二进制回归复验：21 fixture × `--report`/`--report --shapes` 42 份与证据期 `a057aeac…` 的输出逐字节相同；默认 PNG（`fixtures/reading.md`）md5 `9631ca9b7e8304e7111fd5a603e87305` 在 before / `a057aeac…` / 最终三方全等；`--theme "Mono Dark"` PNG md5 `35fc496a122b102ec8f434b6f2bd61f4` 与证据期全等。**结论：等待 master 复核**（若要复跑：见 §1 复核要点；`g5-evidence/` 内 `final-*` 为最终二进制产物）。人工确认项未变：菜单/对话框/窗口内 `[chrome]` 观感、存盘即生效的窗口实测、逐字键名自定（Telari 无公开文档）。
- **master 复核通过（2026-10-17）**：四闸门亲跑全绿（277+36+116+117+53+4+7+64+19；clippy 默认+pdf、release、`git diff --check`）；抽查：默认态 21 文件 × report/shapes/source 全 0 差异、无 `theme` 行；`--theme`/`--list-themes` 行为与报错（缺失名、跨外观、大小写）逐条正确；坏文件电池 6 built-in/11 read/11 not read 与全部原因逐条；默认 PNG md5 与 before 逐字节相同、`Mono Dark` PNG md5 一致、像素 463600/463598/默认 0；主题下 notes.md 几何逐字节相同（只重绘不重排）；PDF 冒烟文本层全等、内容流取色逐色对应 → G5 判定完成，本地提交 `feat: colour the reader from theme files`。

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

- 2026-10-17 master：G5 复核通过——四闸门亲跑全绿（277+36+116+117+53+4+7+64+19）；抽查默认态 21 文件 report/shapes/source 全 0 差异、`--list-themes`/槽/报错文案、坏文件电池 11/11 原因逐条、PNG md5（默认 `9631ca9b…` 与 before 全等、Mono Dark `35fc496a…`）与像素清点（463600/463598/默认 0）、主题下几何逐字节不变、PDF 文本层全等与内容流取色对应两套 palette → G5 标 `已完成` 并本地提交 `feat: colour the reader from theme files`；恢复区切至 G6（表格与行内代码样式）。
- 2026-10-17 G5 子 agent（`ses_eb823b6e3ffepvxYjxA3gIoPxl`）：**G5 完成，等待 master 复核**。收尾证据全存 `%TEMP%/opencode/g5-evidence/`：21 fixture × report/shapes 42 份与 before 逐字节；默认 PNG `9631ca9b…` 在 before/证据期/最终三方全等；Mono Dark 背景 `(38,38,38)` 463600 px、Solarized Light `(253,246,227)` 463598 px；注册表双槽往返（`--list-themes`/`--report`/`--export-png --dark`，含 not installed 与 a dark theme 两条故障行；用完即删净）；22 文件坏文件电池 11 read / 11 not read；`--theme` 名字/路径/大小写不敏感/两处严格报错全过。**PDF 冒烟抓到并修掉一处真漏**：`Op::Diagram` 的 PDF 分支漏跟 G5 签名改动（默认特性编不到这段代码，pdf 构建/clippy 才暴露，`dark`→`palette` 两处），修后 `--export-pdf --theme "Mono Dark"` 内容流取色 = 该 palette（bg #262626 / text #DCDCDC / code_bg #1E1E1E / muted #A0A0A0），默认对照 = 默认浅色，两主题文本层逐字节相同。四闸门全绿：test 277+36+116+117+53+4+7+64+19、clippy 默认 + `--features pdf`、release build（最终 md5 `a23a8a55d6ee078546ef2768db25ba4f`，42 报告 + 两种 PNG 与证据期输出逐字节复验相同）、`git diff --check`。README 新增「Colour themes」一节、IMPLEMENTATION 同步。未 commit、未 push；人工确认项（菜单/对话框/窗口内 `[chrome]` 观感、存盘即生效窗口实测、自定键名）记在 §3-G5。

- 2026-10-17 G5 子 agent（`ses_eb823b6e3ffepvxYjxA3gIoPxl`）：G5 全链落盘并过编译与测试，等待收尾证据。`themes.rs`（自写 TOML 子集解析/链解析/目录扫描/6 内置 3 对/`headless`/`describe`/`copy_into`/`stamp`，21 单测）；`view.rs`（`Palette` 23 色 + `PartialEq` + `settle()`；6 新角色 Sidebar/SidebarText/TabText/TabActive/OnTabActive/Divider + `ROLES` 24 项 + 四处绘制点换角色；`View.themes/themes_stamp/theme_slots/theme_pin`；菜单 `Colour theme`→两槽子菜单（勾当前、失效灰行）+ `Copy as My Theme…` + `Open Themes Folder`；`Command::ColorTheme/CopyAsMyTheme/OpenThemesFolder`；`set_palette` 只重绘不清布局；tick `refresh_themes` 签名比较；`export_png/pdf` 链路改收 `&Palette`）；`settings.rs`（`theme_slot`/`record_theme_slot` → `ThemeLight`/`ThemeDark`，空串=默认）；`i18n.rs`（7 新键 ×11 语言 + `slot_fault`）；`main.rs`（`--list-themes` 早退、`--theme` 接入 PNG/PDF（严格）与 report（条件 `theme` 行）、`TAKES_VALUE` 加 `--theme`、GUI 传 `view::run(..., theme)`）；`report.rs`（`Options.theme` + 条件行）；`peek.rs`（shelf+stamp 缓存、按槽解析、palette 变化才清笔刷）。测试 `cargo test --workspace --locked` 277+36+116+117+53+4+7+64+19 全绿；对比度测试扩展为 8 张 palette（默认对 + 6 内置）且新增侧栏/标签/活动标签三对，Solarized 对为过 4.5:1 加深 quiet inks 与标签色（既定"对比条优先"口径）；clippy 默认 `--all-targets -D warnings` 全绿。修实现两处小问题：非法 `appearance` 值现在作为**文件级**丢弃原因（而非跳过该键后报"无 appearance"）；颜色诊断行的原始值不再套双引号；`Copy` 命名改为计数不堆叠（`X Copy`→再拷得 `X Copy 2`）。下一步：前后证据（report 逐字节/PNG 像素/坏文件电池）+ README/IMPLEMENTATION 同步。

- 2026-10-17 G5 子 agent（`ses_eb823b6e3ffepvxYjxA3gIoPxl`）：现状核实与方案落盘 §3-G5——源码逐条核对（配色唯一入口 `Palette::of`、chrome 三处绘制点、PNG/PDF/peek 同源、零 toml/serde 依赖政策）+ Telari 0.6.0 原文核实（目录/based_on/[chrome]/坏文件/双槽/Copy/存盘即生效/预览跟随；**逐字键名无公开文档**，自定 schema 并记"需人工确认"）；方案定案：新建 `crates/rubrica-app/src/themes.rs` 自写 TOML 子集（备选引 `toml` crate 与存注册表均拒绝并写明理由）、`%APPDATA%\Rubrica\themes\*.toml`、6 个新 ColorRole（Sidebar/SidebarText/TabText/TabActive/OnTabActive/Divider，默认=现行值保证默认像不变）、内置 3 对（Default/Mono/Solarized × 明暗）、`ThemeLight`/`ThemeDark` 槽、菜单两槽子菜单 + Copy as My Theme + Open Themes Folder（i18n 6 键 ×11 语言）、`--theme`/`--list-themes`、`--report` 条件 `theme` 行、tick 签名热重载只重绘。before 二进制 `%TEMP%/opencode/rubrica-before-g5.exe`（md5 `44259aed…` = after-g4 = HEAD `628562d`）已备份。下一步：写 `themes.rs`。

- 2026-10-17 master：G4 复核通过——四闸门亲跑全绿（256+36+116+117+53+4+7+64+19）；抽查 `highlight.md` 前后数字、`--no-highlights` 逐字节、`--shapes` band 几何逐位、21 文件全量对比（仅新 fixture 有差）、`--source` 0 差、PNG 亮/暗/关三态像素（15169/15169/0）→ G4 标 `已完成` 并本地提交 `feat: draw ==marks== as a highlight`；恢复区切至 G5（主题系统）。
- 2026-10-17 G4 子 agent（`ses_eb87cf3f6ffeUXsJ2VkcRRqx7z`）：**G4 全链完成，等待 master 复核**。实现落盘（`theme.rs`：`ResolvedStyle.highlight`/`ColorRole::Highlight`/`resolve_at`；`view.rs`：`Palette.highlight` + `ROLES` 18 项、`AppStyle.highlight`、`intern`/`intern_object`、`BAND_TOP=0.95`/`BAND_H=1.25`、prose 带在 `Op::Runs` 前发、`paint_row` 行级缓冲 + 行末 `ops.splice` 保 `ops_sorted`、`merge_rule` 改左右并集（顺带修多词 span 右端短缺的既有 bug，strike 计数/thinnest 不变）、`View/MenuState.show_highlights`、`Command::Highlights(bool)`、`record_show_highlights`/`reparse`、ParseOptions 五处；`settings.rs`：存 `HKCU\Software\Rubrica\Highlights`；`i18n.rs`：`Key::MenuHighlights` 11 语言；`main.rs`：`--no-highlights` 三处；`report.rs`：`highlight` 行 + `--shapes` `band` 行）。测试：`cargo test --workspace --locked` 256+36+116+117+53+4+7+64+19 / 0 失败（app 新增 5：`a_band_stands_under_its_words_and_covers_the_whole_mark`、`a_highlighted_page_leaves_the_list_in_reading_order`、`the_highlight_switch_off_leaves_the_marks_as_written`、`a_merged_rule_covers_the_spaces_its_span_crossed`、`the_highlight_switch_is_on_until_it_is_turned_off`；doc 新增 `tracked_highlight_fixture_keeps_its_bands`；对比度测试加文字在带上 12.29/6.68）。对照：before 二进制复现 38 份基线逐字节一致；18 个既有 fixture 前后 `--report`/`--report --shapes` 逐字节相同（仅 README.md 有差——fixture 文档本身改过）、`--report --source` 20 文件逐字节相同；highlight.md before/off `15/38/1200pt`、Segoe UI 1372、复制 2049 → after `15/35/1128pt`、`highlight    : 22 span(s) in 6 block(s) -> 23 band(s) drawn`、1284/1961（差 88 = 22×4 标记不入复制）；`--no-highlights` 输出与 before 逐字节相同。几何/像素：`--shapes` 探针 `band : left=143.61 top=52.45 width=160.38 height=22.50`、cell 探针 2 带（`left=197.05 width=92.33/83.70`）；PNG 亮带 `(250,230,107)`、暗带 `(82,71,23)`，带内最深/最亮像素证明文字压在带上，`--no-highlights` 0 带色像素；PDF 冒烟正常（pdftotext 提取到正文与字面长 run）。四闸门全绿（test、clippy 默认 + `--features pdf`、release build、`git diff --check`）。需人工确认（无头不可验）：长 run 字面偏离、链接/上标/autolink 内 `==` 读法偏离、明暗观感、cell 行级缓冲（实现细节）、`==x^2^==` 带断两段——均记 §3-G4。未提交、未 push。
- 2026-10-17 G4 子 agent（`ses_eb87cf3f6ffeUXsJ2VkcRRqx7z`）：`rubrica-doc` 侧完成——新 `highlight.rs` + `lib.rs` 接线 + `tests/model.rs` 16 个 G4 测试全绿（`cargo test -p rubrica-doc --locked` 115 通过 / 0 失败）。**修正一处方案错误**：初稿"开启点前不得是空白"与 Pandoc 实际相反；拉取 Pandoc 3.10 源码核实真规则（`markStart = string "==" >> lookAhead nonspaceChar >> notFollowedBy (char '=')`、`markEnd = try (string "==")`、`inlinesBetween` 的 `innerSpace` 在"空白后紧跟 `==`"时让整个候选回溯）并用 60+ 例复测（`x ==a== y` 成立、段首成立、`==a ==b== c==`→`==a `/mark(b)/` c==`、`==a == b==` 全字面、`==a\nb==` 软换行在内容内成立——早前"因换行不成立"记录系误测、`==\=a==`→mark(`=a`)）；新工具语料：`%TEMP%/opencode/g4rules2.txt`、`g4rules3.txt`（pandoc `+mark` 对照）、`g4cmp/`（本仓库读法电池，45 例逐例比对一致；仅长 run 与链接/上标内 `==` 两族按已声明偏离）。§3-G4 的现状核实、配对规则、偏离清单已同步更正。
- 2026-10-17 G4 子 agent（`ses_eb87cf3f6ffeUXsJ2VkcRRqx7z`）：G4 现状核实与方案落盘 §3-G4——pulldown 探针 42 例（本仓库选项组合下 `==` 全程纯文本，唯一例外是行首 setext 下划线，不在正文）、pandoc 3.10 `+mark` 对照（intraword 成立、内容首尾禁空白、开启点前禁空白、跨代码 span 成立、长 run 拆读）、`fixtures/` 仅 `mermaid-flow.md` 的 `n ==> o`（围栏内，不动）；方案定案：新 `crates/rubrica-doc/src/highlight.rs`（恰 2 长度 run、栈式贪心、内容首尾空白拒绝、`emphasis::cut` 删标记）、`InlineStyle::HIGHLIGHT`（bit 9）、`ParseOptions.show_highlights`、`ColorRole::Highlight` + `Palette.highlight`（亮 `0.98,0.90,0.42` / 暗 `0.32,0.28,0.09`）、高亮带在 `Op::Runs` 前发 `Op::Rect`、菜单开关 + 注册表 + 11 语言 + `--no-highlights`、`--report` 加 `highlight` 行、`--shapes` 印几何、新 `fixtures/highlight.md`。基线：`rubrica-before-g4.exe`（md5 `474e3105e576e66a9d35c0d6e5388121`）已备份，20 fixture 的 `--report`/`--report --shapes --width 900` 基线（38 份）落在 `%TEMP%/opencode/g4-baseline/`；`cargo test --workspace --locked` 基线全绿（251+36+99+117+53+4+7+64+19，0 失败）。下一步：实现 `rubrica-doc` 侧。
- 2026-10-17 master：G3 复核通过——四闸门亲跑全绿（251+36+99+117+53+4+7+64+19）；抽查 `subsuper.md` 前后报告、`--shapes` 几何逐位、`--source` 逐字节、20 fixture `--report` 与 `--shapes` 全量（各仅 3 文件有差且全为引注/`<sup>` 的 MATH 度量所致）、复制语义对照强调探针（一致，标记不入复制文本）；修正 §3-G3 的 notes.md 行数表述（width 900 下 8 行）→ G3 标 `已完成` 并本地提交 `feat: read ^carets^ and ~tildes~ as scripts`；恢复区切至 G4。
- 2026-10-17 G3 子 agent（`ses_eb8b011c0ffelOWA9muJltqYqS`）：G3 实现、验收与文档收尾完成，§1 改「等待 master 复核」。app 侧落地（`math.rs::script_constants`、`theme.rs::ScriptMetrics`+`learn_scripts`+`resolve_at` 同源、`report.rs` scripts 行、`view.rs`/`report.rs`/`peek.rs` 共 11 处调用点）；`fixtures/subsuper.md` 与 `fixtures/README.md`/`README.md`/`IMPLEMENTATION.md` 同步。前后二进制（`%TEMP%/opencode/rubrica-before-g3.exe` / `rubrica-after-g3.exe`，均与各自 target 产物逐字节一致）对 19 fixture + `reading.txt` 的 20 份 `--report`/`--shapes` 全量对比：仅 `html.md`（1 行 extent 630.4→630.7pt）、`notes.md`（6 行 ≤0.3pt）与新 fixture 有差，全部差异来自引注/`<sup>/<sub>` 改用 MATH 值（验收要求本身）；`--source` 前后逐字节相同；探针 `--shapes` 钉住：下标宽 7.08（0.73em）、沉 0.204em；上标/引注/`<sup>` 同宽 7.08、升 0.366em（前 6.79 / 0.62em）；subsuper 报告 `scripts: 5 raised, 5 lowered`（前无该行；census 947→929、copy 1448→1428、strike 2→4）。收尾闸门全绿：test 251+36+99+117+53+4+7+64+19、clippy 默认与 `--features pdf`、release build、`git diff --check`；偏离与观感待确认项已入 §3-G3。改动未提交，等 master 复核。
- 2026-10-17 G3 子 agent（`ses_eb8b011c0ffelOWA9muJltqYqS`）：`rubrica-doc` 侧实现完成并自测全绿——新 `scripts.rs`（后缀扫描 + 栈式贪心配对 + 转义/代码/对象/宽度≥2/已消费分隔符五道拒绝 + `\ `→NBSP；`apply` 排序后 `merge`，spans/sources/objects/actions 同步搬家）；`Builder` 单 `~` 解偷（`strike_literal` 栈，起止 run 皆 1 个未转义 `~` 时不压 `STRIKETHROUGH`）；`Block::close` 里正文与表格 cell 各跑一次。`tests/model.rs` 新增 13 个测试（Pandoc 对照、快照全组合 10^4 语料、长串/深嵌套、source map 与 span 覆盖不变量），跑出并修掉 2 个真 bug（整段被转义 piece 误判为 run；replacement span 未排序导致 NBSP 未并入邻 span）与 3 个测试断言问题（`x\^y^z` 期望按 pandoc 实测定为字面、`find('d')` 撞 `and` 的 d、引用号与上标合并成一条 run）。`cargo test -p rubrica-doc --locked`：99 通过 / 0 失败。
- 2026-10-17 G3 子 agent（`ses_eb8b011c0ffelOWA9muJltqYqS`）：现状核实与方案落盘 §3-G3——pulldown 探针（单 `~` 会被当 Strikethrough 偷走、`event_range` 是整个 span、`ENABLE_SUBSCRIPT` 不满足 Pandoc intraword）、pandoc 3.10 约 60 例对照（`\ `→NBSP、`\^`/`\~` 不参与配对、跨代码 span、交叉配对拒绝、栈式贪心）、Cambria Math 常量 73/750/418；方案定案（新 `scripts.rs` 后缀扫描 + `Builder` 解偷 + MATH 常数化 `ScriptMetrics` + `--report` scripts 行 + `fixtures/subsuper.md`），偏离项（run≥2 字面、对象不与配对）已写明。开始实现 `rubrica-doc`。
- 2026-10-17 master：G2 复核通过——四闸门亲跑全绿（250+36+86+117+53+4+7+64+19）；抽查 `fixtures/html.md` 前后数字、`--source` 逐字节、18 个既有 fixture 报告 0 差异、三条 `--shapes` glyph run 全部一致；G2 标 `已完成` 并本地提交 `feat: read html by what its tags mean`；恢复区切至 G3。另记：昨日 master 误用 `--version` 裸启动 GUI 副本（开窗/占托盘），用户手动清理且自始未受影响——「验证只用无头模式」已写入 §0。
- 2026-10-16 晚 用户关机暂停：G2 子 agent 已自报完成（实现+验收 1–7+四闸门自绿，改动未提交）；master 遵从用户指示不做复核、不派新子 agent；暂停记录与恢复步骤已写入 §1，复核与提交推迟到下次开工。
- 2026-10-16 G2 子 agent（`ses_ebb6df8d8ffexGa2NoiqL32Gag`）：G2 实现与验收 1–7 全部完成，四闸门全绿（test 250+36+86+117+53+4+7+64+19、clippy 默认与 `--features pdf`、release build、`git diff --check`；无新增 `#[allow]`）；`fixtures/html.md` + `html-fig.png`/`html-wide.png` + `fixtures/README.md`、`README.md`、`IMPLEMENTATION.md` 同步；前后数字钉上验收：html.md `21 blocks/1637pt/1 code block` → `18/1719pt/3 code blocks + images 4/3/1`，zoom 两分支 `486.0×202.5pt`（无 zoom，栏宽收缩）vs `469.8×195.8pt`（87% 先于栏宽适配）；`--shapes` 三条 glyph run（`<sub>` +3.6dip、`<sup>` 与脚注引注号同为 −11.16dip、宽 6.79）钉住验收①；16 个既有 fixture（含 mermaid×9、notes）前后报告逐字节一致、`--source` 视图逐字节一致；报告 `images` 行与脚注 census 修正（`mc<sup>2</sup>` 不再误计引注）落盘。§1 改「等待 master 复核」，简报已出。

- 2026-10-16 G2 子 agent（`ses_ebb6df8d8ffexGa2NoiqL32Gag`）：现状核实完成并落 §3-G2（差距文档 §一.2 三条说法：整块 HTML 当文本画=**不成立**（那是源码视图路径）、HTML 图片不显示=**部分不成立**（块内已出图，行内/zoom 缺失）、`<sub>/<sup>` 不成上下标=**成立**）；方案定案（u16+`SUBSCRIPT`、计数器式行内样式、CJK 感知分隔、完整 `<table>/<details>` 转源码 Code 块、`ObjectKind::Image` 加 `html/zoom`、菜单开关+11 语言、`--report` images 行）；改动前二进制备份 `%TEMP%/opencode/rubrica-before-g2.exe`；下一步开始 `rubrica-doc` 实现。

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
