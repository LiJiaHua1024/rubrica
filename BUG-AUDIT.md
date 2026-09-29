# Bug 审查报告（2026-09-28，第二轮 + 第三轮 DPI 专项 + 第四轮回归复查）

全仓库实现级 bug 审查。方法：8 个并行只读审查（按文件分工，互不重叠）→ 交叉印证 → 6 个并行修复（按文件所有权分工，禁止跨文件、禁止并发跑 cargo）→ 集中跑 `cargo test --workspace` + `clippy -D warnings` + `--features pdf` → 对 P0/P1 逐条回滚验证测试确实能失败 → 第三轮专攻上一轮最大未修项（DPI 坐标体系），用**可自动化的不变量测试**与**真实 PNG 输出测量**替代真机验证 → 第四轮**反向的**复核：把 ~4500 行改动逐条 diff 审问"这个修复弄坏了什么"，发现并修复 4 处回归。

**结果：确认并修复 71 处（其中 8 处 P0），新增 67 个回归测试，486 个测试全部通过，clippy 干净，PDF feature 编译通过。**
未修复项均给出理由，见"暂不修复"一节。未提交任何 commit，改动全部在工作区，请人工 review。

---

## 零、第四轮：修复本身引入的 4 处回归

重要性高于任何单条新 bug 的发现——它们是"修 bug 修出来的 bug"，且多数不会崩、只会**静默显示错误内容**。

| # | 位置 | 回归 | 修复 |
|---|---|---|---|
| R1 | `math/src/layout.rs` `source_of` | `too_deep` 把过深子树压成一段字面文本，而 `source_of` 用显式栈逆序推入。注释说"最后推的先读"，六个 arm 却把 brace 与前缀推反了：`x^{2}` 渲染成 `{x2}^{`、`\sqrt[3]{x}` 成 `3]x}`（且 `\sqrt` 前缀**根本没被推入**）、`\left(x\right)` 成 `\left(\rightx)`、`\sum_{i}^{n}` 成 `\sum}i_{}n^{`、`SubSup` 的 `}` 还重复一次。既有测试只断言 `len() == 1 && len > 100`，所以全绿而用户在页面上看到乱序文本 | 每个 arm 改为严格按"期望输出"的逆序推入；测试改为断言折叠后的**完整源文本** |
| R2 | `math/src/parse.rs` `skip_environment` | `open` 只增不减：一旦有嵌套 `\begin` 把它提到 2，任何 `\end` 都满足不了 `open == 1`，扫描跑到输入末尾，公式之后的内容全部被吞进降级字面原子里。130 重嵌套 + 两个内层网格时，表格后面的文档消失 | `open` 计数换成 `Vec<String>` 的具名栈，`\end` 闭合它命名的那层 |
| R3 | `math/src/table.rs` | **斜体修正读了错误字段。** `mathItalicsCorrectionInfoOffset` 在 `MathGlyphInfo` 的 **+0**，上一轮把它从 +0 改成了 +4——而 +4 是 `extendedShapeCoverageOffset`。真实字体上这个值落进 `MathKernInfo`，看起来像个合法 coverage，于是每个字形都读到 0，**每一条斜体修正都静默归零**，每个上标/重音/根号/极限都偏位 | 读回 +0。用 Python 直接解 `C:\Windows\Fonts\cambria.ttc` 的 MATH 表独立复核：`+0=8`→修正表在 526、312 个字形；`+4=3574`→落入 4092 的垃圾。上一轮修好了 coverage 基址却错移了字段，净效果仍是全零——**症状由修复本身造成** |
| R4 | `app/src/view.rs` `visible_band` | 上一轮把可见带的下边界从"回看一个 op"改成"有界回扫（`BAND_REACH = 2048`）"，但 op 的底边**不是单调的**，任何常数都对超过它的 op 失效：**整页截图**（`ImageStore::fit` 只限宽不限高，高图轻易超过 2048px）滚进去后整块变空白；~84 行以上的表格竖线和中型引用块同样消失 | 删掉常数，改为在布局时算一次 `op_bottom` 的**前缀最大值**（`op_reach`/`extend_reach`），下边界取 `reach.partition_point(\|r\| *r < top)`。O(n) 每布局一次 + 每帧一次二分，代价与原来的 `ops_sorted` 扫描同级。另顺带修了 `op_bottom` 对 `Op::Runs` 只读**最后一个** run 的问题（上标记号未必是行内最后一个 run） |

**教训**：`too_deep` 的旧测试 asserts 长度而非内容，`source_of` 才把乱序内容漏了过去；`BAND_REACH` 的测试只造了 2000 行的散文页（band 恒为 108 op），没有造**比常量高的单个 op**。两者都是"测试写得太像被修复的那一行"。

---


> 注意：`[profile.release]` 配置为 `panic = "abort"`，因此下述所有死循环/panic 类发现都是不可恢复的进程崩溃，不存在"优雅降级"。

---

## 一、第二轮已修复

### 崩溃 / 内存安全（P0）

