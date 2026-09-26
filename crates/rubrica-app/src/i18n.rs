//! Internationalization (I18N) and Localization (L10N) for Rubrica.
//!
//! Provides compile-time validated translations, automatic system language detection,
//! dynamic language switching at runtime, and formatted localized strings for UI elements.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    EnUs,
    ZhCn,
    ZhTw,
    JaJp,
    KoKr,
    FrFr,
    DeDe,
    EsEs,
    RuRu,
    ItIt,
    PtBr,
}

impl Language {
    pub const ALL: [Language; 11] = [
        Language::EnUs,
        Language::ZhCn,
        Language::ZhTw,
        Language::JaJp,
        Language::KoKr,
        Language::FrFr,
        Language::DeDe,
        Language::EsEs,
        Language::RuRu,
        Language::ItIt,
        Language::PtBr,
    ];

    pub fn code(&self) -> &'static str {
        match self {
            Language::EnUs => "en-US",
            Language::ZhCn => "zh-CN",
            Language::ZhTw => "zh-TW",
            Language::JaJp => "ja-JP",
            Language::KoKr => "ko-KR",
            Language::FrFr => "fr-FR",
            Language::DeDe => "de-DE",
            Language::EsEs => "es-ES",
            Language::RuRu => "ru-RU",
            Language::ItIt => "it-IT",
            Language::PtBr => "pt-BR",
        }
    }

    pub fn native_name(&self) -> &'static str {
        match self {
            Language::EnUs => "English",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
            Language::JaJp => "日本語",
            Language::KoKr => "한국어",
            Language::FrFr => "Français",
            Language::DeDe => "Deutsch",
            Language::EsEs => "Español",
            Language::RuRu => "Русский",
            Language::ItIt => "Italiano",
            Language::PtBr => "Português",
        }
    }

    pub fn from_code(code: &str) -> Option<Language> {
        let code = code.trim().to_ascii_lowercase().replace('_', "-");
        if code.starts_with("zh-cn") || code.starts_with("zh-hans") || code.starts_with("zh-sg") || code == "zh" {
            Some(Language::ZhCn)
        } else if code.starts_with("zh-tw") || code.starts_with("zh-hant") || code.starts_with("zh-hk") || code.starts_with("zh-mo") {
            Some(Language::ZhTw)
        } else if code.starts_with("ja") {
            Some(Language::JaJp)
        } else if code.starts_with("ko") {
            Some(Language::KoKr)
        } else if code.starts_with("fr") {
            Some(Language::FrFr)
        } else if code.starts_with("de") {
            Some(Language::DeDe)
        } else if code.starts_with("es") {
            Some(Language::EsEs)
        } else if code.starts_with("ru") {
            Some(Language::RuRu)
        } else if code.starts_with("it") {
            Some(Language::ItIt)
        } else if code.starts_with("pt") {
            Some(Language::PtBr)
        } else if code.starts_with("en") {
            Some(Language::EnUs)
        } else {
            None
        }
    }

    pub fn system_language() -> Language {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        let lang_id = unsafe { GetUserDefaultUILanguage() };
        let primary = lang_id & 0x03ff;
        let sub = lang_id >> 10;
        match primary {
            0x04 => {
                if sub == 0x01 || sub == 0x03 || sub == 0x05 {
                    Language::ZhTw
                } else {
                    Language::ZhCn
                }
            }
            0x11 => Language::JaJp,
            0x12 => Language::KoKr,
            0x0c => Language::FrFr,
            0x07 => Language::DeDe,
            0x0a => Language::EsEs,
            0x19 => Language::RuRu,
            0x10 => Language::ItIt,
            0x16 => Language::PtBr,
            _ => Language::EnUs,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    SampleTitle,
    SourceTag,

    FindCue,
    FindNoMatch,
    FindOneMatch,

    MenuBack,
    MenuForward,
    MenuCopy,
    MenuSelectAll,
    MenuFind,
    MenuOpenLink,
    MenuCopyLink,
    MenuContents,

    MenuZoomIn,
    MenuZoomOut,
    MenuZoomReset,

    MenuReadingMode,
    MenuContinuousScroll,
    MenuPageStack,
    MenuPreviousPage,
    MenuNextPage,

    MenuTypography,
    MenuEditSavePreset,

    FaceDefault,
    FaceSerif,
    FaceHumanist,
    FaceMonospace,
    MeasureNarrow,
    MeasureNormal,
    MeasureWide,
    PresetDefault,
    PresetBook,

    MenuLight,
    MenuDark,
    MenuFollowSystem,

    MenuWorkspaceTree,
    MenuOpenFile,
    MenuReload,
    MenuOpenTabs,
    MenuPinTab,
    MenuCloseTab,
    MenuRecentDocuments,

    MenuSingleNewlines,
    MenuNewlineDefaultMerge,
    MenuNewlineDefaultKeep,
    MenuNewlineDocDefault,
    MenuNewlineDocMerge,
    MenuNewlineDocKeep,

    MenuReadSource,

    MenuExternalEditor,
    MenuOpenInEditor,
    MenuChooseEditor,

    MenuSpacebarPeek,
    MenuPeekEnabled,
    MenuPeekTap,
    MenuPeekHold,
    MenuPeekMixed,
    MenuPeekFocusClose,

    MenuTextReading,
    MenuFormatExt,
    MenuFormatPlain,
    MenuFormatMarkdown,
    MenuParagraphsAuto,
    MenuParagraphsLines,
    MenuParagraphsBlank,
    MenuDetectChapters,
    MenuPreviousChapter,
    MenuNextChapter,
    MenuWideTableNarrow,
    MenuWideTableWiden,
    MenuPreviousFile,
    MenuNextFile,

    MenuTextEncoding,
    MenuEncodingGuessed,

    MenuLanguage,

    TypoTitle,
    TypoPresetName,
    TypoCustom,
    TypoKeepKorean,
    TypoBindTxt,
    TypoFontNotice,
    TypoEditorArgs,
    TypoSaveApply,
    TypoCancel,

    TrayPeekTooltip,
    TrayLaunchAtSignIn,
    TrayExit,

    DialogTitle,
    ErrorCannotStartEditor,
    ErrorCannotReadChapter,
    ErrorCannotIndexTxtChapters,
    ErrorCannotReadChangedChapter,
    ErrorCannotReadTxtChapter,
    ErrorChooseAnotherName,
    ErrorTooManyPresets,
}

pub fn t(lang: Language, key: Key) -> &'static str {
    match lang {
        Language::EnUs => translate_en(key),
        Language::ZhCn => translate_zh_cn(key),
        Language::ZhTw => translate_zh_tw(key),
        Language::JaJp => translate_ja(key),
        Language::KoKr => translate_ko(key),
        Language::FrFr => translate_fr(key),
        Language::DeDe => translate_de(key),
        Language::EsEs => translate_es(key),
        Language::RuRu => translate_ru(key),
        Language::ItIt => translate_it(key),
        Language::PtBr => translate_pt(key),
    }
}

fn translate_en(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "sample",
        Key::SourceTag => "Source",

        Key::FindCue => "Find in document",
        Key::FindNoMatch => "no match",
        Key::FindOneMatch => "1 match",

        Key::MenuBack => "Back",
        Key::MenuForward => "Forward",
        Key::MenuCopy => "Copy",
        Key::MenuSelectAll => "Select All",
        Key::MenuFind => "Find in Document",
        Key::MenuOpenLink => "Open Link",
        Key::MenuCopyLink => "Copy Link Address",
        Key::MenuContents => "Contents",

        Key::MenuZoomIn => "Increase Text",
        Key::MenuZoomOut => "Decrease Text",
        Key::MenuZoomReset => "Actual Size",

        Key::MenuReadingMode => "Reading mode",
        Key::MenuContinuousScroll => "Continuous scroll",
        Key::MenuPageStack => "Page stack",
        Key::MenuPreviousPage => "Previous page",
        Key::MenuNextPage => "Next page",

        Key::MenuTypography => "Typography",
        Key::MenuEditSavePreset => "Edit / Save Preset\u{2026}",

        Key::FaceDefault => "Default",
        Key::FaceSerif => "Serif",
        Key::FaceHumanist => "Humanist",
        Key::FaceMonospace => "Monospace",
        Key::MeasureNarrow => "Narrow",
        Key::MeasureNormal => "Normal",
        Key::MeasureWide => "Wide",
        Key::PresetDefault => "Default",
        Key::PresetBook => "Book",

        Key::MenuLight => "Light",
        Key::MenuDark => "Dark",
        Key::MenuFollowSystem => "Follow System",

        Key::MenuWorkspaceTree => "Workspace tree",
        Key::MenuOpenFile => "Open\u{2026}",
        Key::MenuReload => "Reload",
        Key::MenuOpenTabs => "Open tabs",
        Key::MenuPinTab => "Pin",
        Key::MenuCloseTab => "Close",
        Key::MenuRecentDocuments => "Recent documents",

        Key::MenuSingleNewlines => "Single newlines",
        Key::MenuNewlineDefaultMerge => "Default: Merge into paragraph",
        Key::MenuNewlineDefaultKeep => "Default: Keep line breaks",
        Key::MenuNewlineDocDefault => "This document: Follow default",
        Key::MenuNewlineDocMerge => "This document: Merge",
        Key::MenuNewlineDocKeep => "This document: Keep",

        Key::MenuReadSource => "Read Source",

        Key::MenuExternalEditor => "External editor",
        Key::MenuOpenInEditor => "Open in Editor",
        Key::MenuChooseEditor => "Choose Editor\u{2026}",

        Key::MenuSpacebarPeek => "Spacebar peek",
        Key::MenuPeekEnabled => "Enabled",
        Key::MenuPeekTap => "Space bar: Tap to toggle",
        Key::MenuPeekHold => "Space bar: Hold to preview",
        Key::MenuPeekMixed => "Space bar: Tap or hold",
        Key::MenuPeekFocusClose => "Close when focus moves away",

        Key::MenuTextReading => "Text reading",
        Key::MenuFormatExt => "Format: From file extension",
        Key::MenuFormatPlain => "Format: Plain text",
        Key::MenuFormatMarkdown => "Format: Markdown",
        Key::MenuParagraphsAuto => "Paragraphs: Automatic",
        Key::MenuParagraphsLines => "Paragraphs: Each line",
        Key::MenuParagraphsBlank => "Paragraphs: Blank lines",
        Key::MenuDetectChapters => "Detect chapter headings",
        Key::MenuPreviousChapter => "Previous chapter",
        Key::MenuNextChapter => "Next chapter",
        Key::MenuWideTableNarrow => "Wide table: Borrow less margin",
        Key::MenuWideTableWiden => "Wide table: Borrow more margin",
        Key::MenuPreviousFile => "Previous file",
        Key::MenuNextFile => "Next file",

        Key::MenuTextEncoding => "Text encoding",
        Key::MenuEncodingGuessed => " (detected by guess; choose if incorrect)",

        Key::MenuLanguage => "Language",

        Key::TypoTitle => "Typography",
        Key::TypoPresetName => "Preset name",
        Key::TypoCustom => "Custom",
        Key::TypoKeepKorean => "Keep Korean words together",
        Key::TypoBindTxt => "Use this preset for TXT documents",
        Key::TypoFontNotice => "Use installed font family names. Missing fonts use the fallback families.",
        Key::TypoEditorArgs => "Editor arguments ({file}, {line}, {column})",
        Key::TypoSaveApply => "Save and apply",
        Key::TypoCancel => "Cancel",

        Key::TrayPeekTooltip => "Rubrica peek",
        Key::TrayLaunchAtSignIn => "Launch at sign-in",
        Key::TrayExit => "Exit",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "Cannot start editor",
        Key::ErrorCannotReadChapter => "Cannot read chapter",
        Key::ErrorCannotIndexTxtChapters => "Cannot index the TXT chapters after the file changed.",
        Key::ErrorCannotReadChangedChapter => "Cannot read the changed TXT chapter",
        Key::ErrorCannotReadTxtChapter => "Cannot read the TXT chapter",
        Key::ErrorChooseAnotherName => "Choose a name other than Default or Book (up to 80 bytes).",
        Key::ErrorTooManyPresets => "There are already 100 saved typography presets.",
    }
}

