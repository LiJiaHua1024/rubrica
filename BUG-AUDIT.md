# Bug 审查报告（2026-09-28）

全仓库实现级 bug 审查。方法：8 个并行只读审查（按模块分工）→ 逐条人工核验 → 修复高置信项 → 头灯测试 + `cargo test --workspace` 全量回归。

**结果：确认并修复 24 处，新增 14 个回归测试，420 个测试全部通过，clippy 干净。**
未修复项均给出理由与建议，见"暂不修复"一节。未提交任何 commit，改动全部在工作区，请人工 review。

> 注意：`[profile.release]` 配置为 `panic = "abort"`，因此下述所有死循环/panic 类发现都是不可恢复的进程崩溃，不存在"优雅降级"。

---

## 一、已修复

### 崩溃 / 挂起（P0）

| # | 位置 | 问题 | 修复 |
|---|------|------|------|
| 1 | `rubrica-math/src/parse.rs` | 公式是读者可控输入，`{{{{…` 10 万层、`\frac\frac\frac…` 链、`x^a^b^c…` 链都会无界递归，解析、布局、析构三处栈溢出，进程直接崩溃 | 解析加 `depth` 上限（100），超限后线性 `skip_group` 跳过整组并原文显示；`argument` 同样限深且保证始终前进；脚本链用 `script_chain` 计数，超过同一上限后改为并排而非无限嵌套（保留原嵌套语义到上限为止，既有测试 `both_script_orders_converge` 仍通过） |
| 2 | `rubrica-app/src/peek.rs` `paint()` | 渲染目标创建失败（GPU 重置、远程会话）后 `target` 为 `None`，`paint` 在 `BeginPaint` 之前返回，WM_PAINT 永不验证 → 无限重绘，service 线程 CPU 100%，全局键盘/鼠标钩子同线程被系统摘除 | `BeginPaint` 提前到 target 检查之前，两条路径都配对 `EndPaint` |
| 22 | `rubrica-app/src/highlight.rs` `tag_attrs` | `is_word_start` 把一切 ≥0xC2 的前导字节当词首，但 `word_end` 对非字母（如 `（`、`“`）原地返回 → html/xml 围栏里 `<a （>` 一类内容让属性扫描**死循环**（打开文件即挂起） | 推进量至少一个字符（`i = end.max(i + char_len)`） |
| 23 | `rubrica-type/src/breaking.rs` `split_piece` | 分片扫描的 `continue` 跳过了 `i += 1`：当 piece 内 ≥ piece_limit（默认 32768）个条目里没有任何软断点（如字典连字符密布的超长 token）时 `i - start` 无符号回绕 → **无限循环**，打开文件即挂起 | cut 取在硬条目自身时同步推进 `i` |
| 24 | `rubrica-doc/src/lib.rs` `rewrite_origins` | 重写映射的字节级前进可能停在多字节字符中间（契约外的重写或字节级分歧时），下一步 `&original[from..]` 切片 panic（release 即 abort） | 全部前进改按整字符步进；契约仍由 `debug_assert` 把守 |

### 明显错误行为（P1）