| # | 位置 | 问题 | 修复 |
|---|------|------|------|
| 25 | `math/src/parse.rs` `\left` | **`\left` 递归完全没有深度计数**——`MAX_DEPTH` 只在 `{` 组和 `argument()` 上生效，`command()` 直接 `self.list(Ctx::Group)`。上一轮的"已加深度上限"只覆盖了两条路径。5 千个 `\left` 即可耗尽 2 MiB 栈 | 按 `{` 组的同一套守卫包住函数体，超限时线性扫到 `\right` 后整段作字面文本（新的 `skip_left_body`） |
| 26 | `math/src/parse.rs` `environment()` | `\begin{env}` 递归同样无上限（`environment`→`env_rows`→`list`→`command`→`environment`），未知环境名走同一条路。2000 个 `\begin{matrix}` 约 26 KB | 入口处加同样的守卫，超限时线性扫到配对的 `\end` 并返回既有的 `degraded` 节点 |
| 27 | `math/src/layout.rs` `lay()` | `lay` 对 Node 树裸递归，全文件没有 depth 参数。即使 25/26 修好，这仍是独立的第二条溢出路径 | 加 `MAX_DEPTH` 字段与计数包装，超限时用**迭代式** `source_of` 把子树压成一段字面文本（用显式工作栈，避免打印源码时重新引入它要报的递归） |
| 28 | `app/src/view.rs` `run()` | **启动失败的堆 use-after-free**：`finish_startup(saved)?` 让错误穿过 `run()`，在 `Rubrica.Main` 窗口仍然存活、仍注册着 `wnd_proc`、`GWLP_USERDATA` 仍指向已释放的 `Box<View>` 时释放它。随后 `MessageBoxW` 的模态循环会向这个死窗口派发 `WM_PAINT`/`WM_TIMER`；即便用户秒关，`ExitProcess` 也会把 `WM_DESTROY` 送给同一个野指针 | 改成 `match`：出错时先杀三个定时器、清 `GWLP_USERDATA`、`DestroyWindow`，再返回错误。独立地补 `WM_NCDESTROY` 清指针 + `WM_DESTROY` 杀全部五个定时器，堵住将来任何提前返回 |
| 29 | `app/src/settings.rs` `Batch::text` | **堆破坏**：目标缓冲区按 `vec![0u16; len / 2]` 分配，奇数长度短一个字节，第二次 `RegQueryValueExW` 却仍被告知 `len` 字节。注册表里一个 7 字节的 `REG_BINARY` 就会往 6 字节的分配里写 7 字节。模块自己的文档就说注册表可被手改 | 分配 `len / 2 + 1`，第二次查询告知真实容量；第一次查询同时读类型，非 `REG_SZ` 直接拒绝。`word()`/`binary()` 本来就是对的，只有 `text()` 错 |
| 30 | `app/src/main.rs` | `std::env::args()` 遇到任一非 UTF-8 参数即 panic。NTFS 允许未配对代理项的文件名（`args_os()` 不会） | 改用 `args_os()`，标志比较走 `OsStr`/`to_string_lossy()`，路径按 `PathBuf` 传递 |
| 31 | `app/src/font.rs` `shape_runs` | **UI 冻结**：每轮迭代都对**剩余全部文本**重探每个候选字体族；当没有字体覆盖下一个字符时 `taken` 塌缩成 1 个字符，于是整段重探。不被任何已装字体覆盖的 5000 字无空白串（PUA、埃及象形文字、Linear B）≈ 2500 万次字形查询、2.5 万次分配，且每次测量、每次重绘都重来 | 覆盖探测合成一次（`coverage`），`resolve_face` 直接返回已探明的覆盖前缀；**无任何字体覆盖时整段剩余一次成形**为 `.notdef`，O(k·n²)→O(k·n) |
| 32 | `app/src/instance.rs` `take_forward` | `from_raw_parts` 的长度取自**发送方**的 `cbData`，只校验了 `dwData` 标签。任意本地进程可发 WM_COPYDATA 让它按攻击者选定的 u32 长度分配 | 超过 64 KB 直接拒绝（对齐 `font.rs` 已有的 256 MB 上限做法） |

### 明显错误行为 / 数据丢失（P1）