fn translate_zh_cn(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "示例",
        Key::SourceTag => "源码",

        Key::FindCue => "在文档中查找",
        Key::FindNoMatch => "无匹配项",
        Key::FindOneMatch => "1 个匹配项",

        Key::MenuBack => "后退",
        Key::MenuForward => "前进",
        Key::MenuCopy => "复制",
        Key::MenuSelectAll => "全选",
        Key::MenuFind => "在文档中查找",
        Key::MenuOpenLink => "打开链接",
        Key::MenuCopyLink => "复制链接地址",
        Key::MenuContents => "目录",

        Key::MenuZoomIn => "放大文字",
        Key::MenuZoomOut => "缩小文字",
        Key::MenuZoomReset => "实际大小",

        Key::MenuReadingMode => "阅读模式",
        Key::MenuContinuousScroll => "连续滚动",
        Key::MenuPageStack => "页面叠放",
        Key::MenuPreviousPage => "上一页",
        Key::MenuNextPage => "下一页",

        Key::MenuTypography => "排版",
        Key::MenuEditSavePreset => "编辑 / 保存预设…",

        Key::FaceDefault => "默认",
        Key::FaceSerif => "衬线体",
        Key::FaceHumanist => "人文主义体",
        Key::FaceMonospace => "等宽体",
        Key::MeasureNarrow => "窄栏",
        Key::MeasureNormal => "标准",
        Key::MeasureWide => "宽栏",
        Key::PresetDefault => "默认",
        Key::PresetBook => "书籍",

        Key::MenuLight => "浅色",
        Key::MenuDark => "深色",
        Key::MenuFollowSystem => "跟随系统",

        Key::MenuWorkspaceTree => "工作区目录树",
        Key::MenuOpenFile => "打开…",
        Key::MenuReload => "重新加载",
        Key::MenuOpenTabs => "打开的标签页",
        Key::MenuPinTab => "固定",
        Key::MenuCloseTab => "关闭",
        Key::MenuRecentDocuments => "最近打开的文档",

        Key::MenuSingleNewlines => "单换行处理",
        Key::MenuNewlineDefaultMerge => "默认：合并至段落",
        Key::MenuNewlineDefaultKeep => "默认：保留换行",
        Key::MenuNewlineDocDefault => "此文档：跟随默认",
        Key::MenuNewlineDocMerge => "此文档：合并",
        Key::MenuNewlineDocKeep => "此文档：保留换行",

        Key::MenuReadSource => "阅读源码",

        Key::MenuExternalEditor => "外部编辑器",
        Key::MenuOpenInEditor => "在编辑器中打开",
        Key::MenuChooseEditor => "选择编辑器…",

        Key::MenuSpacebarPeek => "空格键预览",
        Key::MenuPeekEnabled => "已启用",
        Key::MenuPeekTap => "空格键：点击切换",
        Key::MenuPeekHold => "空格键：长按预览",
        Key::MenuPeekMixed => "空格键：点击或长按",
        Key::MenuPeekFocusClose => "失去焦点时关闭",

        Key::MenuTextReading => "纯文本阅读",
        Key::MenuFormatExt => "格式：根据文件扩展名",
        Key::MenuFormatPlain => "格式：纯文本",
        Key::MenuFormatMarkdown => "格式：Markdown",
        Key::MenuParagraphsAuto => "段落识别：自动",
        Key::MenuParagraphsLines => "段落识别：每行一段",
        Key::MenuParagraphsBlank => "段落识别：空行分段",
        Key::MenuDetectChapters => "识别章节标题",
        Key::MenuPreviousChapter => "上一章",
        Key::MenuNextChapter => "下一章",
        Key::MenuWideTableNarrow => "宽表格：少占边距",
        Key::MenuWideTableWiden => "宽表格：多占边距",
        Key::MenuPreviousFile => "上一个文件",
        Key::MenuNextFile => "下一个文件",

        Key::MenuTextEncoding => "文本编码",
        Key::MenuEncodingGuessed => "（推测检测；如不正确请手动选择）",

        Key::MenuLanguage => "界面语言",

        Key::TypoTitle => "排版设置",
        Key::TypoPresetName => "预设名称",
        Key::TypoCustom => "自定义",
        Key::TypoKeepKorean => "保持韩文单词不分行",
        Key::TypoBindTxt => "对 TXT 文档使用此预设",
        Key::TypoFontNotice => "使用系统中已安装的字体族名称。缺失的字体将使用后备字体。",
        Key::TypoEditorArgs => "编辑器参数 ({file}, {line}, {column})",
        Key::TypoSaveApply => "保存并应用",
        Key::TypoCancel => "取消",

        Key::TrayPeekTooltip => "Rubrica 快捷预览",
        Key::TrayLaunchAtSignIn => "开机自启",
        Key::TrayExit => "退出",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "无法启动编辑器",
        Key::ErrorCannotReadChapter => "无法读取章节",
        Key::ErrorCannotIndexTxtChapters => "文件发生变更后无法重新索引 TXT 章节。",
        Key::ErrorCannotReadChangedChapter => "无法读取变更后的 TXT 章节",
        Key::ErrorCannotReadTxtChapter => "无法读取 TXT 章节",
        Key::ErrorChooseAnotherName => "请选择 Default 或 Book 以外的名称（最多 80 字节）。",
        Key::ErrorTooManyPresets => "排版预设已达 100 个上限。",
    }
}

fn translate_zh_tw(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "範例",
        Key::SourceTag => "原始碼",

        Key::FindCue => "在文件中尋找",
        Key::FindNoMatch => "無相符項目",
        Key::FindOneMatch => "1 個相符項目",

        Key::MenuBack => "返回",
        Key::MenuForward => "前進",
        Key::MenuCopy => "複製",
        Key::MenuSelectAll => "全選",
        Key::MenuFind => "在文件中尋找",
        Key::MenuOpenLink => "開啟連結",
        Key::MenuCopyLink => "複製連結網址",
        Key::MenuContents => "目錄",

        Key::MenuZoomIn => "放大文字",
        Key::MenuZoomOut => "縮小文字",
        Key::MenuZoomReset => "實際大小",

        Key::MenuReadingMode => "閱讀模式",
        Key::MenuContinuousScroll => "連續捲動",
        Key::MenuPageStack => "頁面堆疊",
        Key::MenuPreviousPage => "上一頁",
        Key::MenuNextPage => "下一頁",

        Key::MenuTypography => "版面排版",
        Key::MenuEditSavePreset => "編輯 / 儲存預設…",

        Key::FaceDefault => "預設",
        Key::FaceSerif => "襯線體",
        Key::FaceHumanist => "人文主義體",
        Key::FaceMonospace => "等寬體",
        Key::MeasureNarrow => "窄欄",
        Key::MeasureNormal => "標準",
        Key::MeasureWide => "寬欄",
        Key::PresetDefault => "預設",
        Key::PresetBook => "書籍",

        Key::MenuLight => "淺色",
        Key::MenuDark => "深色",
        Key::MenuFollowSystem => "跟隨系統",

        Key::MenuWorkspaceTree => "工作區目錄樹",
        Key::MenuOpenFile => "開啟…",
        Key::MenuReload => "重新載入",
        Key::MenuOpenTabs => "已開啟的標籤頁",
        Key::MenuPinTab => "釘選",
        Key::MenuCloseTab => "關閉",
        Key::MenuRecentDocuments => "最近開啟的文件",

        Key::MenuSingleNewlines => "單換行處理",
        Key::MenuNewlineDefaultMerge => "預設：合併至段落",
        Key::MenuNewlineDefaultKeep => "預設：保留換行",
        Key::MenuNewlineDocDefault => "此文件：跟隨預設",
        Key::MenuNewlineDocMerge => "此文件：合併",
        Key::MenuNewlineDocKeep => "此文件：保留換行",

        Key::MenuReadSource => "檢視原始碼",

        Key::MenuExternalEditor => "外部編輯器",
        Key::MenuOpenInEditor => "在編輯器中開啟",
        Key::MenuChooseEditor => "選擇編輯器…",

        Key::MenuSpacebarPeek => "空格鍵預覽",
        Key::MenuPeekEnabled => "已啟用",
        Key::MenuPeekTap => "空格鍵：按一下切換",
        Key::MenuPeekHold => "空格鍵：長按預覽",
        Key::MenuPeekMixed => "空格鍵：按一下或長按",
        Key::MenuPeekFocusClose => "失去焦點時關閉",

        Key::MenuTextReading => "純文字閱讀",
        Key::MenuFormatExt => "格式：依副檔名判斷",
        Key::MenuFormatPlain => "格式：純文字",
        Key::MenuFormatMarkdown => "格式：Markdown",
        Key::MenuParagraphsAuto => "段落識別：自動",
        Key::MenuParagraphsLines => "段落識別：每行一段",
        Key::MenuParagraphsBlank => "段落識別：空行分段",
        Key::MenuDetectChapters => "識別章節標題",
        Key::MenuPreviousChapter => "上一章",
        Key::MenuNextChapter => "下一章",
        Key::MenuWideTableNarrow => "寬表格：少佔邊距",
        Key::MenuWideTableWiden => "寬表格：多佔邊距",
        Key::MenuPreviousFile => "上一個檔案",
        Key::MenuNextFile => "下一個檔案",

        Key::MenuTextEncoding => "文字編碼",
        Key::MenuEncodingGuessed => "（推測檢測；若不正確請手動選擇）",

        Key::MenuLanguage => "介面語言",

        Key::TypoTitle => "排版設定",
        Key::TypoPresetName => "預設名稱",
        Key::TypoCustom => "自訂",
        Key::TypoKeepKorean => "保持韓文單詞不換行",
        Key::TypoBindTxt => "對 TXT 文件使用此預設",
        Key::TypoFontNotice => "使用系統中已安裝的字型家族名稱。缺失的字型將使用後備字型。",
        Key::TypoEditorArgs => "編輯器引數 ({file}, {line}, {column})",
        Key::TypoSaveApply => "儲存並套用",
        Key::TypoCancel => "取消",

        Key::TrayPeekTooltip => "Rubrica 快捷預覽",
        Key::TrayLaunchAtSignIn => "開機自動啟動",
        Key::TrayExit => "結束",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "無法啟動編輯器",
        Key::ErrorCannotReadChapter => "無法讀取章節",
        Key::ErrorCannotIndexTxtChapters => "檔案變更後無法建立 TXT 章節索引。",
        Key::ErrorCannotReadChangedChapter => "無法讀取變更後的 TXT 章節",
        Key::ErrorCannotReadTxtChapter => "無法讀取 TXT 章節",
        Key::ErrorChooseAnotherName => "請選擇 Default 或 Book 以外的名稱（最多 80 位元組）。",
        Key::ErrorTooManyPresets => "排版預設已達 100 個上限。",
    }
}