| # | 位置 | 问题 | 修复 |
|---|------|------|------|
| 3 | `rubrica-type/src/breaking.rs` solve | 分片（硬断行/求解器切分之后）的首行按"段落首行"宽度打分，却按"续行"宽度放置与校验：列表项内硬断行后的行按整栏宽选断点，再被压进悬挂宽度 | 打分与 drop 检查统一用 `opts.target(first_line && at_start[e])`（两处） |
| 4 | `rubrica-type/src/breaking.rs` solve | 以硬断行结尾的段落会多出一条内容为空的"行"（只有 `\parfillskip`，页面上是段尾空行）；浏览器与 TeX 都不产出这条行 | 仅当分片是段落最后一片时，丢弃无 Box 的末行；段落中部的空行（双断行）保留 |
| 5 | `rubrica-type/src/breaking.rs` score | fitness 阶梯真值表颠倒：0→2/0→3（最差跳变）只付 lousy(100)，2→2/3→3（根本没跳变）反而付 nasty(1000)，变好从不计费 | TeX 语义：`abs(fit−prev) > 1` 才计费，按跳变规模分级（2 级 lousy，3 级 awful+nasty）；首边种子 fitness=1（TeX 的 decent），首行不再为"与不存在的邻居跳变"付费 |
| 6 | `rubrica-type/src/paragraph.rs` emit_space | 只认 `\n` 为强制断行，UAX #14 的 CR/FF/NEL 变成词间空格（`a\rb` 排成 "a b"） | `\n`、`\r`、`\f`、NEL 任一出现在空白 run 即强制断行 |
| 7 | `rubrica-type/src/paragraph.rs` emit_space | 字面空格 glue 绕过了 `Item::glue` 的 shrink ≤ base 上限：主题把 shrink 调过 base 时，两端对齐会负向压缩、字叠字 | 同样施加 `.min(base * n)` 上限 |
| 8 | `rubrica-type/src/paragraph.rs` | 字典断点无字符边界校验（非边界切片 panic），且显式连字符后的断点会再画一个 discretionary 连字符（"foo--"） | 过滤非 char boundary 的点与紧跟 `-` 的点 |
| 9 | `rubrica-math/src/parse.rs` `command_delim` | `\left\{`、`\right\}`、`\big\|` 整个丢定界符：`\{` 后的 `{` 未被消费还被当成组开括号，公式结构错乱 | 先识别单字符转义 `\{` `\}` `\|`，再走单词扫描 |
| 10 | `rubrica-math/src/parse.rs` `delim` | `\left（` 全角定界符只消费 1 字节 → 光标落在多字节字符内部 → 后续解码失败，公式剩余部分全部被静默丢弃 | 改用 `take_char` 按整字符消费 |
| 11 | `rubrica-math/src/parse.rs` named("right") | 独立的 `\right.` 产出 U+0000 原子，被测量、整形并画成豆腐 | `'\0'` 时返回空原子（`push` 丢弃） |
| 12 | `rubrica-math/src/parse.rs` environment | 环境之前写的 `\hline` 泄漏进下一个环境，多画一条作者没写的横线 | `environment()` 入口清掉残留标志（TeX 称之为 misplaced） |
| 13 | `rubrica-app/src/math.rs` `percent` | `MathLeading` 是 MATH 表的**长度**记录（设计单位/字高），却被当百分比除以 100：任何自带非零 MathLeading 的字体（如 Cambria Math）矩阵/分情况行的行距会膨胀约 20 倍 | `MATH_LEADING` 特判按 `units_per_em` 归一，其余常量保持百分比通道 |
| 14 | `rubrica-app/src/view.rs` relayout/zoom | 重文档（>512 块或单块 >8KB）走 worker 排版时，`begin_layout_job` 清掉 `pending_anchor` 与索引，`scroll_for_anchor` 对空索引求值 → **缩放窗口、跨 DPI、改字号、换字体，视口全部跳回顶部** | worker 路径把锚点交给 `pending_anchor`，Finished 批次到达时恢复（与启动路径同机制）；`relayout_from_anchor` 与 `zoom_to` 两处 |
| 15 | `rubrica-app/src/view.rs` WM_KEYDOWN | 自动重复检测读了 lParam bit 15（重复计数的最高位，实际永不置位），`plain_chord` 的防抖从未生效：按住 Ctrl+4 每秒 ~30 次开关页模式，Ctrl+3 同理反复全量重解析 | 改读 bit 30（previous-key-state） |
| 16 | `rubrica-app/src/view.rs` build_ops | `emit!` 每批 `mem::take` 四个列表，`finish` 再 take 一次拿到的是空壳 → **导出的 PDF 永远没有跨页表头重复、脚注续页标记和可复制的数学文本层**（亮/暗两条排版路径都中招） | `LayoutFinals` 删掉这四个字段，`build_ops` 从 sink 的累积缓冲读取 |
| 17 | `rubrica-app/src/view.rs` build_in_chunks | `HotKind::Wide(index)` 记的是批内局部索引，而 wide 区按批清空、全局累积：第二个窗口之后的宽图/宽公式热点指向错误区域，宽表格热点可能错点成图片预览 | emit 时统一把本批 hot 的索引加上 `wide_emitted` 基数 |
| 18 | `rubrica-app/src/peek.rs` hook_proc | 预览存续期间 **Enter/F5/Escape 全系统被吞**（切到浏览器 F5 变成刷新预览、聊天里 Enter 永久失灵），Space 在任何前台应用也被吞且字符丢失 | 只在前台仍是"选文件的文件夹"时响应预览键；并记录 `vocabulary_down` 配对吞 release，避免半个按键落进别的窗口 |
| 19 | `rubrica-workspace/src/lib.rs` close | 关闭活动标签**左侧**的标签会把选中权抢给被关标签的位置（正确语义：活动标签索引左移 1，保持选中） | `active > index` 分支自减；`active == index` 保持"右侧优先" |
| 20 | `rubrica-app/src/profiles.rs` read_names | 名单按大小写不敏感去重，保存与选中按大小写敏感比较，注册表键却按字节区分：`MyPreset`/`mypreset` 两个预设下次启动其一从名单消失、选中回落 Default | 去重改为精确匹配，与保存/选中/键名三者一致 |
| 21 | `rubrica-doc/src/lib.rs` tex 重写 | 围栏闭合行带尾随空格（CommonMark 合法）不被识别 → 重写器围栏状态卡在打开，**后续全文 TeX 定界符停止转换**；引用块 `>` 里的围栏同理不被识别，引用代码里的 LaTeX 被改写成 `$` | 闭合判定先剥尾随空白；围栏识别前剥离块引用前缀 |