| # | 位置 | 问题 | 修复 |
|---|------|------|------|
| 33 | `app/src/view.rs` `reader_page_starts` | **导出内容静默丢失**：分页锚点搜索用 `>=`，而只要上一个分页落在标题上，`previous` **永远**是一个锚点 → `start == previous` → 不断开页、`limit` 不推进。已复现：`[0, 780, 1700]` 下，一张 400 pt 高的图有 120 pt 落在两页之间，**任何一页都不画它**。实际触发是"标题 + 行内图片" | 要求候选严格越过当前上限：`heading.filter(\|top\| *top > previous + 0.01).unwrap_or(y)`。测试断言"每条线都被某页完整覆盖"这个性质，而不是断言具体页数组 |
| 34 | `app/src/view.rs` `TableSpan.header` | 分块排版里 `table_headers` 每个 chunk 被 `mem::take` 清空，而 `header` 记的是**块内局部**下标；`PageSink::accept` 却是拼接。第一块之后的每张表，续页上重复的是**第一张表**的表头（面板、线、表头文字，画在第二张表的列宽上）。上一轮修 `HotKind::Wide` 时漏了它 | 与 `wide_emitted` 完全对称地加 `headers_emitted` 重基。已把 `emit!` 排空的**每一个**缓冲区与"块内记、拼接后查"的下标全部复核一遍，`TableSpan` 是唯一实例 |
| 35 | `app/src/view.rs` `caret_at` | **表格命中测试全落在第一列**：单元格的 `SelLine` 按列推入，一整行共用同一个 y，二分查找只能取到行首那个。点第 3 列 → 光标和拖拽起点都在第 1 列；双击选中第 1 列的词 | 二分之后继续走完这一行（同行同 y，或带 `Join::Tab` 的续行），取 x 距离最小者；非表格路径的步数与结果与原来逐字节一致。`sel` 的单元格序是**故意**的（复制路径依赖），未改 |
| 36 | `app/src/view.rs` `apply_find` / `top_line` | 搜索焦点下标未夹紧：所有命中都在窗口上方时 `focus == len`，工具栏显示 **"7 of 6"**，且 `Enter` 跳到第二个命中而不是回到第一个。另：`top_line()` 在滚过末行后返回 0，于是在文末往搜索框敲一个字符，页面会**从文末跳回第一个命中** | 焦点下标夹紧；`top_line()` 改回 `Option`，越界时取最后一个命中且不滚动 |
| 37 | `app/src/view.rs` `relayout` | 重文档走 worker 时切换到**同步路径**的文档，旧的排版任务从不取消：旧文档的批次 `epoch` 校验**通过**，被追加到新文档的显示列表与选择索引上，页面渲染成两个文档的混合。同标签页换文件必中（换标签页是安全的） | 在 `relayout` 的 heavy 判定**之前**取走并取消旧任务 |
| 38 | `app/src/view.rs` `spawn_layout` | worker 排版时 `Objects::new(..., None, ...)`，`base_dir` 丢失 → 相对图片路径按**进程工作目录**解析。同一份文档在小（同步路径）时正常，超过 512 块或单块 8 KB（转 worker）后图片全部消失 | `LayoutRequest` 增加 `base: Option<PathBuf>`（仍为 `Send`） |
| 39 | `math/src/parse.rs` `row_rules` | 规则缓冲是共享的，`env_rows` 入口无条件 `clear()`：单元格里的嵌套环境会抹掉外层网格**已经收集**的边界，外层的 `\hline` 静默不画；`degraded` 路径不 `take()`，内层网格的规则反而被外层领走。已复现 `\begin{array}\hline 1 \\ \substack{a\\b} \\ 2\end{array}` 不画顶线 | 改成栈：`environment()` 与 `\substack` 各自 `mem::take` 进出；`\substack` 同时清 `self.hline`（上一轮 `\hline` 修复留下的最后一个洞） |
| 40 | `math/src/parse.rs` `command()` | `\` 后跟非 ASCII 字节时只 `bump()` 一个字节，光标落在多字节字符中间 → 下次解码失败 → `list` **跳出循环，公式剩余部分全部丢弃**。`$\frac{1}{2}\α + 1$` 渲染成 `1/2` 加乱码，` + 1$` 消失 | 非 ASCII 用 `take_char()` 整字符消费并作字面原子；解码失败改为跳过 1 字节继续，而不是放弃剩余公式 |
| 41 | `math/src/table.rs` `italics_correction` | **斜体修正取自错误偏移**：`rel(&r, info, 0)` 把第三参数当子表内偏移，实际是**文件**偏移，于是从 MATH 版本号处读 Offset16；`r.u16(self.glyph_info)` 又把主版本号（1）当成修正表距离。斜体修正决定每一个上标、重音、横线、伸展根号的位置 | 两处都改为从 `info` 解析（`italics` 字段在 `MathGlyphInfo + 4`），并补上"修正表偏移为 0"的零守卫。回滚验证：修前 `left: 0, right: 120`，修后通过 |
| 42 | `type/src/breaking.rs` solve | **求解器整段死亡**：所有活动边都超宽时 `next_active` 为空、`best` 为 `None`，活动链表就此清空且永不重建，终端不可达 → 三趟全返回 `None` → 整段落进 `desperate` 贪心，每行 `ragged: true`、`badness: 10000`、`demerits = INFINITY`。已复现 `"the extraordinarily word"` @ 96pt | 补 TeX 的 `create_new_active_node`：候选为空且链表为空时，以巨额代价重建最不坏的那条边。不变式成立（起点严格递增、终结点始终可达、文本不丢） |
| 43 | `type/src/paragraph.rs` `build` | **O(n²) 卡死**：`hyphen_set.contains` 是对 `Vec<usize>` 的线性扫描，而代码围栏在**每个字符边界**都给一个断点。10 万字节的源文件 ≈ 130 亿次比较；1 MB 要几小时 | 断点本就有序，`sort_unstable()+dedup()` 后 `binary_search` |
| 44 | `app/src/images.rs` | 图片缓存只按路径做键，**从不失效**：磁盘上的图换了（构建脚本重新生成截图）按 Ctrl+R 仍画旧位图，而 worker 上重新测量过所以**盒子已经是新尺寸**——于是出现"新尺寸 + 旧画面"；解码失败还会被永久缓存（文件写了一半时引用它，之后整个会话都是空白） | 键改为 `(len, mtime)`，无戳的文件不被记住；失败的解码不缓存 |
| 45 | `app/src/images.rs` | 位图缓存与渲染目标的 DPI 无关：把窗口拖到 150% 显示器上，文字变了尺寸而所有图片不变，直到关窗 | 键加上目标 DPI（取自 `ID2D1RenderTarget::GetDpi`），`WM_DPICHANGED` 原地 `SetDpi` 后自然落空 |
| 46 | `doc/src/lib.rs` `Tag::Item` | `Tag::Item` 会**抢占式**开一个 Paragraph 占位块（紧凑列表项不产 Paragraph 事件），但 `open()` 在已有块时是空操作。列表项第一个块不是段落时，占位符胜出：`- ```rust` 得到一个带 `lang` 的**段落**而非代码块；`- # 标题` 的 H1 从大纲里消失 | 不再抢占；把 `ListInfo` 暂存给 `open()`，由真正打开的块类型消费 |
| 47 | `doc/src/lib.rs` `\[` 识别 | 显示公式定界符判定跑在**剥离引用标记之前**、且不容忍尾随空白。`> \[ … > \]` 不产生公式对象；`- \[ … \]` 出现"开标记没转、闭标记转了"的**不对称**，留下一句游离的 `$$` | 改用 `quoted`，容忍尾随空白，并记录 `display_open` 使 `\]` 只在其 `\[` 已被改写时才改写 |
| 48 | `doc/src/lib.rs` `Tag::Image` | 只把 `Event::Text` 并入 alt，其余内联事件落进正文；且 `Tag::Image` **覆盖**而非嵌套已打开的图片。`![a $y$ b](i.png)` 产生两个对象；`![a ![b](j.png) c](i.png)` 让**外层图片整个消失** | 所有内联事件按 `self.image.is_some()` 分流；`image` 改为栈 |
| 49 | `doc/src/lib.rs` `u8` 深度计数 | `in_item`/`quote_depth` 用 `+=` 无饱和，256 层嵌套就是 512 字节输入 → debug/溢出检查构建下 panic（release 关闭溢出检查后静默回绕，300 层给出 `depth: Some(43)`） | `saturating_add(1)` + `u8::try_from(...).unwrap_or(u8::MAX)`，公开类型不变 |
| 50 | `doc/src/emphasis.rs` | `relax` 用**栈位置**而不是栈里装的**那个 run** 去索引 `runs`（`runs[open]`，`open` 来自对 `stack` 的 `rposition`）。任何两个配对之间跳过过一个 run，两个配对就共用开标记，`cut` 收到同一段字节范围两次 → `begin > end` panic → 打开文件即 abort | 改为 `runs[stack[at]]` |
| 51 | `type/src/paragraph.rs` 剥离 | `trim_start`/`trim_end` 用 `char::is_whitespace`，把 U+3000 全角空格、U+00A0 一并剥掉；再加上 `trim()` 丢弃**前导** glue，段首缩进被整段删除。`"　　这是缩进段落。"` 渲染成**顶格**；行内 `"中　文"` 的 1 em 全角空格变成 0.33 em 可断空格 | 只剥 ASCII 空白（外加三种行分隔符）。U+3000 留在正文、被计为 1 em、拿到 `cjk_join`；U+2000-200A 顺带恢复了各自的真实宽度 |
| 52 | `app/src/view.rs` `draw_document` | 可见区二分只**回看一个** op，而代码块背景 `Rect` 是该块的**第一个** op：滚动到围栏中部时，从视口顶到围栏底**整片没有底色** | 抽出 `visible_band()`，用 y 方向的回扫 + 有界前瞻（`BAND_REACH`）取代单步回看；`op_bottom` 同时用于 `Rect`/`Image` 的剔除，使band 与逐 op 剔除可证明一致 |
| 53 | `app/src/peek.rs` `hook_proc` | 吞掉按下键后，**自动重复**不再被吞（`service.shown` 已为 false，整块被跳过），但最终那次抬起仍被吞 → 前台程序收到约 30 次 key-down 却没有 key-up，Explorer 的重命名框会反复弹开并认为 Enter 还按着 | 配对检查上移到 `service.shown` 守卫**之前**，两条边都测 |
| 54 | `app/src/peek.rs` `preview_answer` | 没有 Alt 掩码，`WM_SYSKEYDOWN` 当作按下处理 → **Alt+Enter 的属性对话框被吞**，Ctrl+Shift+Escape / Ctrl+Escape 同样 | 加 `alt` 参数并要求其为假 |
| 55 | `app/src/peek.rs` `shape_bar` | 标题栏把页单位（点）和设备像素混用：`at` 按像素累加，却拿去和按点算的 `right` 比较。150% 屏幕上提示文字离右边缘差几百像素，且文件一长整个提示被吞 | 两侧统一为设备像素 |
| 56 | `app/src/peek.rs` `paint` | `EndDraw` 结果被丢弃，`D2DERR_RECREATE_TARGET`（显卡重置、扩展坞、Win+Ctrl+Shift+B）后目标永久失效，窗口冻结在最后一帧 | 检查结果，重建目标并重绘 |
| 57 | `app/src/view.rs` `paint` | 同类问题：`BeginDraw`/`EndDraw` 结果都丢弃，设备丢失后 WM_PAINT 仍 `ValidateRect`，窗口**永久空白**直到重启进程 | 新增 `draw_failed` 统一处理（仅在真正重建成功时才 `InvalidateRect`，避免坏设备把绘制循环变成空转） |