fn translate_ja(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "サンプル",
        Key::SourceTag => "ソース",

        Key::FindCue => "ドキュメント内を検索",
        Key::FindNoMatch => "一致なし",
        Key::FindOneMatch => "1 件一致",

        Key::MenuBack => "戻る",
        Key::MenuForward => "進む",
        Key::MenuCopy => "コピー",
        Key::MenuSelectAll => "すべて選択",
        Key::MenuFind => "ドキュメント内を検索",
        Key::MenuOpenLink => "リンクを開く",
        Key::MenuCopyLink => "リンクのアドレスをコピー",
        Key::MenuContents => "目次",

        Key::MenuZoomIn => "テキストを拡大",
        Key::MenuZoomOut => "テキストを縮小",
        Key::MenuZoomReset => "実際のサイズ",

        Key::MenuReadingMode => "閲覧モード",
        Key::MenuContinuousScroll => "連続スクロール",
        Key::MenuPageStack => "ページスタック",
        Key::MenuPreviousPage => "前のページ",
        Key::MenuNextPage => "次のページ",

        Key::MenuTypography => "タイポグラフィ",
        Key::MenuEditSavePreset => "プリセットの編集 / 保存\u{2026}",

        Key::FaceDefault => "既定",
        Key::FaceSerif => "明朝 / セリフ",
        Key::FaceHumanist => "ヒューマニスト",
        Key::FaceMonospace => "等幅",
        Key::MeasureNarrow => "狭い",
        Key::MeasureNormal => "標準",
        Key::MeasureWide => "広い",
        Key::PresetDefault => "既定",
        Key::PresetBook => "書籍",

        Key::MenuLight => "ライト",
        Key::MenuDark => "ダーク",
        Key::MenuFollowSystem => "システムに従う",

        Key::MenuWorkspaceTree => "ワークスペースツリー",
        Key::MenuOpenFile => "開く\u{2026}",
        Key::MenuReload => "再読み込み",
        Key::MenuOpenTabs => "開いているタブ",
        Key::MenuPinTab => "固定",
        Key::MenuCloseTab => "閉じる",
        Key::MenuRecentDocuments => "最近開いたドキュメント",

        Key::MenuSingleNewlines => "単一の改行",
        Key::MenuNewlineDefaultMerge => "既定: 段落に結合",
        Key::MenuNewlineDefaultKeep => "既定: 改行を保持",
        Key::MenuNewlineDocDefault => "このドキュメント: 既定に従う",
        Key::MenuNewlineDocMerge => "このドキュメント: 結合",
        Key::MenuNewlineDocKeep => "このドキュメント: 保持",

        Key::MenuReadSource => "ソースを表示",

        Key::MenuExternalEditor => "外部エディタ",
        Key::MenuOpenInEditor => "エディタで開く",
        Key::MenuChooseEditor => "エディタを選択\u{2026}",

        Key::MenuSpacebarPeek => "スペースキープレビュー",
        Key::MenuPeekEnabled => "有効",
        Key::MenuPeekTap => "スペースキー: タップで切り替え",
        Key::MenuPeekHold => "スペースキー: 長押しでプレビュー",
        Key::MenuPeekMixed => "スペースキー: タップまたは長押し",
        Key::MenuPeekFocusClose => "フォーカスが外れたら閉じる",

        Key::MenuTextReading => "テキスト閲覧",
        Key::MenuFormatExt => "形式: 拡張子から判定",
        Key::MenuFormatPlain => "形式: プレーンテキスト",
        Key::MenuFormatMarkdown => "形式: Markdown",
        Key::MenuParagraphsAuto => "段落認識: 自動",
        Key::MenuParagraphsLines => "段落認識: 各行ごと",
        Key::MenuParagraphsBlank => "段落認識: 空行で区切る",
        Key::MenuDetectChapters => "章の見出しを検出",
        Key::MenuPreviousChapter => "前の章",
        Key::MenuNextChapter => "次の章",
        Key::MenuWideTableNarrow => "幅広テーブル: 余白の借用を減らす",
        Key::MenuWideTableWiden => "幅広テーブル: 余白の借用を増やす",
        Key::MenuPreviousFile => "前のファイル",
        Key::MenuNextFile => "次のファイル",

        Key::MenuTextEncoding => "文字エンコード",
        Key::MenuEncodingGuessed => "（推定により検出；正しくない場合は選択）",

        Key::MenuLanguage => "言語",

        Key::TypoTitle => "タイポグラフィ",
        Key::TypoPresetName => "プリセット名",
        Key::TypoCustom => "カスタム",
        Key::TypoKeepKorean => "韓国語の単語を折り返さない",
        Key::TypoBindTxt => "TXT ドキュメントにこのプリセットを使用",
        Key::TypoFontNotice => "インストール済みのフォント名を使用してください。見つからない場合は代替フォントが使用されます。",
        Key::TypoEditorArgs => "エディタ引数 ({file}, {line}, {column})",
        Key::TypoSaveApply => "保存して適用",
        Key::TypoCancel => "キャンセル",

        Key::TrayPeekTooltip => "Rubrica プレビュー",
        Key::TrayLaunchAtSignIn => "サインイン時に起動",
        Key::TrayExit => "終了",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "エディタを起動できません",
        Key::ErrorCannotReadChapter => "章を読み取れません",
        Key::ErrorCannotIndexTxtChapters => "ファイル変更後に TXT の章インデックスを作成できません。",
        Key::ErrorCannotReadChangedChapter => "変更された TXT の章を読み取れません",
        Key::ErrorCannotReadTxtChapter => "TXT の章を読み取れません",
        Key::ErrorChooseAnotherName => "Default や Book 以外の名前を指定してください（最大 80 バイト）。",
        Key::ErrorTooManyPresets => "保存されたタイポグラフィプリセットが既に 100 個あります。",
    }
}

fn translate_ko(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "샘플",
        Key::SourceTag => "소스",

        Key::FindCue => "문서에서 찾기",
        Key::FindNoMatch => "일치 항목 없음",
        Key::FindOneMatch => "1개 일치",

        Key::MenuBack => "뒤로",
        Key::MenuForward => "앞으로",
        Key::MenuCopy => "복사",
        Key::MenuSelectAll => "모두 선택",
        Key::MenuFind => "문서에서 찾기",
        Key::MenuOpenLink => "링크 열기",
        Key::MenuCopyLink => "링크 주소 복사",
        Key::MenuContents => "목차",

        Key::MenuZoomIn => "글자 확대",
        Key::MenuZoomOut => "글자 축소",
        Key::MenuZoomReset => "원래 크기",

        Key::MenuReadingMode => "읽기 모드",
        Key::MenuContinuousScroll => "연속 스크롤",
        Key::MenuPageStack => "페이지 스택",
        Key::MenuPreviousPage => "이전 페이지",
        Key::MenuNextPage => "다음 페이지",

        Key::MenuTypography => "타이포그래피",
        Key::MenuEditSavePreset => "프리셋 편집 / 저장\u{2026}",

        Key::FaceDefault => "기본값",
        Key::FaceSerif => "명조 / 세리프",
        Key::FaceHumanist => "휴머니스트",
        Key::FaceMonospace => "고정폭",
        Key::MeasureNarrow => "좁게",
        Key::MeasureNormal => "보통",
        Key::MeasureWide => "넓게",
        Key::PresetDefault => "기본값",
        Key::PresetBook => "서적",

        Key::MenuLight => "밝게",
        Key::MenuDark => "어둡게",
        Key::MenuFollowSystem => "시스템 설정 따름",

        Key::MenuWorkspaceTree => "작업 영역 트리",
        Key::MenuOpenFile => "열기\u{2026}",
        Key::MenuReload => "새로 고침",
        Key::MenuOpenTabs => "열린 탭",
        Key::MenuPinTab => "고정",
        Key::MenuCloseTab => "닫기",
        Key::MenuRecentDocuments => "최근 문서",

        Key::MenuSingleNewlines => "단일 줄 바꿈",
        Key::MenuNewlineDefaultMerge => "기본값: 단락으로 병합",
        Key::MenuNewlineDefaultKeep => "기본값: 줄 바꿈 유지",
        Key::MenuNewlineDocDefault => "이 문서: 기본값 따름",
        Key::MenuNewlineDocMerge => "이 문서: 병합",
        Key::MenuNewlineDocKeep => "이 문서: 유지",

        Key::MenuReadSource => "소스 보기",

        Key::MenuExternalEditor => "외부 편집기",
        Key::MenuOpenInEditor => "편집기에서 열기",
        Key::MenuChooseEditor => "편집기 선택\u{2026}",

        Key::MenuSpacebarPeek => "스페이스바 미리보기",
        Key::MenuPeekEnabled => "사용",
        Key::MenuPeekTap => "스페이스바: 탭하여 전환",
        Key::MenuPeekHold => "스페이스바: 길게 눌러 미리보기",
        Key::MenuPeekMixed => "스페이스바: 탭 또는 길게 누르기",
        Key::MenuPeekFocusClose => "포커스를 잃으면 닫기",

        Key::MenuTextReading => "텍스트 읽기",
        Key::MenuFormatExt => "형식: 파일 확장자 기준",
        Key::MenuFormatPlain => "형식: 일반 텍스트",
        Key::MenuFormatMarkdown => "형식: Markdown",
        Key::MenuParagraphsAuto => "단락 인식: 자동",
        Key::MenuParagraphsLines => "단락 인식: 각 줄마다",
        Key::MenuParagraphsBlank => "단락 인식: 빈 줄 기준",
        Key::MenuDetectChapters => "장 제목 감지",
        Key::MenuPreviousChapter => "이전 장",
        Key::MenuNextChapter => "다음 장",
        Key::MenuWideTableNarrow => "넓은 표: 여백 축소",
        Key::MenuWideTableWiden => "넓은 표: 여백 확대",
        Key::MenuPreviousFile => "이전 파일",
        Key::MenuNextFile => "다음 파일",

        Key::MenuTextEncoding => "텍스트 인코딩",
        Key::MenuEncodingGuessed => " (추정에 의해 감지됨; 잘못된 경우 선택)",

        Key::MenuLanguage => "언어",

        Key::TypoTitle => "타이포그래피",
        Key::TypoPresetName => "프리셋 이름",
        Key::TypoCustom => "사용자 지정",
        Key::TypoKeepKorean => "한국어 단어 분리 방지",
        Key::TypoBindTxt => "TXT 문서에 이 프리셋 적용",
        Key::TypoFontNotice => "설치된 글꼴 패밀리 이름을 사용하세요. 없는 글꼴은 대체 글꼴이 사용됩니다.",
        Key::TypoEditorArgs => "편집기 인수 ({file}, {line}, {column})",
        Key::TypoSaveApply => "저장 및 적용",
        Key::TypoCancel => "취소",

        Key::TrayPeekTooltip => "Rubrica 미리보기",
        Key::TrayLaunchAtSignIn => "로그인 시 실행",
        Key::TrayExit => "종료",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "편집기를 시작할 수 없습니다",
        Key::ErrorCannotReadChapter => "장을 읽을 수 없습니다",
        Key::ErrorCannotIndexTxtChapters => "파일이 변경된 후 TXT 장을 인덱싱할 수 없습니다.",
        Key::ErrorCannotReadChangedChapter => "변경된 TXT 장을 읽을 수 없습니다",
        Key::ErrorCannotReadTxtChapter => "TXT 장을 읽을 수 없습니다",
        Key::ErrorChooseAnotherName => "Default 또는 Book 이외의 이름을 선택하세요(최대 80바이트).",
        Key::ErrorTooManyPresets => "저장된 타이포그래피 프리셋이 이미 100개 있습니다.",
    }
}

