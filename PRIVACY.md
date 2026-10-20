# Privacy

Rubrica is a local program. It does not use the network, it does not collect telemetry,
and it does not report crashes. Everything it knows about you stays in the registry of
this one Windows account, and the only data that ever leaves the machine is what you
yourself send by opening a link.

## What Rubrica does not do

- No network request of any kind. There is no HTTP client compiled into it, and a test
  refuses any change that adds a networking dependency to the workspace.
- No telemetry, analytics, usage counting, or "anonymous statistics".
- No crash reporting. A failure is a message box on this machine and nothing else.
- No automatic update. The update check reads a file beside the program and compares two
  version numbers; the download and the install are both yours, in your browser.
- No writing to the documents you open. Rubrica opens a file, reads it, and draws it. It
  writes only where you ask it to: an export, a copied theme, a copied text selection.

## What it stores on this machine

All of it is per-user, under `HKEY_CURRENT_USER\Software\Rubrica`, and none of it is
encrypted or sent anywhere:

- Reading settings: zoom, face, measure, theme, the line-break choice, and the switches
  in the menu (including the update channel).
- Where you left each document: the file's path with the scroll and caret position, and
  the last document reopened at start, so that a restart returns you to your page.
- Your favourites and recent folders, and the folder the workspace tree was last opened
  on.
- Nothing else. The full list of values is in the source, in
  [`crates/rubrica-app/src/settings.rs`](crates/rubrica-app/src/settings.rs) and
  [`crates/rubrica-app/src/peek.rs`](crates/rubrica-app/src/peek.rs).

The window's own position and size are kept by Windows, not by Rubrica.

## What can leave the machine

Only addresses you hand to the shell yourself:

- A link in a document you click, opened in your browser or mail client.
- **Check for updates**, if a newer version is found and you accept the offer: the
  release page opens in your browser. Declining leaves the network untouched.
- **Send feedback**: the issue tracker opens in your browser. What you type there is
  between you and GitHub, under GitHub's privacy policy.
- **Privacy policy** and **Licences**: local files open if the archive shipped them; if
  this copy has none, the repository page opens instead.

In every case the request is your browser's, made because you clicked. Rubrica itself
opens no socket.

## Removing what it stored

Close Rubrica, then either delete the registry key

```
reg delete HKCU\Software\Rubrica /f
```

or run the packaged uninstaller with `-Purge`, which removes the program and the settings
together. Without `-Purge`, uninstalling removes the program, the shortcut, the `PATH`
entry and the `App Paths` key, and leaves your settings for a later reinstall.

## Contact

Questions about this document belong in the
[issue tracker](https://github.com/LiJiaHua1024/rubrica/issues).

## 摘要（中文）

- Rubrica 不联网、不上报任何数据、不做自动更新。检查更新只读取程序旁边的
  `update-channel.json` 并比较版本号；下载与安装由你在浏览器里完成。
- 它只在本机注册表 `HKEY_CURRENT_USER\Software\Rubrica` 下保存阅读设置、每篇文档
  读到的位置、收藏与最近文件夹。文档本身只读不写。
- 只有你亲手点击的链接会打开浏览器：文档里的链接、检查更新后确认打开发布页、
  反馈入口、以及随包分发的隐私页与许可证（缺少文件时打开仓库页面）。
- 要清除这些记录：关闭程序后 `reg delete HKCU\Software\Rubrica /f`，或运行卸载脚本
  并加上 `-Purge`。