### 性能与其他（P2，摘要）

- `doc/src/lib.rs` `Document::source` 原本对每个边界窗口重新折叠全部标记（O(边界×标记)）：16 000 行要 241 ms，1 MB 外推约 1.5 s。改为差分数组单趟扫描后约 0.4 s。
- `app/src/view.rs` `write_pdf` 此前**每页深拷贝整篇文档的显示列表**再做裁剪：1 万行文档 ≈ 330 万次 `Op::clone()`、约 2000 万次堆分配、约 2 GB 拷贝，且随文档长度平方增长。裁剪提到克隆之前。
- `app/src/view.rs` 超过一页高的图片只画一次就被裁掉；改为把余下部分作为**新 op** 续到下一页（`XObjectTransform` 表达不了源裁剪，重画原 op 会重复 band）。
- `app/src/view.rs` 一个字体无法内嵌时 `return Err`，**整个导出作废**（数学路径同样的失败只是跳过）。改为警告并继续。
- `app/src/view.rs` `op_top` 对空 `Op::Runs` 返回 0.0，一旦出现就让 `ops_sorted` 永久为假，之后每帧遍历全文的 op；引用线与表格竖线的 `y0` 排在自身内容之前，同类。已改为不推空 runs、规则移到本块内容之前，页内顺序保持单调。
- 其余：`thumb_rect`/`clamp_scroll`/`progress_percent` 三者对"最大滚动量"各说各话（已统一为 `max_scroll_of`）；Shift+滚轮只设不清 `wide_active`（滚轮在别处"滚不动"）；`relayout` 换掉 `wide_regions` 却不重置 `wide_active`；`History::step` 在 `reopen` 成功前就提交（Alt+← 失败后历史被吃掉）；`Command::StatusItem` 之后滚动量未夹紧；`auto_scroll` 用 `client_h` 而非内容区底边（拖进状态栏会滚页）；`WM_DPICHANGED` 触发两次排版；`WM_KILLFOCUS` 不停光标闪烁（焦点在搜索框时每 500 ms 整窗重绘）；`WM_DESTROY` 泄漏 `edit_font`/`edit_brush`；窄于约 226 DIP 时标签条**一个标签都不画**；直引号 `'` 不算词内字符（双击 `don't` 只选中 `don`）；跨 bidi 行边界不是左右互逆；`peek.rs` 的 `vocabulary_down` 单槽被覆盖、失败路径无限重试、点击源文件本身也关闭、`Drop` 缺失导致 HWND 泄漏、每次点击都在钩子里开关注册表、每个按键都 `format!` 调 `OutputDebugStringW`；`reading.rs` 按**过期**索引分配（2 GB 小说被截断仍先分配 2 GB）；`tables.rs` 按字体表目录里的长度分配；`report.rs` 的 `--dpi 0`/`--width 0` 产生 inf/NaN；`instance.rs` 的载荷上限；`font/analysis.rs` 未夹紧 COM 传入的 `position`；`pagination.rs` 的三次方扫描；`\sqrt[` 无 `]` 会吞掉整个公式作度数；`\begin {matrix}` 名字带空格整格退化为文字；`\big\foo` 吞掉分隔符文字；`\displaystyle` 跨组丢失；`widest_fragment` 遇任何 penalty 就截断；超宽 RTL 行左对齐而非向左溢出；`plain.rs` 的 CJK 判定与 `classify` 不一致（韩文换行处会插入空格）；标签数无上限。