另有两处小修：文件打开对话框过滤器补上 `*.log`（阅读器本就把它当一等文档，`i18n.rs`）；peek 窗口高分辨率滚轮的亚格增量用累加器保留（原先 `(delta/120)*3` 把大多数消息变成 0，`peek.rs`）。

机械扫描的其余发现（均为低置信或病态输入）：`view.rs:10092` 的 `StyleId` 用 `as u16` 截断，>65535 个 interned 样式时会错误着色（不崩溃）——记录在案；D2D 设备丢失（见"暂不修复"DPI/EndDraw 条目）。扫描确认干净的区域：GDI 句柄全部配对 DeleteObject、线程无 static mut / 无跨线程 Rc、u32 时钟唯一减法已用 wrapping_sub、注册表持久化无撕裂风险、BOM 与多字节行分割正确。

### 脚注气泡（V2，view.rs）

气泡打开时 `tick_note_bubble` 清空 hover 状态，随后指针从引用移向气泡的**第一步**就把气泡关掉——与模块自己的文档承诺相反。修复：tick 不再清 hover（`hover_step` 的 held 分支保留现场），并把引用与气泡之间的 8px 走廊计入 held。

---

## 二、已核验、暂不修复（含理由）

| 位置 | 问题 | 不修的理由 / 建议 |
|------|------|------------------|
| `view.rs:3667` DPI 体系 | 渲染目标以窗口 DPI 创建（`dpiX: self.dpi`），而显示列表按**设备像素**计算（`scale_of = dpi/72`，命中测试用原始 px）：两者只在 96 DPI 一致，>100% 缩放下渲染与命中测试互相矛盾。peek 同模式 | 修法明确（目标建在 96 DPI 并去掉 `SetDpi`），但牵连 find 输入框定位、`unit_px`、`peek.rs` 等多处彼此矛盾的换算，且**无法在不弹窗口的前提下验证**。需要在一台 >96 DPI 的机器上人工过一遍再动。已核验 96 DPI 下行为完全一致，若你在 100% 缩放使用则暂无感知 |
| `view.rs:9317` | 滚动到代码块中部时，`draw_document` 的二分带只回看 1 个 op，代码底板 Rect 排在更早的位置 → 长代码块中部背景缺失（散文+代码且无其他低 top op 时可复现） | 修法需要"带查询"支持任意回看或给 Rect 记录底边，属小重构；建议下个迭代做 |
| `view.rs:692` | worker 排版把公式 intern 进线程局部 `MathStore` 后丢弃，重文档的宽公式预览可能取到错误 store 的索引 | 正确修法是把 store 随 LayoutFinals 送回或宽区直接携带源串，涉及 Send 约束，需要设计 |
| `rubrica-doc/src/lib.rs:403` | `$…$` 无条件启用且 pulldown 的 flanking 只看 ASCII 空白：`成本$3，售价$5` 中间被吞成公式对象 | **与 GitHub 行为一致**（pandoc 有"闭 `$` 后跟数字不闭合"的保护，GitHub 没有）。保持 GitHub 对齐是更可辩护的默认；如要改，需仿 pandoc 的 flanking 后处理，代价是自维护一套判定 |
| `rubrica-doc/src/emphasis.rs:66,52` | CJK 强调修复通道在包裹内含行内对象（公式/图片）时跨区配对出错；`\*` 转义星与 CJK 相邻时被当强调吃掉 | 两者都要动 `relax` 的配对模型（转义星需要回溯源串映射），改动面大且易引入新回归，值得单独立项 |
| `peek.rs:162` | 预览存续时焦点关闭/点击关闭默认关，预览可能长期驻留 | 这是特性开关（`PeekFocusClose`），本次只修"劫持按键"的部分；驻留策略属产品决策 |
| `pagination.rs:307` | 页填充扫描 O(n³)，超长文档会卡 | 当前 `#![allow(dead_code)]`，无调用方；启用分页模式前修即可 |
| `font.rs:508` | 一串无字体覆盖的字符（PUA/生僻 emoji）使 itemization 平方级变慢 | 修法（按"任一 family 覆盖的最长前缀"推进）已明确，但需要构造覆盖数据验证，建议与字形 fallback 测试一起做 |
| `images.rs` | 图片缓存不随磁盘变化失效（文档重载前一直旧图）、EXIF 方向不应用、解码失败粘滞 | 前者加 mtime 失效属小修，可与"文件变更自动刷新"一起做；EXIF 是新特性 |
| `view.rs:3359` | `finish_startup` 完成前关窗会把会话状态覆盖为空 | 需要区分"启动未完成的关窗"与"正常关窗"，加 `started` 守卫即可，但触发窗口极窄，先记录 |
| `view.rs:3144` relocated_source | 文件变更后按 80 字符尾串重定位，尾串本身被编辑时回退到偏移量近似 | 算法已防 UTF-8 与重复上下文的panic/错配，精度问题接受 |
| `view.rs:10895` PDF | 导出按页克隆整个显示列表（O(页×op)），行/图跨页裁剪不完全 | 导出正确性无碍，纯性能/边缘几何，重写 PDF 生成时一并处理 |
| `settings.rs:592` | 阅读位置两条注册表写非原子（崩溃窗口内新路径配旧锚点） | 注册表无事务；概率极低且后果是滚动位置异常 |
| `i18n.rs` 其余 / `theme.rs` / `instance.rs` / `reading.rs` | 审查未发现问题 | — |