fn translate_fr(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "échantillon",
        Key::SourceTag => "Source",

        Key::FindCue => "Rechercher dans le document",
        Key::FindNoMatch => "aucune correspondance",
        Key::FindOneMatch => "1 correspondance",

        Key::MenuBack => "Précédent",
        Key::MenuForward => "Suivant",
        Key::MenuCopy => "Copier",
        Key::MenuSelectAll => "Tout sélectionner",
        Key::MenuFind => "Rechercher dans le document",
        Key::MenuOpenLink => "Ouvrir le lien",
        Key::MenuCopyLink => "Copier l'adresse du lien",
        Key::MenuContents => "Table des matières",

        Key::MenuZoomIn => "Agrandir le texte",
        Key::MenuZoomOut => "Diminuer le texte",
        Key::MenuZoomReset => "Taille réelle",

        Key::MenuReadingMode => "Mode de lecture",
        Key::MenuContinuousScroll => "Défilement continu",
        Key::MenuPageStack => "Pile de pages",
        Key::MenuPreviousPage => "Page précédente",
        Key::MenuNextPage => "Page suivante",

        Key::MenuTypography => "Typographie",
        Key::MenuEditSavePreset => "Modifier / Enregistrer le préréglage\u{2026}",

        Key::FaceDefault => "Par défaut",
        Key::FaceSerif => "Serif",
        Key::FaceHumanist => "Humaniste",
        Key::FaceMonospace => "Chasse fixe",
        Key::MeasureNarrow => "Étroit",
        Key::MeasureNormal => "Normal",
        Key::MeasureWide => "Large",
        Key::PresetDefault => "Par défaut",
        Key::PresetBook => "Livre",

        Key::MenuLight => "Clair",
        Key::MenuDark => "Sombre",
        Key::MenuFollowSystem => "Suivre le système",

        Key::MenuWorkspaceTree => "Arborescence",
        Key::MenuOpenFile => "Ouvrir\u{2026}",
        Key::MenuReload => "Recharger",
        Key::MenuOpenTabs => "Onglets ouverts",
        Key::MenuPinTab => "Épingler",
        Key::MenuCloseTab => "Fermer",
        Key::MenuRecentDocuments => "Documents récents",

        Key::MenuSingleNewlines => "Retours à la ligne simples",
        Key::MenuNewlineDefaultMerge => "Par défaut : Fusionner dans le paragraphe",
        Key::MenuNewlineDefaultKeep => "Par défaut : Conserver les retours à la ligne",
        Key::MenuNewlineDocDefault => "Ce document : Suivre la valeur par défaut",
        Key::MenuNewlineDocMerge => "Ce document : Fusionner",
        Key::MenuNewlineDocKeep => "Ce document : Conserver",

        Key::MenuReadSource => "Afficher la source",

        Key::MenuExternalEditor => "Éditeur externe",
        Key::MenuOpenInEditor => "Ouvrir dans l'éditeur",
        Key::MenuChooseEditor => "Choisir un éditeur\u{2026}",

        Key::MenuSpacebarPeek => "Aperçu barre d'espace",
        Key::MenuPeekEnabled => "Activé",
        Key::MenuPeekTap => "Barre d'espace : Appuyer pour basculer",
        Key::MenuPeekHold => "Barre d'espace : Maintenir pour l'aperçu",
        Key::MenuPeekMixed => "Barre d'espace : Appuyer ou maintenir",
        Key::MenuPeekFocusClose => "Fermer lorsque le focus change",

        Key::MenuTextReading => "Lecture de texte",
        Key::MenuFormatExt => "Format : Selon l'extension",
        Key::MenuFormatPlain => "Format : Texte brut",
        Key::MenuFormatMarkdown => "Format : Markdown",
        Key::MenuParagraphsAuto => "Paragraphes : Automatique",
        Key::MenuParagraphsLines => "Paragraphes : Chaque ligne",
        Key::MenuParagraphsBlank => "Paragraphes : Lignes vides",
        Key::MenuDetectChapters => "Détecter les titres de chapitre",
        Key::MenuPreviousChapter => "Chapitre précédent",
        Key::MenuNextChapter => "Chapitre suivant",
        Key::MenuWideTableNarrow => "Tableau large : Moins de marge",
        Key::MenuWideTableWiden => "Tableau large : Plus de marge",
        Key::MenuPreviousFile => "Fichier précédent",
        Key::MenuNextFile => "Fichier suivant",

        Key::MenuTextEncoding => "Encodage du texte",
        Key::MenuEncodingGuessed => " (détecté par déduction ; choisir si incorrect)",

        Key::MenuLanguage => "Langue",

        Key::TypoTitle => "Typographie",
        Key::TypoPresetName => "Nom du préréglage",
        Key::TypoCustom => "Personnalisé",
        Key::TypoKeepKorean => "Garder les mots coréens ensemble",
        Key::TypoBindTxt => "Utiliser ce préréglage pour les documents TXT",
        Key::TypoFontNotice => "Utilisez les noms de polices installées. Les polices manquantes utiliseront les polices de secours.",
        Key::TypoEditorArgs => "Arguments de l'éditeur ({file}, {line}, {column})",
        Key::TypoSaveApply => "Enregistrer et appliquer",
        Key::TypoCancel => "Annuler",

        Key::TrayPeekTooltip => "Rubrica aperçu",
        Key::TrayLaunchAtSignIn => "Lancer à la connexion",
        Key::TrayExit => "Quitter",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "Impossible de lancer l'éditeur",
        Key::ErrorCannotReadChapter => "Impossible de lire le chapitre",
        Key::ErrorCannotIndexTxtChapters => "Impossible d'indexer les chapitres TXT après modification du fichier.",
        Key::ErrorCannotReadChangedChapter => "Impossible de lire le chapitre TXT modifié",
        Key::ErrorCannotReadTxtChapter => "Impossible de lire le chapitre TXT",
        Key::ErrorChooseAnotherName => "Choisissez un nom autre que Default ou Book (jusqu'à 80 octets).",
        Key::ErrorTooManyPresets => "Il y a déjà 100 préréglages typographiques enregistrés.",
    }
}

fn translate_de(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "Beispiel",
        Key::SourceTag => "Quelltext",

        Key::FindCue => "Im Dokument suchen",
        Key::FindNoMatch => "keine Treffer",
        Key::FindOneMatch => "1 Treffer",

        Key::MenuBack => "Zurück",
        Key::MenuForward => "Vorwärts",
        Key::MenuCopy => "Kopieren",
        Key::MenuSelectAll => "Alles auswählen",
        Key::MenuFind => "Im Dokument suchen",
        Key::MenuOpenLink => "Link öffnen",
        Key::MenuCopyLink => "Link-Adresse kopieren",
        Key::MenuContents => "Inhalt",

        Key::MenuZoomIn => "Text vergrößern",
        Key::MenuZoomOut => "Text verkleinern",
        Key::MenuZoomReset => "Tatsächliche Größe",

        Key::MenuReadingMode => "Lesemodus",
        Key::MenuContinuousScroll => "Fortlaufendes Scrollen",
        Key::MenuPageStack => "Seitenstapel",
        Key::MenuPreviousPage => "Vorherige Seite",
        Key::MenuNextPage => "Nächste Seite",

        Key::MenuTypography => "Typografie",
        Key::MenuEditSavePreset => "Voreinstellung bearbeiten / speichern\u{2026}",

        Key::FaceDefault => "Standard",
        Key::FaceSerif => "Serif",
        Key::FaceHumanist => "Humanistisch",
        Key::FaceMonospace => "Nichtproportional",
        Key::MeasureNarrow => "Schmal",
        Key::MeasureNormal => "Normal",
        Key::MeasureWide => "Breit",
        Key::PresetDefault => "Standard",
        Key::PresetBook => "Buch",

        Key::MenuLight => "Hell",
        Key::MenuDark => "Dunkel",
        Key::MenuFollowSystem => "Systemeinstellung folgen",

        Key::MenuWorkspaceTree => "Arbeitsbereich-Baum",
        Key::MenuOpenFile => "Öffnen\u{2026}",
        Key::MenuReload => "Neu laden",
        Key::MenuOpenTabs => "Offene Tabs",
        Key::MenuPinTab => "Anheften",
        Key::MenuCloseTab => "Schließen",
        Key::MenuRecentDocuments => "Zuletzt geöffnete Dokumente",

        Key::MenuSingleNewlines => "Einfache Zeilenumbrüche",
        Key::MenuNewlineDefaultMerge => "Standard: In Absatz zusammenführen",
        Key::MenuNewlineDefaultKeep => "Standard: Zeilenumbrüche beibehalten",
        Key::MenuNewlineDocDefault => "Dieses Dokument: Standard folgen",
        Key::MenuNewlineDocMerge => "Dieses Dokument: Zusammenführen",
        Key::MenuNewlineDocKeep => "Dieses Dokument: Beibehalten",

        Key::MenuReadSource => "Quelltext anzeigen",

        Key::MenuExternalEditor => "Externer Editor",
        Key::MenuOpenInEditor => "Im Editor öffnen",
        Key::MenuChooseEditor => "Editor auswählen\u{2026}",

        Key::MenuSpacebarPeek => "Leertasten-Vorschau",
        Key::MenuPeekEnabled => "Aktiviert",
        Key::MenuPeekTap => "Leertaste: Antippen zum Umschalten",
        Key::MenuPeekHold => "Leertaste: Halten für Vorschau",
        Key::MenuPeekMixed => "Leertaste: Antippen oder Halten",
        Key::MenuPeekFocusClose => "Schließen bei Fokusverlust",

        Key::MenuTextReading => "Textlesen",
        Key::MenuFormatExt => "Format: Nach Dateierweiterung",
        Key::MenuFormatPlain => "Format: Nur-Text",
        Key::MenuFormatMarkdown => "Format: Markdown",
        Key::MenuParagraphsAuto => "Absätze: Automatisch",
        Key::MenuParagraphsLines => "Absätze: Jede Zeile",
        Key::MenuParagraphsBlank => "Absätze: Leerzeilen",
        Key::MenuDetectChapters => "Kapitelüberschriften erkennen",
        Key::MenuPreviousChapter => "Vorheriges Kapitel",
        Key::MenuNextChapter => "Nächstes Kapitel",
        Key::MenuWideTableNarrow => "Breite Tabelle: Weniger Rand nutzen",
        Key::MenuWideTableWiden => "Breite Tabelle: Mehr Rand nutzen",
        Key::MenuPreviousFile => "Vorherige Datei",
        Key::MenuNextFile => "Nächste Datei",

        Key::MenuTextEncoding => "Textcodierung",
        Key::MenuEncodingGuessed => " (vermutet; bei Fehlern bitte auswählen)",

        Key::MenuLanguage => "Sprache",

        Key::TypoTitle => "Typografie",
        Key::TypoPresetName => "Voreinstellungsname",
        Key::TypoCustom => "Benutzerdefiniert",
        Key::TypoKeepKorean => "Koreanische Wörter zusammenhalten",
        Key::TypoBindTxt => "Diese Voreinstellung für TXT-Dokumente verwenden",
        Key::TypoFontNotice => "Verwenden Sie installierte Schriftfamilien. Fehlende Schriften verwenden Ausweichschriften.",
        Key::TypoEditorArgs => "Editor-Argumente ({file}, {line}, {column})",
        Key::TypoSaveApply => "Speichern und anwenden",
        Key::TypoCancel => "Abbrechen",

        Key::TrayPeekTooltip => "Rubrica Vorschau",
        Key::TrayLaunchAtSignIn => "Beim Anmelden starten",
        Key::TrayExit => "Beenden",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "Editor kann nicht gestartet werden",
        Key::ErrorCannotReadChapter => "Kapitel kann nicht gelesen werden",
        Key::ErrorCannotIndexTxtChapters => "TXT-Kapitel können nach Dateiänderung nicht indiziert werden.",
        Key::ErrorCannotReadChangedChapter => "Geändertes TXT-Kapitel kann nicht gelesen werden",
        Key::ErrorCannotReadTxtChapter => "TXT-Kapitel kann nicht gelesen werden",
        Key::ErrorChooseAnotherName => "Wählen Sie einen anderen Namen als Default oder Book (bis zu 80 Bytes).",
        Key::ErrorTooManyPresets => "Es gibt bereits 100 gespeicherte Typografie-Voreinstellungen.",
    }
}