---

## 二、第二轮：暂不修复（含理由）

| 位置 | 问题 | 不修的理由 |
|------|------|-----------|
| ~~`view.rs` `attach`（`dpiX`/`dpiY = self.dpi`）~~ | ~~**本轮最大的一条。**~~ **已在第三轮修复**，见下方"第三轮：DPI 坐标体系" | 上一轮列为"最大未修项"，理由是无法在 >96 DPI 机器上验证。第三轮找到**可自动化的验证方式**（见下），因此不再依赖人工 |
| `peek.rs` 钩子被系统摘除 | 低级钩子的宿主线程同时跑整个选中项轮询与排版；轮询超过 `LowLevelHooksTimeout`（300 ms）后 Windows 会静默摘除该线程的 `WH_KEYBOARD_LL`/`WH_MOUSE_LL`，且**没有任何地方重装**（原注释声称定时器会"治愈"它，实际只清了 `space_down`） | 已修掉最坏后果的一半：加了对超时的精确检测与 **1 秒内重装**，"整个会话失聪"变成"短暂失聪"。彻底修法（把读+解析+排版挪到工作线程、回投页面）需要新增线程、消息与挂起页槽位，在无法编译验证的情况下盲改风险过高 |
| `view.rs:9068` | worker 的 `MathStore` 下标被拿去查**窗口**的 store（`spawn_layout` 里是线程私有的） | 上一轮已记录。正确修法是让 `LayoutFinals` 把 store 带回或让宽区直接携带源串，受 `Send` 约束，需设计 |
| `font.rs` `OpenParagraph` | 以裸 `(*const u8, usize)` 标识段落且从不清理，上一段的地址+长度被复用时会把**上一段的 script/bidi run** 发给新文本 | 自包含的改法都会让 `itemization()` 退回 O(n²)（正是该缓存要消除的）；真正的修法需要新增 `FontEngine::end_paragraph` 并在 `view.rs` 的 `typeset_hyphenated` 返回后调用 |
| ~~`plain.rs` `read_window` / `read_source_window`~~ | ~~"按过期索引分配"~~ | **第四轮已修**：新增 `readable(range, len)`，按 `file.metadata()` 校订长度，失败时报与原来 `read_exact` 相同的 `UnexpectedEof` 而不再预留整段 |
| `emphasis.rs` 转义星 | 修复通道分不清 `\*` 与字面 `*`（pulldown 都交回 `Text("*")`），作者明确转义的星号仍会变成强调 | 需要在 `Block` 上新增一个"来自转义"的样式位并按代码跨过滤。改动面大、回归风险高于收益 |
| `rubrica-doc/src/lib.rs:706` | `raw.find(text)` 对 pulldown **合成**出来的文本（实体、转义）失配，回退路径把整段显示字节都映射到 `event_range.start`，导出的文本层里一个字符的源位置指向实体的字母 | 需为实体/转义构造逐字符映射 |
| `settings.rs:592` 两条注册表写 | 阅读位置非原子（崩溃窗口内新路径配旧锚点） | 注册表无事务，概率极低且后果只是滚动位置异常 |
| `peek.rs` `Drop for Peek` | 预览 HWND 从不销毁（`close()` 只隐藏），关停时 `Box<Peek>` 先于窗口释放 | 已确认可以安全补 `Drop`（关闭顺序无需调整，进程退出期间不泵消息），但需要 `IsWindow` 守卫；属清理性质，未与功能修复混在一起 |
| `pagination.rs` 的 `paginate` | 原本 O(n³) | 已在 P2 里修掉了（提前 `break` + 预计算分组边界），四项既有测试行为逐位相同 |
| `math` 的 `MathGlyphInfo` 其余子表 | `mathTopAccentAttachment` / `mathExtendedShapeCoverage` / `mathKernInfo` 完全未实现 | 新特性，不是 bug |

