# Scrctl CLI 图标

SDL 窗口使用编译时内嵌的 `scrctl-icon-1024.png`。可编辑原稿和 SVG 位于 [scrctl/Design 的 cli-icon 目录](https://github.com/scrctl/Design/tree/main/cli-icon)。

在本仓库根目录执行以下命令，从相邻的 Design 仓库更新图标，再重新编译 CLI。

```sh
vectorcraft-cli convert ../Design/cli-icon/AppIcon.vectorcraft assets/icon/scrctl-icon-1024.png
```