fn translate_es(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "ejemplo",
        Key::SourceTag => "Fuente",

        Key::FindCue => "Buscar en el documento",
        Key::FindNoMatch => "sin coincidencias",
        Key::FindOneMatch => "1 coincidencia",

        Key::MenuBack => "Atrás",
        Key::MenuForward => "Adelante",
        Key::MenuCopy => "Copiar",
        Key::MenuSelectAll => "Seleccionar todo",
        Key::MenuFind => "Buscar en el documento",
        Key::MenuOpenLink => "Abrir enlace",
        Key::MenuCopyLink => "Copiar dirección del enlace",
        Key::MenuContents => "Contenido",

        Key::MenuZoomIn => "Aumentar texto",
        Key::MenuZoomOut => "Reducir texto",
        Key::MenuZoomReset => "Tamaño real",

        Key::MenuReadingMode => "Modo de lectura",
        Key::MenuContinuousScroll => "Desplazamiento continuo",
        Key::MenuPageStack => "Pila de páginas",
        Key::MenuPreviousPage => "Página anterior",
        Key::MenuNextPage => "Página siguiente",

        Key::MenuTypography => "Tipografía",
        Key::MenuEditSavePreset => "Editar / Guardar ajuste\u{2026}",

        Key::FaceDefault => "Predeterminado",
        Key::FaceSerif => "Serif",
        Key::FaceHumanist => "Humanista",
        Key::FaceMonospace => "Monoespaciado",
        Key::MeasureNarrow => "Estrecho",
        Key::MeasureNormal => "Normal",
        Key::MeasureWide => "Ancho",
        Key::PresetDefault => "Predeterminado",
        Key::PresetBook => "Libro",

        Key::MenuLight => "Claro",
        Key::MenuDark => "Oscuro",
        Key::MenuFollowSystem => "Seguir el sistema",

        Key::MenuWorkspaceTree => "Árbol del espacio de trabajo",
        Key::MenuOpenFile => "Abrir\u{2026}",
        Key::MenuReload => "Recargar",
        Key::MenuOpenTabs => "Pestañas abiertas",
        Key::MenuPinTab => "Fijar",
        Key::MenuCloseTab => "Cerrar",
        Key::MenuRecentDocuments => "Documentos recientes",

        Key::MenuSingleNewlines => "Saltos de línea individuales",
        Key::MenuNewlineDefaultMerge => "Predeterminado: Combinar en párrafo",
        Key::MenuNewlineDefaultKeep => "Predeterminado: Mantener saltos de línea",
        Key::MenuNewlineDocDefault => "Este documento: Seguir predeterminado",
        Key::MenuNewlineDocMerge => "Este documento: Combinar",
        Key::MenuNewlineDocKeep => "Este documento: Mantener",

        Key::MenuReadSource => "Ver código fuente",

        Key::MenuExternalEditor => "Editor externo",
        Key::MenuOpenInEditor => "Abrir en el editor",
        Key::MenuChooseEditor => "Elegir editor\u{2026}",

        Key::MenuSpacebarPeek => "Vista previa con espacio",
        Key::MenuPeekEnabled => "Activado",
        Key::MenuPeekTap => "Barra espaciadora: Pulsar para alternar",
        Key::MenuPeekHold => "Barra espaciadora: Mantener para vista previa",
        Key::MenuPeekMixed => "Barra espaciadora: Pulsar o mantener",
        Key::MenuPeekFocusClose => "Cerrar al perder el foco",

        Key::MenuTextReading => "Lectura de texto",
        Key::MenuFormatExt => "Formato: Según la extensión",
        Key::MenuFormatPlain => "Formato: Texto sin formato",
        Key::MenuFormatMarkdown => "Formato: Markdown",
        Key::MenuParagraphsAuto => "Párrafos: Automático",
        Key::MenuParagraphsLines => "Párrafos: Cada línea",
        Key::MenuParagraphsBlank => "Párrafos: Líneas en blanco",
        Key::MenuDetectChapters => "Detectar encabezados de capítulo",
        Key::MenuPreviousChapter => "Capítulo anterior",
        Key::MenuNextChapter => "Capítulo siguiente",
        Key::MenuWideTableNarrow => "Tabla ancha: Tomar menos margen",
        Key::MenuWideTableWiden => "Tabla ancha: Tomar más margen",
        Key::MenuPreviousFile => "Archivo anterior",
        Key::MenuNextFile => "Archivo siguiente",

        Key::MenuTextEncoding => "Codificación de texto",
        Key::MenuEncodingGuessed => " (detectado por deducción; elija si es incorrecto)",

        Key::MenuLanguage => "Idioma",

        Key::TypoTitle => "Tipografía",
        Key::TypoPresetName => "Nombre del ajuste",
        Key::TypoCustom => "Personalizado",
        Key::TypoKeepKorean => "Mantener juntas las palabras coreanas",
        Key::TypoBindTxt => "Usar este ajuste para documentos TXT",
        Key::TypoFontNotice => "Use nombres de familias de fuentes instaladas. Las fuentes faltantes usarán las de respaldo.",
        Key::TypoEditorArgs => "Argumentos del editor ({file}, {line}, {column})",
        Key::TypoSaveApply => "Guardar y aplicar",
        Key::TypoCancel => "Cancelar",

        Key::TrayPeekTooltip => "Rubrica vista previa",
        Key::TrayLaunchAtSignIn => "Iniciar al iniciar sesión",
        Key::TrayExit => "Salir",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "No se puede iniciar el editor",
        Key::ErrorCannotReadChapter => "No se puede leer el capítulo",
        Key::ErrorCannotIndexTxtChapters => "No se pueden indexar los capítulos TXT después de que cambió el archivo.",
        Key::ErrorCannotReadChangedChapter => "No se puede leer el capítulo TXT modificado",
        Key::ErrorCannotReadTxtChapter => "No se puede leer el capítulo TXT",
        Key::ErrorChooseAnotherName => "Elija un nombre distinto de Default o Book (hasta 80 bytes).",
        Key::ErrorTooManyPresets => "Ya hay 100 ajustes tipográficos guardados.",
    }
}

fn translate_ru(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "образец",
        Key::SourceTag => "Исходный текст",

        Key::FindCue => "Найти в документе",
        Key::FindNoMatch => "нет совпадений",
        Key::FindOneMatch => "1 совпадение",

        Key::MenuBack => "Назад",
        Key::MenuForward => "Вперед",
        Key::MenuCopy => "Копировать",
        Key::MenuSelectAll => "Выделить всё",
        Key::MenuFind => "Найти в документе",
        Key::MenuOpenLink => "Открыть ссылку",
        Key::MenuCopyLink => "Копировать адрес ссылки",
        Key::MenuContents => "Содержание",

        Key::MenuZoomIn => "Увеличить текст",
        Key::MenuZoomOut => "Уменьшить текст",
        Key::MenuZoomReset => "Фактический размер",

        Key::MenuReadingMode => "Режим чтения",
        Key::MenuContinuousScroll => "Непрерывная прокрутка",
        Key::MenuPageStack => "Стопка страниц",
        Key::MenuPreviousPage => "Предыдущая страница",
        Key::MenuNextPage => "Следующая страница",

        Key::MenuTypography => "Типографика",
        Key::MenuEditSavePreset => "Изменить / сохранить набор\u{2026}",

        Key::FaceDefault => "По умолчанию",
        Key::FaceSerif => "С засечками",
        Key::FaceHumanist => "Гуманистический",
        Key::FaceMonospace => "Моноширинный",
        Key::MeasureNarrow => "Узкий",
        Key::MeasureNormal => "Стандартный",
        Key::MeasureWide => "Широкий",
        Key::PresetDefault => "По умолчанию",
        Key::PresetBook => "Книга",

        Key::MenuLight => "Светлая",
        Key::MenuDark => "Темная",
        Key::MenuFollowSystem => "Как в системе",

        Key::MenuWorkspaceTree => "Дерево рабочей области",
        Key::MenuOpenFile => "Открыть\u{2026}",
        Key::MenuReload => "Перезагрузить",
        Key::MenuOpenTabs => "Открытые вкладки",
        Key::MenuPinTab => "Закрепить",
        Key::MenuCloseTab => "Закрыть",
        Key::MenuRecentDocuments => "Недавние документы",

        Key::MenuSingleNewlines => "Одиночные переносы строк",
        Key::MenuNewlineDefaultMerge => "По умолчанию: Объединять в абзац",
        Key::MenuNewlineDefaultKeep => "По умолчанию: Сохранять переносы",
        Key::MenuNewlineDocDefault => "Этот документ: Как по умолчанию",
        Key::MenuNewlineDocMerge => "Этот документ: Объединять",
        Key::MenuNewlineDocKeep => "Этот документ: Сохранять",

        Key::MenuReadSource => "Просмотр исходного текста",

        Key::MenuExternalEditor => "Внешний редактор",
        Key::MenuOpenInEditor => "Открыть в редакторе",
        Key::MenuChooseEditor => "Выбрать редактор\u{2026}",

        Key::MenuSpacebarPeek => "Быстрый просмотр пробелом",
        Key::MenuPeekEnabled => "Включено",
        Key::MenuPeekTap => "Пробел: Нажатие для переключения",
        Key::MenuPeekHold => "Пробел: Удержание для просмотра",
        Key::MenuPeekMixed => "Пробел: Нажатие или удержание",
        Key::MenuPeekFocusClose => "Закрывать при потере фокуса",

        Key::MenuTextReading => "Чтение текста",
        Key::MenuFormatExt => "Формат: По расширению файла",
        Key::MenuFormatPlain => "Формат: Обычный текст",
        Key::MenuFormatMarkdown => "Формат: Markdown",
        Key::MenuParagraphsAuto => "Абзацы: Автоматически",
        Key::MenuParagraphsLines => "Абзацы: Каждая строка",
        Key::MenuParagraphsBlank => "Абзацы: Пустые строки",
        Key::MenuDetectChapters => "Определять заголовки глав",
        Key::MenuPreviousChapter => "Предыдущая глава",
        Key::MenuNextChapter => "Следующая глава",
        Key::MenuWideTableNarrow => "Широкая таблица: Меньше отступа",
        Key::MenuWideTableWiden => "Широкая таблица: Больше отступа",
        Key::MenuPreviousFile => "Предыдущий файл",
        Key::MenuNextFile => "Следующий файл",

        Key::MenuTextEncoding => "Кодировка текста",
        Key::MenuEncodingGuessed => " (определено предположительно; выберите, если неверно)",

        Key::MenuLanguage => "Язык",

        Key::TypoTitle => "Типографика",
        Key::TypoPresetName => "Имя набора",
        Key::TypoCustom => "Пользовательский",
        Key::TypoKeepKorean => "Не разбивать корейские слова",
        Key::TypoBindTxt => "Использовать этот набор для файлов TXT",
        Key::TypoFontNotice => "Используйте имена установленных шрифтов. Недостающие шрифты заменяются резервными.",
        Key::TypoEditorArgs => "Аргументы редактора ({file}, {line}, {column})",
        Key::TypoSaveApply => "Сохранить и применить",
        Key::TypoCancel => "Отмена",

        Key::TrayPeekTooltip => "Rubrica быстрый просмотр",
        Key::TrayLaunchAtSignIn => "Запуск при входе в систему",
        Key::TrayExit => "Выход",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "Не удалось запустить редактор",
        Key::ErrorCannotReadChapter => "Не удалось прочитать главу",
        Key::ErrorCannotIndexTxtChapters => "Не удалось проиндексировать главы TXT после изменения файла.",
        Key::ErrorCannotReadChangedChapter => "Не удалось прочитать измененную главу TXT",
        Key::ErrorCannotReadTxtChapter => "Не удалось прочитать главу TXT",
        Key::ErrorChooseAnotherName => "Выберите имя, отличное от Default или Book (до 80 байт).",
        Key::ErrorTooManyPresets => "Уже сохранено максимально допустимое число наборов (100).",
    }
}