---

## 三、第二轮：分层验证机制（实际执行）

- **L0 静态审查**：8 个并行只读审查，按文件严格分工互不重叠（view.rs 按行段切成 4 份、peek.rs 单列、四个库 crate、一次全局机械扫描：unwrap/UTF-8 边界/无符号下溢/无法前进的循环/递归深度/线程与 COM 生命周期）。
- **L1 交叉印证**：机械扫描与各 crate 审查**独立命中同一批问题**（`\left` 递归、无符号溢出、死循环类），互相提高置信度。
- **L2 集中验证**：修复 agent 一律**禁止**运行 cargo（view.rs 被 4 个 agent 并发编辑，中间态本就编译不过，且共享 target 锁会互相排队），由我一次性跑 `cargo test --workspace`（477 全过）、`cargo clippy --workspace --all-targets -D warnings`（干净）、`cargo check -p rubrica-app --features pdf`（通过）。clippy 抓到并修掉 5 处 agent 引入的告警（2 处 `single_match`、2 处 `needless_range_loop`、`cmp_owned`）。
- **L3 回滚验证**：对两条 P0/P1 逐条把修复改回原样，确认新测试**确实失败**，再恢复——否则"测试通过"不能证明测试有效。
- **L4 重型验证**（release 构建、GUI 端到端、视觉验收、>96 DPI 真机）：**有意不执行**，与上一轮一致。其中 DPI 项未修正是因为它只能靠 L4 验证。

---

## 四、第二轮新增回归测试（共 58 个）

