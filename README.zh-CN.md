# scrctl

[English](README.md) | [简体中文](README.zh-CN.md)

通过 USB 或 Wi-Fi，在原生桌面窗口中镜像和控制 iOS 设备。命令行与快捷键参考
scrcpy，支持视频、音频、鼠标与键盘控制、显式剪贴板粘贴，以及不重编码的录制。

scrctl 是桌面前端。[XDeviceBridge（xdb）](https://github.com/scrctl/XDeviceBridge)
在独立的本机 Server 进程中负责设备发现、配对、开发镜像（DDI）、媒体采集、
解码和录制。

## 安装

### Homebrew（macOS 和 Linux）

```sh
brew install scrctl/tap/scrctl
```

会自动安装 `xdb`。有兼容 bottle 时使用预编译包，否则构建固定版本源码。
Tap 的 bottle 与正式发布页中经 Developer ID 签名、公证的 macOS ZIP 是不同的构建。

### 正式发布包

下载 [scrctl 1.0.1](https://github.com/scrctl/scrctl/releases/tag/v1.0.1)
和对应平台的 [XDeviceBridge 运行包](https://github.com/scrctl/XDeviceBridge/releases)。

- scrctl 提供 macOS ARM64、Windows ARM64 和 Windows x64 包。
- macOS 正式 ZIP 使用 Developer ID 签名并通过 Apple 公证。
- 解压后保留 `bin`、私有库和 `share` 的完整目录结构。
- 将 `xdb` 放入 `PATH`，或用 `XDB_EXECUTABLE` 指定绝对路径。
- 按需下载对应源码包，并使用 `SHA256SUMS.txt` 校验下载文件。

DDI 按需下载，不捆绑在运行包内。

## 快速开始

开启设备的开发者模式。USB 连接时保持解锁，并确认信任提示。发现并准备设备：

```sh
scrctl --list-devices
scrctl --pair -s '<UDID>'
scrctl --install-ddi -s '<UDID>'
scrctl -s '<UDID>'
```

无线 PIN 配对要求设备与电脑在网络上可达。运行以下命令，然后在手机
**设置 → 隐私与安全性 → 开发者模式**中选择电脑，输入电脑显示的 PIN：

```sh
scrctl --pair --wifi auto
scrctl -s '<UDID>' --wifi auto
```

发现受阻时可以指定 `--wifi <地址>`，必要时用 `--wifi-port` 指定端口。
手动地址仍需要有效的配对记录。xdb 按需启动，只监听本机回环地址。

## 常用操作

```sh
scrctl --audio-buffer=200           # 增加缓冲，缓解传输抖动
scrctl --audio-dup                  # 手机与电脑同时发声
scrctl --no-audio                   # 不改变手机音频路由
scrctl --record capture.mkv         # 保存原始 HEVC + AAC-ELD 音视频
scrctl --record capture.mp4 --no-audio
scrctl --video-source screenshot    # 截图轮询，无音频及编码录制
scrctl --lang zh-CN --help
```

`MOD` 默认是左 Alt 或左 Super/Command，可用 `--shortcut-mod` 更改。

| 操作 | 快捷键 |
| --- | --- |
| 退出 / 全屏 | MOD+Q / MOD+F 或 F11 |
| 点击 / 拖动 | 鼠标左键 |
| 主屏 / 锁屏 / 音量 | MOD+H / MOD+P / MOD+↑ 或 ↓ |
| 粘贴电脑剪贴板 | MOD+V |
| 请求设备转屏 | MOD+R |
| 旋转电脑画面 | MOD+← 或 → |
| 冻结 / 刷新冻结画面 / 恢复 | MOD+Z / MOD+Z / MOD+Shift+Z |

普通按键作为物理键盘事件转发，使用设备的键盘布局和输入法。不会自动粘贴电脑
输入法的提交文字。显式粘贴支持 Unicode，可能触发 iOS 剪贴板授权。

## 兼容范围与限制

主要实测设备是运行 iOS 27.x 的 iPhone 13 mini（`iPhone14,4`）。
iPadOS 18.7.8 的 iPad mini 5 已有截图轮询与控制记录，但设备拒绝实时视频和音频。
这些观察不能证明相同系统版本的所有机型均受支持；DDI 可下载也不代表设备提供
所需的开发服务。

录制保留设备原始编码流，不重编码、不缩放、不补黑边。容器保存起录时的固定方向，
后续转屏不会动态改变该方向。切换音频路由可能让播放器暂停一次，需要在手机恢复播放。

## 文档

- [用户指南](docs/USER_GUIDE.zh-CN.md)：安装、配对、DDI、控制、音频、录制、构建及故障排查。
- [更新记录](CHANGELOG.zh-CN.md)。
- [XDB 安装及 SDK 文档](https://github.com/scrctl/XDeviceBridge)。
- 研究笔记和验收日志保留原文：[协议研究](docs/coredevice.md)、
  [重构记录](docs/REFACTOR.md)、[历史 README 与验证详情](docs/README_HISTORY.zh-CN.md)。

命令行语言仍按系统区域设置选择（`--lang auto`），可用 `--lang en` 或
`--lang zh-CN` 覆盖。文档默认使用英文。

## 从源码构建

```sh
git clone --recurse-submodules https://github.com/scrctl/scrctl.git
cd scrctl
# macOS 构建依赖：
brew install cmake sdl2-compat ffmpeg pkgconf gettext
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel 4
ctest --test-dir build --output-on-failure
./build/scrctl --help
```

scrctl 只构建固定版本的 XDB 1.1.0 客户端库，Server 需要单独安装。
将 Server 升级到 1.1.0 并重启后，才能使用原生持续发现和 DDI 改进；现有配对记录会保留。
Linux、Windows 及离线依赖设置见
[构建指南](docs/USER_GUIDE.zh-CN.md#从源码构建)。

## 许可

scrctl 使用 [Apache-2.0](LICENSE)；XDeviceBridge 是独立的 LGPL-3.0-only 项目。
依赖遵循各自许可，见[第三方声明](THIRD_PARTY_NOTICES.md)及安装包对应源码材料。