fn translate_it(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "esempio",
        Key::SourceTag => "Sorgente",

        Key::FindCue => "Trova nel documento",
        Key::FindNoMatch => "nessuna corrispondenza",
        Key::FindOneMatch => "1 corrispondenza",

        Key::MenuBack => "Indietro",
        Key::MenuForward => "Avanti",
        Key::MenuCopy => "Copia",
        Key::MenuSelectAll => "Seleziona tutto",
        Key::MenuFind => "Trova nel documento",
        Key::MenuOpenLink => "Apri collegamento",
        Key::MenuCopyLink => "Copia indirizzo del collegamento",
        Key::MenuContents => "Indice",

        Key::MenuZoomIn => "Ingrandisci testo",
        Key::MenuZoomOut => "Riduci testo",
        Key::MenuZoomReset => "Dimensioni effettive",

        Key::MenuReadingMode => "Modalità lettura",
        Key::MenuContinuousScroll => "Scorrimento continuo",
        Key::MenuPageStack => "Pila di pagine",
        Key::MenuPreviousPage => "Pagina precedente",
        Key::MenuNextPage => "Pagina successiva",

        Key::MenuTypography => "Tipografia",
        Key::MenuEditSavePreset => "Modifica / Salva predefinito\u{2026}",

        Key::FaceDefault => "Predefinito",
        Key::FaceSerif => "Serif",
        Key::FaceHumanist => "Umanistico",
        Key::FaceMonospace => "Spaziatura fissa",
        Key::MeasureNarrow => "Stretto",
        Key::MeasureNormal => "Normale",
        Key::MeasureWide => "Largo",
        Key::PresetDefault => "Predefinito",
        Key::PresetBook => "Libro",

        Key::MenuLight => "Chiaro",
        Key::MenuDark => "Scuro",
        Key::MenuFollowSystem => "Segui il sistema",

        Key::MenuWorkspaceTree => "Albero dell'area di lavoro",
        Key::MenuOpenFile => "Apri\u{2026}",
        Key::MenuReload => "Ricarica",
        Key::MenuOpenTabs => "Schede aperte",
        Key::MenuPinTab => "Blocca",
        Key::MenuCloseTab => "Chiudi",
        Key::MenuRecentDocuments => "Documenti recenti",

        Key::MenuSingleNewlines => "Interruzioni di riga singole",
        Key::MenuNewlineDefaultMerge => "Predefinito: Unisci nel paragrafo",
        Key::MenuNewlineDefaultKeep => "Predefinito: Mantieni interruzioni",
        Key::MenuNewlineDocDefault => "Questo documento: Segui predefinito",
        Key::MenuNewlineDocMerge => "Questo documento: Unisci",
        Key::MenuNewlineDocKeep => "Questo documento: Mantieni",

        Key::MenuReadSource => "Visualizza sorgente",

        Key::MenuExternalEditor => "Editor esterno",
        Key::MenuOpenInEditor => "Apri nell'editor",
        Key::MenuChooseEditor => "Scegli editor\u{2026}",

        Key::MenuSpacebarPeek => "Anteprima barra spaziatrice",
        Key::MenuPeekEnabled => "Abilitato",
        Key::MenuPeekTap => "Barra spaziatrice: Tocca per alternare",
        Key::MenuPeekHold => "Barra spaziatrice: Tieni premuto per anteprima",
        Key::MenuPeekMixed => "Barra spaziatrice: Tocca o tieni premuto",
        Key::MenuPeekFocusClose => "Chiudi quando perde il focus",

        Key::MenuTextReading => "Lettura del testo",
        Key::MenuFormatExt => "Formato: Dall'estensione",
        Key::MenuFormatPlain => "Formato: Testo normale",
        Key::MenuFormatMarkdown => "Formato: Markdown",
        Key::MenuParagraphsAuto => "Paragrafi: Automatico",
        Key::MenuParagraphsLines => "Paragrafi: Ogni riga",
        Key::MenuParagraphsBlank => "Paragrafi: Righe vuote",
        Key::MenuDetectChapters => "Rileva titoli di capitolo",
        Key::MenuPreviousChapter => "Capitolo precedente",
        Key::MenuNextChapter => "Capitolo successivo",
        Key::MenuWideTableNarrow => "Tabella larga: Meno margine",
        Key::MenuWideTableWiden => "Tabella larga: Più margine",
        Key::MenuPreviousFile => "File precedente",
        Key::MenuNextFile => "File successivo",

        Key::MenuTextEncoding => "Codifica del testo",
        Key::MenuEncodingGuessed => " (rilevato per stima; selezionare se errato)",

        Key::MenuLanguage => "Lingua",

        Key::TypoTitle => "Tipografia",
        Key::TypoPresetName => "Nome predefinito",
        Key::TypoCustom => "Personalizzato",
        Key::TypoKeepKorean => "Mantieni unite le parole coreane",
        Key::TypoBindTxt => "Usa questo predefinito per i documenti TXT",
        Key::TypoFontNotice => "Usa i nomi delle famiglie di caratteri installate. I caratteri mancanti useranno quelli di fallback.",
        Key::TypoEditorArgs => "Argomenti editor ({file}, {line}, {column})",
        Key::TypoSaveApply => "Salva e applica",
        Key::TypoCancel => "Annulla",

        Key::TrayPeekTooltip => "Rubrica anteprima",
        Key::TrayLaunchAtSignIn => "Avvia all'accesso",
        Key::TrayExit => "Esci",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "Impossibile avviare l'editor",
        Key::ErrorCannotReadChapter => "Impossibile leggere il capitolo",
        Key::ErrorCannotIndexTxtChapters => "Impossibile indicizzare i capitoli TXT dopo la modifica del file.",
        Key::ErrorCannotReadChangedChapter => "Impossibile leggere il capitolo TXT modificato",
        Key::ErrorCannotReadTxtChapter => "Impossibile leggere il capitolo TXT",
        Key::ErrorChooseAnotherName => "Scegli un nome diverso da Default o Book (fino a 80 byte).",
        Key::ErrorTooManyPresets => "Ci sono già 100 predefiniti tipografici salvati.",
    }
}

fn translate_pt(key: Key) -> &'static str {
    match key {
        Key::SampleTitle => "amostra",
        Key::SourceTag => "Código-fonte",

        Key::FindCue => "Localizar no documento",
        Key::FindNoMatch => "nenhuma correspondência",
        Key::FindOneMatch => "1 correspondência",

        Key::MenuBack => "Voltar",
        Key::MenuForward => "Avançar",
        Key::MenuCopy => "Copiar",
        Key::MenuSelectAll => "Selecionar tudo",
        Key::MenuFind => "Localizar no documento",
        Key::MenuOpenLink => "Abrir link",
        Key::MenuCopyLink => "Copiar endereço do link",
        Key::MenuContents => "Conteúdo",

        Key::MenuZoomIn => "Aumentar texto",
        Key::MenuZoomOut => "Diminuir texto",
        Key::MenuZoomReset => "Tamanho real",

        Key::MenuReadingMode => "Modo de leitura",
        Key::MenuContinuousScroll => "Rolagem contínua",
        Key::MenuPageStack => "Pilha de páginas",
        Key::MenuPreviousPage => "Página anterior",
        Key::MenuNextPage => "Próxima página",

        Key::MenuTypography => "Tipografia",
        Key::MenuEditSavePreset => "Editar / Salvar predefinição\u{2026}",

        Key::FaceDefault => "Padrão",
        Key::FaceSerif => "Serif",
        Key::FaceHumanist => "Humanista",
        Key::FaceMonospace => "Monoespaçado",
        Key::MeasureNarrow => "Estreito",
        Key::MeasureNormal => "Normal",
        Key::MeasureWide => "Largo",
        Key::PresetDefault => "Padrão",
        Key::PresetBook => "Livro",

        Key::MenuLight => "Claro",
        Key::MenuDark => "Escuro",
        Key::MenuFollowSystem => "Seguir o sistema",

        Key::MenuWorkspaceTree => "Árvore do espaço de trabalho",
        Key::MenuOpenFile => "Abrir\u{2026}",
        Key::MenuReload => "Recarregar",
        Key::MenuOpenTabs => "Guias abertas",
        Key::MenuPinTab => "Fixar",
        Key::MenuCloseTab => "Fechar",
        Key::MenuRecentDocuments => "Documentos recentes",

        Key::MenuSingleNewlines => "Quebras de linha simples",
        Key::MenuNewlineDefaultMerge => "Padrão: Mesclar no parágrafo",
        Key::MenuNewlineDefaultKeep => "Padrão: Manter quebras de linha",
        Key::MenuNewlineDocDefault => "Este documento: Seguir padrão",
        Key::MenuNewlineDocMerge => "Este documento: Mesclar",
        Key::MenuNewlineDocKeep => "Este documento: Manter",

        Key::MenuReadSource => "Exibir código-fonte",

        Key::MenuExternalEditor => "Editor externo",
        Key::MenuOpenInEditor => "Abrir no editor",
        Key::MenuChooseEditor => "Escolher editor\u{2026}",

        Key::MenuSpacebarPeek => "Prévia com barra de espaço",
        Key::MenuPeekEnabled => "Ativado",
        Key::MenuPeekTap => "Barra de espaço: Toque para alternar",
        Key::MenuPeekHold => "Barra de espaço: Segure para prévia",
        Key::MenuPeekMixed => "Barra de espaço: Toque ou segure",
        Key::MenuPeekFocusClose => "Fechar ao perder o foco",

        Key::MenuTextReading => "Leitura de texto",
        Key::MenuFormatExt => "Formato: Pela extensão do arquivo",
        Key::MenuFormatPlain => "Formato: Texto sem formatação",
        Key::MenuFormatMarkdown => "Formato: Markdown",
        Key::MenuParagraphsAuto => "Parágrafos: Automático",
        Key::MenuParagraphsLines => "Parágrafos: Cada linha",
        Key::MenuParagraphsBlank => "Parágrafos: Linhas em branco",
        Key::MenuDetectChapters => "Detectar títulos de capítulos",
        Key::MenuPreviousChapter => "Capítulo anterior",
        Key::MenuNextChapter => "Próximo capítulo",
        Key::MenuWideTableNarrow => "Tabela larga: Reduzir margem",
        Key::MenuWideTableWiden => "Tabela larga: Aumentar margem",
        Key::MenuPreviousFile => "Arquivo anterior",
        Key::MenuNextFile => "Próximo arquivo",

        Key::MenuTextEncoding => "Codificação do texto",
        Key::MenuEncodingGuessed => " (detectado por estimativa; selecione se incorreto)",

        Key::MenuLanguage => "Idioma",

        Key::TypoTitle => "Tipografia",
        Key::TypoPresetName => "Nome da predefinição",
        Key::TypoCustom => "Personalizado",
        Key::TypoKeepKorean => "Manter palavras coreanas juntas",
        Key::TypoBindTxt => "Usar esta predefinição para documentos TXT",
        Key::TypoFontNotice => "Use os nomes das famílias de fontes instaladas. Fontes ausentes usarão as fontes de fallback.",
        Key::TypoEditorArgs => "Argumentos do editor ({file}, {line}, {column})",
        Key::TypoSaveApply => "Salvar e aplicar",
        Key::TypoCancel => "Cancelar",

        Key::TrayPeekTooltip => "Rubrica prévia",
        Key::TrayLaunchAtSignIn => "Iniciar no logon",
        Key::TrayExit => "Sair",

        Key::DialogTitle => "Rubrica",
        Key::ErrorCannotStartEditor => "Não é possível iniciar o editor",
        Key::ErrorCannotReadChapter => "Não é possível ler o capítulo",
        Key::ErrorCannotIndexTxtChapters => "Não é possível indexar os capítulos TXT após a alteração do arquivo.",
        Key::ErrorCannotReadChangedChapter => "Não é possível ler o capítulo TXT alterado",
        Key::ErrorCannotReadTxtChapter => "Não é possível ler o capítulo TXT",
        Key::ErrorChooseAnotherName => "Escolha um nome diferente de Default ou Book (até 80 bytes).",
        Key::ErrorTooManyPresets => "Já existem 100 predefinições tipográficas salvas.",
    }
}

pub const FONT_LABELS_EN: [&str; 19] = [
    "Latin body", "Latin headings", "Latin code",
    "Chinese body", "Chinese headings", "Chinese code",
    "Japanese body", "Japanese headings", "Japanese code",
    "Korean body", "Korean headings", "Korean code",
    "Latin emphasis", "Chinese emphasis", "Japanese emphasis", "Korean emphasis",
    "Math", "Math fallback", "Font fallback",
];