---

## 三、分层验证机制（本次实际执行）

- **L0 静态走查**：8 个并行只读审查全部完成，覆盖 type / math / doc+workspace / app 基础设施 / app 功能模块 / view 输入侧 / view 排版与 PDF 侧 / 全局机械类扫描（unwrap、UTF-8 边界、无符号下溢、无限循环、递归深度、COM 泄漏、u32 时钟回绕、非原子写）。
- **L1 人工核验**：所有 P0/P1 逐条读代码确认触发路径后才动手；未核验的 L0 发现全部降级到"待确认"。
- **L2 轻量验证**：`cargo check` + `cargo clippy`（干净）+ `cargo test --workspace`（420 全过，含 14 个新增回归测试）；编译/测试以 `-j 2` 限速运行。
- **L3 重型验证**（release 构建、GUI 端到端、视觉验收）：本次有意不执行。

## 四、新增回归测试

- `rubrica-type/tests/typesetting.rs`：硬断行不产生尾随空行；悬挂块中硬断行后续行受悬挂宽度约束（含 `natural ≤ target` 断言）；CR 强制断行；显式连字符后不重复画连字符；字面空格 shrink 上限；无软断点的超长 piece 能分裂并终止。
- `rubrica-math/src/parse.rs`：`\left\{`/`\big\|` 简写定界符；全角定界符不截断公式；`\right.` 不产出 NUL；10 万层嵌套不栈溢出；`\hline` 不泄漏进环境。
- `rubrica-doc/tests/model.rs`：围栏闭合行尾随空白仍闭合；引用块内围栏护住 LaTeX。
- `rubrica-workspace/src/lib.rs`：关闭活动标签左侧标签保持选中。
- `rubrica-app/src/highlight.rs`：html 标签属性里的全角符号不再死循环。