- `rubrica-type/tests/`：求解器不因超宽 token 落入 `desperate`（含 2 em 悬挂缩进）、U+3000 是墨不是词间空格（含 U+00A0/U+2000-200A 回归）、ragged 行按留白打分、强制断行覆盖 LF/CR/VT/FF/NEL/ZL/ZP、discretionary 不断开片段、超宽 RTL 行向左溢出。
- `rubrica-math/src/`：`\left` 与环境嵌套超限后整段读取、300 层树排版降级为自身源码、嵌套网格各守各的规则、`\` + 多字节字符保住公式剩余、`\sqrt[` 无闭括号不是度数、环境名两侧空格、未知分隔符按字面显示、样式切换持续到所在组末尾、无 `MathGlyphInfo` 的字体不作修正、斜体修正读自列举它的那个 glyph info。
- `rubrica-doc/`：列表项首个块是什么就是什么、空标记不留下、任务标记与正文同块、`\[` 穿过引用标记与尾随空白、图片 alt 收下每一个内联、嵌套图片折进外层 alt、深度超限饱和而非回绕、差分扫描与被替换的折叠逐字节一致、代码块源区间不越过正文、Hangul/扩展区/假名补充的 CJK 换行不加空格、强调不跨代码跨度和链接、两次配对共用开标记不再 panic。
- `rubrica-workspace`：标签数停在上限且保住正在读的那个、先舍未固定的、快照里标签多于上限时被夹紧。
- `rubrica-app`（`view.rs`）：表格点击落在指针所指单元格、行内多空行后仍保持阅读顺序、直引号属于词、跨 bidi 行边界与普通行一致、窄窗仍显示正在读的标签、worker 排版拿到图片所在目录、拇指到底即到底、位置条不再越界、跨页不吞掉锚点下的行、块后的表指回自己的表头、高 op 按底边被找到（并断言band 宽度与文档长度无关）、搜索从读者附近开始而非越过列表末尾、拖到页边才滚页、跳不过去的导航不改变历史。
- `rubrica-app`（其余）：图换了不再画旧的、过大图片按像素拒绝、不是字符串的值不会被当成字符串（并做堆健全性检查）、非 Unicode 文件名仍是文档、标志从原始命令行读、载荷超过文件列表上限不取、过期区间不按旧长度分配、比文件还长的字体表不分配、没有可除的页面尺寸被拒绝、窗口外的时间戳也能预读、文档之后的段落照常排版。


---

---

# 第三轮（2026-09-28）：DPI 坐标体系

上一轮把 `view.rs` 的 DPI 问题列为"最大未修项"，理由是**无法验证**。这一轮找到了能自动化的验证路径，因此把它连同两个同源 bug 一起修掉了。

## 根因

D2D 渲染目标上 `dpiX: D` 的含义是"一个 D2D 单位 = D/96 个设备像素"。而整个应用的显示列表、命中测试、界面常量**全部是设备像素**：显示列表由 `GetClientRect` 的 `client_w/client_h` 构建，绘制路径只有 `SetTransform(translation)`、**没有任何缩放变换**，`DrawGlyphRun` 的 `fontEmSize` 又是 DIP。目标 DPI 因此给每一个绘制操作乘了一个 `dpi/96`，而别处都不认这个因子。

96 DPI 时该因子恰为 1.0，一切正确——这正是仓库里所有测试都写死 `const DPI: f32 = 96.0` 却全绿、bug 却真实存在的原因。125%/150% 时页面被放大并**裁掉右侧与底部**（永久滚不到），标题栏按钮/状态栏/滚动条画到窗口外而命中测试仍按原区域判定，点击选中的行比指针高约 `dpi/96` 倍。

## 修复

| 位置 | 改动 |
|------|------|
| `view.rs` | 新增 `const TARGET_DPI: f32 = 96.0` 与 `fn window_target_properties()`；`attach()` 用后者建目标（`dpiX/dpiY = TARGET_DPI`）；删除 `WM_DPICHANGED` 与 `WM_SIZE` 两处 `SetDpi`（目标恒为 96，无需再设） |
| `view.rs` | `unit_px` 原为 `96.0 / dpi`（**倒**了），改为 `target_dpi / 96.0` 并传入 `TARGET_DPI`；`position_edit` 用它摆放 find 输入框。`MoveWindow` 在 Per-Monitor-VPI 父窗口下取**物理像素**，因子 1.0 才对 |
| `view.rs` | 抽出 `caption_button_rect(index, client_w)` 与 `CAPTION_ORDER`，让绘制与命中测试共用同一份算式，杜绝二者漂移（索引算术与 `floor()` 分区逐字节未变） |
| `peek.rs` | **同源 bug**：预览目标同样按窗口 DPI 创建 → `PEEK_TARGET_DPI = 96.0`。该文件无 `SetDpi`、无子窗口，两处 `SetWindowPos` 一处不带坐标、一处用 `GetMonitorInfoW` 的工作区（已是物理像素），均无需换算 |
| `view.rs` `write_png` | **另一个独立 bug**：PNG 导出目标建在 72 DPI，而 `export_png` 传给 `build_ops` 的是 `72 * scale`，于是 `Page.ops` 已经是输出像素，目标却又乘了 0.75 → 内容缩在左上角 75%，右侧与底部 25% 空白，且脚注被切掉。改为 `TARGET_DPI` |

`self.dpi`（`GetDpiForWindow`）**保持不变**——它仍需驱动 `scale_of = dpi/72`，那才是排版随显示器缩放的来源；`DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` 也必须保留。被钉死的只是**渲染目标的单位**，不是进程对 DPI 的感知。

## 验证

关键在于：上一轮认为"无法验证"是错的。**不变量可以写成测试**——绘制与命中测试必须在每个 DPI 上一致：

```
document_y((Y - up) * u, scroll, dpi) == Y      对任意 P、scroll、dpi 成立
```

（Y = `P * scale_of(dpi)`，up = `scroll_dip(scroll, dpi)`，u = 目标单位），代数上等价于 `u == 1`。新增 4 个测试在 dpi ∈ {96, 120, 144, 192} × scroll ∈ {0, 20, 400} × P ∈ {0, 10, 96.5, 600, 2500} 上验证该恒等式，并断言 find 输入框与所绘面板逐像素重合、窗口控件与状态栏/滚动条在任何 DPI 下都完整落在 `[0,client_w]×[0,client_h]` 内且**命中区域等于绘制区域**。把 `TARGET_DPI` 临时改成 144 后 4 个测试全部失败，确认它们真的会咬。

`--export-png` 是**无头 CLI 路径**，因此 `write_png` 的修复可以拿真实输出测量，而不是纸上推导（左边距应为 `base * MARGIN_EM = 13.5 * 2.6 = 35.1` 像素）：

| | 画布 | 首墨 x | 墨 x 范围 | 期望左边距 |
|---|---|---|---|---|
| 修前 scale 1 | 400×678 | **26** | 26..273 | 35.1（26 ≈ 35.1×0.75） |
| 修后 scale 1 | 400×678 | **34** | 34..**365** | 35.1（右端 365 ≈ 400−35.1=364.9） |
| 修后 scale 2 | 800×3606 | **69** | 69..356 | 70.2 |

左边距随 scale 翻倍，说明修复是 DPI 正确的而非固定偏移；把修前包围盒除以 0.75 可逐像素复现修后包围盒，证明只有目标单位变了、`Page.ops` 未动。

**96 DPI 行为逐位不变**（96 是该变更的不动点）：`scale_of(96) = 4/3` 未动、`SetDpi(96,96)` 本就是空操作、`unit_px` 修前修后在 96 下都等于 1.0。所有写死 `DPI = 96.0` 的既有测试原样通过。

**仍需人工过一遍**（代码无法验证的部分）：125%/150% 下的字形锐度（目标钉在 96 后 DirectWrite 按 96 度量栅格化，hinting 可能偏软；若不可接受，替代方案是保留窗口 DPI 并把每个几何坐标除以 `dpi/96`，那是一次大改）、选区与拖拽选中的对齐、find 框与面板的贴合、滚动条到底、跨 100%↔150% 显示器拖动窗口（`WM_DPICHANGED`）时只重排一次。

---

# 第一轮（2026-09-28）

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

> **第二轮已解决本表中的以下条目**：代码块背景缺失（→ 52）、`font.rs:508` 平方级（→ 31）、`images.rs` 缓存失效（→ 44/45）、`view.rs:10895` PDF 克隆与跨页裁剪（P2）、`pagination.rs:307` O(n³)、`view.rs:3359` 启动失败（→ 28，实际情况比记录更严重：`?` 会让 `Box<View>` 在窗口存活期间释放）。`emphasis.rs` 的跨区配对（→ 50）也修了。

| 位置 | 问题 | 不修的理由 / 建议 |
|------|------|------------------|
| `view.rs:3667` DPI 体系 | 渲染目标以窗口 DPI 创建（`dpiX: self.dpi`），而显示列表按**设备像素**计算（`scale_of = dpi/72`，命中测试用原始 px）：两者只在 96 DPI 一致，>100% 缩放下渲染与命中测试互相矛盾。peek 同模式 | **第二轮已重新核验并升级为本轮最大未修项**（见上文"二、第二轮：暂不修复"首行，含完整触发推演）。结论不变：需要一台 >96 DPI 的机器人工验证后再动 |
| `view.rs:692` | worker 排版把公式 intern 进线程局部 `MathStore` 后丢弃，重文档的宽公式预览可能取到错误 store 的索引 | 正确修法是把 store 随 LayoutFinals 送回或宽区直接携带源串，涉及 Send 约束，需要设计 |
| `rubrica-doc/src/lib.rs:403` | `$…$` 无条件启用且 pulldown 的 flanking 只看 ASCII 空白：`成本$3，售价$5` 中间被吞成公式对象 | **与 GitHub 行为一致**（pandoc 有"闭 `$` 后跟数字不闭合"的保护，GitHub 没有）。保持 GitHub 对齐是更可辩护的默认；如要改，需仿 pandoc 的 flanking 后处理，代价是自维护一套判定 |
| `rubrica-doc/src/emphasis.rs` 转义星 | `\*` 与 CJK 相邻时被当强调吃掉 | 修复通道分不清 `\*` 与字面 `*`，需要新增"来自转义"的样式位。改动面大、回归风险高于收益，值得单独立项 |
| `peek.rs:162` | 预览存续时焦点关闭/点击关闭默认关，预览可能长期驻留 | 特性开关（`PeekFocusClose`）。第二轮已在关闭按钮上做到"不依赖钩子存活"，驻留策略本身仍属产品决策 |
| `images.rs` EXIF 方向 | 竖拍照片按未旋转尺寸渲染，且 `natural_size` 报的宽高也是未旋转的 | 新特性，不是 bug |
| `view.rs:3144` relocated_source | 文件变更后按 80 字符尾串重定位，尾串本身被编辑时回退到偏移量近似 | 算法已防 UTF-8 与重复上下文的panic/错配，精度问题接受 |
| `settings.rs:592` | 阅读位置两条注册表写非原子（崩溃窗口内新路径配旧锚点） | 注册表无事务；概率极低且后果是滚动位置异常 |
| `i18n.rs` 其余 / `theme.rs` / `instance.rs` | 审查未发现问题 | — |

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