pub const FONT_LABELS_ZH_CN: [&str; 19] = [
    "西文正文", "西文标题", "西文代码",
    "中文正文", "中文标题", "中文代码",
    "日文正文", "日文标题", "日文代码",
    "韩文正文", "韩文标题", "韩文代码",
    "西文强调", "中文强调", "日文强调", "韩文强调",
    "数学公式", "数学后备", "字体后备",
];

pub const FONT_LABELS_ZH_TW: [&str; 19] = [
    "西文本文", "西文標題", "西文代碼",
    "中文本文", "中文標題", "中文代碼",
    "日文本文", "日文標題", "日文代碼",
    "韓文本文", "韓文標題", "韓文代碼",
    "西文強調", "中文強調", "日文強調", "韓文強調",
    "數學公式", "數學後備", "字型後備",
];

pub const FONT_LABELS_JA: [&str; 19] = [
    "欧文本文", "欧文見出し", "欧文コード",
    "中国語本文", "中国語見出し", "中国語コード",
    "和文本文", "和文見出し", "和文コード",
    "韓国語本文", "韓国語見出し", "韓国語コード",
    "欧文強調", "中国語強調", "和文強調", "韓国語強調",
    "数式", "数式代替", "フォント代替",
];

pub const FONT_LABELS_KO: [&str; 19] = [
    "로마자 본문", "로마자 제목", "로마자 코드",
    "중국어 본문", "중국어 제목", "중국어 코드",
    "일본어 본문", "일본어 제목", "일본어 코드",
    "한국어 본문", "한국어 제목", "한국어 코드",
    "로마자 강조", "중국어 강조", "일본어 강조", "한국어 강조",
    "수식", "수식 대체", "글꼴 대체",
];

pub const FONT_LABELS_FR: [&str; 19] = [
    "Corps latin", "Titres latins", "Code latin",
    "Corps chinois", "Titres chinois", "Code chinois",
    "Corps japonais", "Titres japonais", "Code japonais",
    "Corps coréen", "Titres coréens", "Code coréen",
    "Emphase latine", "Emphase chinoise", "Emphase japonaise", "Emphase coréenne",
    "Mathématiques", "Secours mathématiques", "Secours de police",
];

pub const FONT_LABELS_DE: [&str; 19] = [
    "Lateinischer Fließtext", "Lateinische Überschriften", "Lateinischer Code",
    "Chinesischer Fließtext", "Chinesische Überschriften", "Chinesischer Code",
    "Japanischer Fließtext", "Japanische Überschriften", "Japanischer Code",
    "Koreanischer Fließtext", "Koreanische Überschriften", "Koreanischer Code",
    "Lateinische Hervorhebung", "Chinesische Hervorhebung", "Japanische Hervorhebung", "Koreanische Hervorhebung",
    "Mathematik", "Mathematik-Ausweich", "Schrift-Ausweich",
];

pub const FONT_LABELS_ES: [&str; 19] = [
    "Cuerpo latino", "Encabezados latinos", "Código latino",
    "Cuerpo chino", "Encabezados chinos", "Código chino",
    "Cuerpo japonés", "Encabezados japoneses", "Código japonés",
    "Cuerpo coreano", "Encabezados coreanos", "Código coreano",
    "Énfasis latino", "Énfasis chino", "Énfasis japonés", "Énfasis coreano",
    "Matemáticas", "Respaldo matemáticas", "Respaldo de fuente",
];

pub const FONT_LABELS_RU: [&str; 19] = [
    "Латинский текст", "Латинские заголовки", "Латинский код",
    "Китайский текст", "Китайские заголовки", "Китайский код",
    "Японский текст", "Японские заголовки", "Японский код",
    "Корейский текст", "Корейские заголовки", "Корейский код",
    "Латинский курсив/акцент", "Китайский курсив/акцент", "Японский курсив/акцент", "Корейский курсив/акцент",
    "Математика", "Резерв математики", "Резервный шрифт",
];

pub const FONT_LABELS_IT: [&str; 19] = [
    "Testo latino", "Titoli latini", "Codice latino",
    "Testo cinese", "Titoli cinesi", "Codice cinese",
    "Testo giapponese", "Titoli giapponesi", "Codice giapponese",
    "Testo coreano", "Titoli coreani", "Codice coreano",
    "Enfasi latina", "Enfasi cinese", "Enfasi giapponese", "Enfasi coreana",
    "Matematica", "Fallback matematica", "Fallback caratteri",
];

pub const FONT_LABELS_PT: [&str; 19] = [
    "Corpo latino", "Títulos latinos", "Código latino",
    "Corpo chinês", "Títulos chineses", "Código chinês",
    "Corpo japonês", "Títulos japoneses", "Código japonês",
    "Corpo coreano", "Títulos coreanos", "Código coreano",
    "Ênfase latina", "Ênfase chinesa", "Ênfase japonesa", "Ênfase coreana",
    "Matemática", "Fallback matemática", "Fallback de fonte",
];

pub fn font_label(lang: Language, index: usize) -> &'static str {
    let list = match lang {
        Language::EnUs => &FONT_LABELS_EN,
        Language::ZhCn => &FONT_LABELS_ZH_CN,
        Language::ZhTw => &FONT_LABELS_ZH_TW,
        Language::JaJp => &FONT_LABELS_JA,
        Language::KoKr => &FONT_LABELS_KO,
        Language::FrFr => &FONT_LABELS_FR,
        Language::DeDe => &FONT_LABELS_DE,
        Language::EsEs => &FONT_LABELS_ES,
        Language::RuRu => &FONT_LABELS_RU,
        Language::ItIt => &FONT_LABELS_IT,
        Language::PtBr => &FONT_LABELS_PT,
    };
    list.get(index).copied().unwrap_or("")
}

pub const NUMBER_LABELS_EN: [&str; 17] = [
    "Size (pt)", "Latin body leading", "Asian body leading", "Tracking (em)",
    "Paragraph gap", "First indent (em)", "Column (em)", "Ragged below (em)",
    "Punctuation compression", "Hanging punctuation (em)", "Latin heading leading",
    "Asian heading leading", "Heading gap", "Code gap", "Quote indent (em)",
    "List indent (em)", "Definition indent (em)",
];

pub const NUMBER_LABELS_ZH_CN: [&str; 17] = [
    "字号 (pt)", "西文正文行距", "东亚正文行距", "字距 (em)",
    "段落间距", "首行缩进 (em)", "栏宽 (em)", "非对齐阈值 (em)",
    "标点挤压", "标点外挂 (em)", "西文标题行距",
    "东亚标题行距", "标题间距", "代码间距", "引用缩进 (em)",
    "列表缩进 (em)", "定义缩进 (em)",
];

pub const NUMBER_LABELS_ZH_TW: [&str; 17] = [
    "字級 (pt)", "西文本文行距", "東亞本文行距", "字距 (em)",
    "段落間距", "首行縮排 (em)", "欄寬 (em)", "非齊行閾值 (em)",
    "標點擠壓", "標點凸出 (em)", "西文標題行距",
    "東亞標題行距", "標題間距", "代碼間距", "引用縮排 (em)",
    "清單縮排 (em)", "定義縮排 (em)",
];

pub const NUMBER_LABELS_JA: [&str; 17] = [
    "サイズ (pt)", "欧文本文行送り", "和文本文行送り", "字送り (em)",
    "段落間隔", "字下げ (em)", "段組幅 (em)", "不揃い閾値 (em)",
    "約物詰め", "ぶら下げ (em)", "欧文見出し行送り",
    "和文見出し行送り", "見出し間隔", "コード間隔", "引用インデント (em)",
    "リストインデント (em)", "定義インデント (em)",
];

pub const NUMBER_LABELS_KO: [&str; 17] = [
    "크기 (pt)", "로마자 본문 줄간격", "동아시아 본문 줄간격", "자간 (em)",
    "단락 간격", "첫 줄 들여쓰기 (em)", "단 너비 (em)", "들쭉날쭉 기준 (em)",
    "문장부호 압축", "문장부호 돌출 (em)", "로마자 제목 줄간격",
    "동아시아 제목 줄간격", "제목 간격", "코드 간격", "인용 들여쓰기 (em)",
    "목록 들여쓰기 (em)", "정의 들여쓰기 (em)",
];

pub const NUMBER_LABELS_FR: [&str; 17] = [
    "Taille (pt)", "Interligne corps latin", "Interligne corps asiatique", "Approche (em)",
    "Espacement paragraphe", "Alinéa 1ère ligne (em)", "Colonne (em)", "Seuil justification (em)",
    "Compression ponctuation", "Ponctuation suspendue (em)", "Interligne titres latins",
    "Interligne titres asiatiques", "Espacement titre", "Espacement code", "Retrait citation (em)",
    "Retrait liste (em)", "Retrait définition (em)",
];

pub const NUMBER_LABELS_DE: [&str; 17] = [
    "Schriftgröße (pt)", "Zeilenabstand lat. Fließtext", "Zeilenabstand asiat. Fließtext", "Laufweite (em)",
    "Absatzabstand", "Erstzeileneinzug (em)", "Spaltenbreite (em)", "Flattersatz-Schwelle (em)",
    "Interpunktionskompression", "Hängende Interpunktion (em)", "Zeilenabstand lat. Überschrift",
    "Zeilenabstand asiat. Überschrift", "Überschriftabstand", "Code-Abstand", "Zitateinzug (em)",
    "Listeneinzug (em)", "Definitionseinzug (em)",
];

pub const NUMBER_LABELS_ES: [&str; 17] = [
    "Tamaño (pt)", "Interlineado cuerpo latino", "Interlineado cuerpo asiático", "Espaciado (em)",
    "Espacio de párrafo", "Sangría 1ª línea (em)", "Columna (em)", "Umbral justificación (em)",
    "Compresión de puntuación", "Puntuación colgante (em)", "Interlineado títulos latinos",
    "Interlineado títulos asiáticos", "Espacio de título", "Espacio de código", "Sangría de cita (em)",
    "Sangría de lista (em)", "Sangría de definición (em)",
];

pub const NUMBER_LABELS_RU: [&str; 17] = [
    "Размер (pt)", "Интерлиньяж латинского текста", "Интерлиньяж азиатского текста", "Трекинг (em)",
    "Отступ абзаца", "Красная строка (em)", "Колонка (em)", "Порог неровности (em)",
    "Сжатие пунктуации", "Висячая пунктуация (em)", "Интерлиньяж латинских заголовков",
    "Интерлиньяж азиатских заголовков", "Отступ заголовка", "Отступ кода", "Отступ цитаты (em)",
    "Отступ списка (em)", "Отступ определения (em)",
];

pub const NUMBER_LABELS_IT: [&str; 17] = [
    "Dimensione (pt)", "Interlinea testo latino", "Interlinea testo asiatico", "Crenatura (em)",
    "Spazio paragrafo", "Rientro prima riga (em)", "Colonna (em)", "Soglia allineamento (em)",
    "Compressione punteggiatura", "Punteggiatura sporgente (em)", "Interlinea titoli latini",
    "Interlinea titoli asiatici", "Spazio titolo", "Spazio codice", "Rientro citazione (em)",
    "Rientro elenco (em)", "Rientro definizione (em)",
];

pub const NUMBER_LABELS_PT: [&str; 17] = [
    "Tamanho (pt)", "Entrelinha corpo latino", "Entrelinha corpo asiático", "Espaçamento (em)",
    "Espaço de parágrafo", "Recuo da 1ª linha (em)", "Coluna (em)", "Limite justificação (em)",
    "Compressão de pontuação", "Pontuação suspensa (em)", "Entrelinha títulos latinos",
    "Entrelinha títulos asiáticos", "Espaço de título", "Espaço de código", "Recuo de citação (em)",
    "Recuo de lista (em)", "Recuo de definição (em)",
];

pub fn number_label(lang: Language, index: usize) -> &'static str {
    let list = match lang {
        Language::EnUs => &NUMBER_LABELS_EN,
        Language::ZhCn => &NUMBER_LABELS_ZH_CN,
        Language::ZhTw => &NUMBER_LABELS_ZH_TW,
        Language::JaJp => &NUMBER_LABELS_JA,
        Language::KoKr => &NUMBER_LABELS_KO,
        Language::FrFr => &NUMBER_LABELS_FR,
        Language::DeDe => &NUMBER_LABELS_DE,
        Language::EsEs => &NUMBER_LABELS_ES,
        Language::RuRu => &NUMBER_LABELS_RU,
        Language::ItIt => &NUMBER_LABELS_IT,
        Language::PtBr => &NUMBER_LABELS_PT,
    };
    list.get(index).copied().unwrap_or("")
}

pub fn face_label(lang: Language, index: usize) -> &'static str {
    match index {
        0 => t(lang, Key::FaceDefault),
        1 => t(lang, Key::FaceSerif),
        2 => t(lang, Key::FaceHumanist),
        3 => t(lang, Key::FaceMonospace),
        _ => "",
    }
}

pub fn measure_label(lang: Language, index: usize) -> &'static str {
    match index {
        0 => t(lang, Key::MeasureNarrow),
        1 => t(lang, Key::MeasureNormal),
        2 => t(lang, Key::MeasureWide),
        _ => "",
    }
}

pub fn preset_display_name(lang: Language, name: &str) -> String {
    match name {
        "Default" => t(lang, Key::PresetDefault).to_string(),
        "Book" => t(lang, Key::PresetBook).to_string(),
        other => other.to_string(),
    }
}

pub fn find_count(lang: Language, focus: usize, hits: usize) -> String {
    match hits {
        0 => t(lang, Key::FindNoMatch).to_string(),
        1 => t(lang, Key::FindOneMatch).to_string(),
        n => match lang {
            Language::EnUs => format!("{} of {}", focus + 1, n),
            Language::ZhCn => format!("第 {} / {} 个", focus + 1, n),
            Language::ZhTw => format!("第 {} / {} 個", focus + 1, n),
            Language::JaJp => format!("{} / {} 件", focus + 1, n),
            Language::KoKr => format!("{}/{}", focus + 1, n),
            Language::FrFr => format!("{} sur {}", focus + 1, n),
            Language::DeDe => format!("{} von {}", focus + 1, n),
            Language::EsEs => format!("{} de {}", focus + 1, n),
            Language::RuRu => format!("{} из {}", focus + 1, n),
            Language::ItIt => format!("{} di {}", focus + 1, n),
            Language::PtBr => format!("{} de {}", focus + 1, n),
        },
    }
}

pub fn error_font_required(lang: Language, role_label: &str) -> String {
    match lang {
        Language::EnUs => format!("Enter a font family for {role_label}."),
        Language::ZhCn => format!("请输入 {role_label} 的字体族名称。"),
        Language::ZhTw => format!("請輸入 {role_label} 的字型家族名稱。"),
        Language::JaJp => format!("{role_label} のフォントファミリーを入力してください。"),
        Language::KoKr => format!("{role_label}의 글꼴 패밀리를 입력하세요."),
        Language::FrFr => format!("Entrez une famille de police pour {role_label}."),
        Language::DeDe => format!("Geben Sie eine Schriftfamilie für {role_label} ein."),
        Language::EsEs => format!("Introduzca una familia de fuentes para {role_label}."),
        Language::RuRu => format!("Введите семейство шрифтов для «{role_label}»."),
        Language::ItIt => format!("Inserisci una famiglia di caratteri per {role_label}."),
        Language::PtBr => format!("Insira uma família de fontes para {role_label}."),
    }
}

pub fn error_number_between(lang: Language, label: &str, min: f32, max: f32) -> String {
    match lang {
        Language::EnUs => format!("{label} must be between {min} and {max}."),
        Language::ZhCn => format!("{label} 必须介于 {min} 和 {max} 之间。"),
        Language::ZhTw => format!("{label} 必須介於 {min} 與 {max} 之間。"),
        Language::JaJp => format!("{label} は {min} から {max} の間で指定してください。"),
        Language::KoKr => format!("{label}은(는) {min}에서 {max} 사이여야 합니다."),
        Language::FrFr => format!("{label} doit être compris entre {min} et {max}."),
        Language::DeDe => format!("{label} muss zwischen {min} und {max} liegen."),
        Language::EsEs => format!("{label} debe estar entre {min} y {max}."),
        Language::RuRu => format!("{label} должно быть от {min} до {max}."),
        Language::ItIt => format!("{label} deve essere compreso tra {min} e {max}."),
        Language::PtBr => format!("{label} deve estar entre {min} e {max}."),
    }
}

pub fn error_doc_not_found(lang: Language, path: &str) -> String {
    match lang {
        Language::EnUs => format!("{path} is no longer available; it may have been moved or deleted."),
        Language::ZhCn => format!("{path} 不再可用；它可能已被移动或删除。"),
        Language::ZhTw => format!("{path} 已無法使用；可能已移動或刪除。"),
        Language::JaJp => format!("{path} は利用できなくなりました。移動または削除された可能性があります。"),
        Language::KoKr => format!("{path}을(를) 더 이상 사용할 수 없습니다. 이동되었거나 삭제되었을 수 있습니다."),
        Language::FrFr => format!("{path} n'est plus disponible ; il a peut-être été déplacé ou supprimé."),
        Language::DeDe => format!("{path} ist nicht mehr verfügbar; die Datei wurde eventuell verschoben oder gelöscht."),
        Language::EsEs => format!("{path} ya no está disponible; es posible que se haya movido o eliminado."),
        Language::RuRu => format!("{path} больше недоступен; возможно, файл был перемещен или удален."),
        Language::ItIt => format!("{path} non è più disponibile; potrebbe essere stato spostato o eliminato."),
        Language::PtBr => format!("{path} não está mais disponível; pode ter sido movido ou excluído."),
    }
}

pub fn error_doc_permission_denied(lang: Language, path: &str) -> String {
    match lang {
        Language::EnUs => format!("Cannot read {path}: permission denied."),
        Language::ZhCn => format!("无法读取 {path}：访问被拒绝。"),
        Language::ZhTw => format!("無法讀取 {path}：權限被拒絕。"),
        Language::JaJp => format!("{path} を読み取れません: アクセスが拒否されました。"),
        Language::KoKr => format!("{path}을(를) 읽을 수 없습니다: 권한이 거부되었습니다."),
        Language::FrFr => format!("Impossible de lire {path} : accès refusé."),
        Language::DeDe => format!("{path} kann nicht gelesen werden: Zugriff verweigert."),
        Language::EsEs => format!("No se puede leer {path}: permiso denegado."),
        Language::RuRu => format!("Не удалось прочитать {path}: доступ запрещен."),
        Language::ItIt => format!("Impossibile leggere {path}: autorizzazione negata."),
        Language::PtBr => format!("Não é possível ler {path}: permissão negada."),
    }
}

pub fn error_doc_cannot_open(lang: Language, path: &str, error: &str) -> String {
    match lang {
        Language::EnUs => format!("Cannot open {path}: {error}"),
        Language::ZhCn => format!("无法打开 {path}：{error}"),
        Language::ZhTw => format!("無法開啟 {path}：{error}"),
        Language::JaJp => format!("{path} を開けません: {error}"),
        Language::KoKr => format!("{path}을(를) 열 수 없습니다: {error}"),
        Language::FrFr => format!("Impossible d'ouvrir {path} : {error}"),
        Language::DeDe => format!("{path} kann nicht geöffnet werden: {error}"),
        Language::EsEs => format!("No se puede abrir {path}: {error}"),
        Language::RuRu => format!("Не удалось открыть {path}: {error}"),
        Language::ItIt => format!("Impossibile aprire {path}: {error}"),
        Language::PtBr => format!("Não é possível abrir {path}: {error}"),
    }
}

pub fn filter_documents(lang: Language) -> String {
    let (docs, all) = match lang {
        Language::EnUs => ("Documents", "All files"),
        Language::ZhCn => ("文档", "所有文件"),
        Language::ZhTw => ("文件檔案", "所有檔案"),
        Language::JaJp => ("ドキュメント", "すべてのファイル"),
        Language::KoKr => ("문서", "모든 파일"),
        Language::FrFr => ("Documents", "Tous les fichiers"),
        Language::DeDe => ("Dokumente", "Alle Dateien"),
        Language::EsEs => ("Documentos", "Todos los archivos"),
        Language::RuRu => ("Документы", "Все файлы"),
        Language::ItIt => ("Documenti", "Tutti i file"),
        Language::PtBr => ("Documentos", "Todos os arquivos"),
    };
    format!("{docs}\0*.md;*.markdown;*.txt\0{all}\0*.*\0\0")
}

pub fn filter_applications(lang: Language) -> String {
    let (apps, all) = match lang {
        Language::EnUs => ("Applications", "All files"),
        Language::ZhCn => ("应用程序", "所有文件"),
        Language::ZhTw => ("應用程式", "所有檔案"),
        Language::JaJp => ("アプリケーション", "すべてのファイル"),
        Language::KoKr => ("응용 프로그램", "모든 파일"),
        Language::FrFr => ("Applications", "Tous les fichiers"),
        Language::DeDe => ("Anwendungen", "Alle Dateien"),
        Language::EsEs => ("Aplicaciones", "Todos los archivos"),
        Language::RuRu => ("Приложения", "Все файлы"),
        Language::ItIt => ("Applicazioni", "Tutti i file"),
        Language::PtBr => ("Aplicativos", "Todos os arquivos"),
    };
    format!("{apps}\0*.exe\0{all}\0*.*\0\0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_from_code_handles_common_tags() {
        assert_eq!(Language::from_code("en"), Some(Language::EnUs));
        assert_eq!(Language::from_code("en-US"), Some(Language::EnUs));
        assert_eq!(Language::from_code("zh-CN"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh-Hans"), Some(Language::ZhCn));
        assert_eq!(Language::from_code("zh-TW"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("zh-Hant"), Some(Language::ZhTw));
        assert_eq!(Language::from_code("ja-JP"), Some(Language::JaJp));
        assert_eq!(Language::from_code("ko-KR"), Some(Language::KoKr));
        assert_eq!(Language::from_code("fr"), Some(Language::FrFr));
        assert_eq!(Language::from_code("de"), Some(Language::DeDe));
        assert_eq!(Language::from_code("es"), Some(Language::EsEs));
        assert_eq!(Language::from_code("ru"), Some(Language::RuRu));
        assert_eq!(Language::from_code("it"), Some(Language::ItIt));
        assert_eq!(Language::from_code("pt-BR"), Some(Language::PtBr));
        assert_eq!(Language::from_code("unknown"), None);
    }

    #[test]
    fn all_languages_have_labels_and_translations() {
        for lang in Language::ALL {
            assert!(!lang.code().is_empty());
            assert!(!lang.native_name().is_empty());
            assert_eq!(Language::from_code(lang.code()), Some(lang));

            assert!(!t(lang, Key::MenuBack).is_empty());
            assert!(!t(lang, Key::MenuForward).is_empty());
            assert!(!t(lang, Key::MenuCopy).is_empty());
            assert!(!t(lang, Key::MenuFind).is_empty());
            assert!(!t(lang, Key::MenuContents).is_empty());
            assert!(!t(lang, Key::MenuReadingMode).is_empty());
            assert!(!t(lang, Key::MenuTypography).is_empty());
            assert!(!t(lang, Key::MenuLanguage).is_empty());

            assert_eq!(font_label(lang, 0).is_empty(), false);
            assert_eq!(font_label(lang, 18).is_empty(), false);
            assert_eq!(number_label(lang, 0).is_empty(), false);
            assert_eq!(number_label(lang, 16).is_empty(), false);

            assert!(!find_count(lang, 0, 0).is_empty());
            assert!(!find_count(lang, 0, 1).is_empty());
            assert!(!find_count(lang, 2, 5).is_empty());
        }
    }
}
