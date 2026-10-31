# 用户指南

[English](USER_GUIDE.md) | [简体中文](USER_GUIDE.zh-CN.md) | [README](../README.zh-CN.md)

本文说明 scrctl 1.0 桌面前端与独立 XDB Server 的当前用法。
研究笔记和历史验收日志保留原文，不代替当前操作指南。

## 安装与更新

```sh
brew install scrctl/tap/scrctl
brew update
brew upgrade scrctl/tap/xdb scrctl/tap/scrctl
```

Tap 提供源码配方及已验证平台的 bottle，xdb 是 scrctl 的运行依赖。
不会安装登录服务：客户端按需启动 xdb。切换不同来源安装的 Server 前，
先运行 `xdb kill-server`；下次客户端请求会启动所选的新版本。

也可以从两个项目的 GitHub Release 下载运行包。各自保留完整目录结构，
将 `xdb` 放入 `PATH`；`XDB_EXECUTABLE` 可覆盖 Server 查找路径。
使用 `SHA256SUMS.txt` 核对下载包，对应源码归档与运行包同时提供。

macOS ARM64 正式 ZIP 使用 Developer ID 签名并通过 Apple 公证。
ZIP 和裸可执行文件无法装订票据，macOS 可能需要联网查询 Apple 的票据。
Homebrew bottle 是单独的构建，不继承该 ZIP 的公证。

Windows 的 USB 连接需要适配的 Apple Mobile Device USB 驱动，无线连接不需要
USB 驱动。Linux 的 USB 连接需要 usbmuxd 服务与设备权限，通常由发行版提供。
虚拟机必须拥有转发的 USB 设备；仅在 USB 清单中出现不代表设备协议已连通。

## 设备发现与选择

```sh
scrctl --list-devices
scrctl --list-devices --discovery-timeout=0
scrctl -s '<UDID>'
scrctl -s '<UDID>' --wifi auto
scrctl -s '<UDID>' --wifi 192.168.1.50 --wifi-port 49152
```

`-s` 选择 Apple 设备 UDID。`--wifi auto` 请求无线发现；`--wifi <地址>` 跳过发现。
一台设备可以有多个 IPv4、IPv6 和 USB 网络候选地址，不代表有多台手机或多份配对。
发现清单是快照，不保证每个地址都可达。

不显式选择时，设备必须唯一明确。USB 连接失败不会悄悄改连 Wi-Fi；
需要无线连接时请显式使用 `--wifi auto`。

## 配对与 DDI

1. 开启设备的开发者模式，按提示完成重启。
2. USB 连接时解锁设备，确认“信任此电脑”。
3. 运行 `scrctl --pair -s '<UDID>'` 建立或验证配对。
4. 运行 `scrctl --install-ddi -s '<UDID>'` 检查并准备开发镜像。
5. 运行 `scrctl -s '<UDID>'`。

首次无线 PIN 配对：

```sh
scrctl --pair --wifi auto
```

在手机的**设置 → 隐私与安全性 → 开发者模式**中选择广播出来的电脑，输入
scrctl 显示的 PIN。按设备提示解锁并授权。如果手机撤销了配对但电脑还保留记录，
重新运行 PIN 配对即可；新记录通过验证后才保存，不必先手动删除旧文件。

DDI 由 xdb 准备。合适的现有挂载会直接复用，否则下载、校验所选镜像并请求设备安装。
系统更新后缺少服务时，需要重新准备 DDI。`--replace-ddi` 显式请求替换，而非普通复用：

```sh
scrctl --install-ddi -s '<UDID>' --wifi auto
scrctl --install-ddi --replace-ddi -s '<UDID>'
```

当前目录区分 Classic（iOS/iPadOS 16 及以下）、Personalized（17–26）和
Cryptex（27）。目录中有镜像或挂载成功，都不能证明屏幕、音频或输入服务可用。
旧 Classic 系统尚无 CoreDevice 功能验收。
当前目录、身份检查、缓存、安装和 SDK 详情以
[XDB 文档](https://github.com/scrctl/XDeviceBridge)为准。
配对凭据与 DDI 缓存是用户数据，独立于程序安装目录。

## 窗口与键盘

`MOD` 默认是左 Alt 或左 Super/Command。例如 `--shortcut-mod=lalt` 只保留左 Alt
作为电脑端快捷键，使 Command 组合键可以转发到手机。

| 操作 | 输入 |
| --- | --- |
| 退出 | MOD+Q 或关闭窗口；终端按 Ctrl+C |
| 全屏 | MOD+F 或不带修饰键的 F11 |
| 点击 / 拖动 | 鼠标左键 |
| 主屏 | MOD+H 或鼠标中键 |
| 锁屏 / 侧键 | MOD+P |
| 音量 | MOD+↑ / ↓，支持转发主机重复事件以长按 |
| 恢复像素尺寸 / 去留边 | MOD+G / MOD+W |
| 旋转电脑画面 | MOD+← / → |
| 水平翻转电脑画面 | MOD+Shift+← / → |
| 垂直翻转电脑画面 | MOD+Shift+↑ / ↓ |
| 请求手机横竖屏切换 | MOD+R |
| 冻结 / 刷新冻结画面 | MOD+Z |
| 恢复实时画面 | MOD+Shift+Z |
| 粘贴电脑剪贴板 | MOD+V |

普通 Q 和 Escape 不退出。物理按键使用设备的键盘布局和输入法，不自动注入电脑
输入法提交的文字。显式粘贴支持 Unicode，但目标 App 可能请求允许从
`dtpasteboardd` 粘贴；这是 Apple 的远程剪贴板服务。
窗口失焦及几何变化会松开输入。`--no-control` 禁用设备输入。

macOS 视频窗口拖动边框时保持画面比例，转屏后同步更新比例。全屏时等比适配，
可能留有黑边；缩放事件会立即重绘当前帧，不必等待新视频帧。

设备转屏要求 App 支持旋转，不覆盖手机的方向锁。电脑端旋转、翻转只影响显示
与触摸映射，不改变设备编码录制内容。

## 音频

| 参数 | 行为 |
| --- | --- |
| 默认 | 转发到电脑，手机扬声器无声 |
| `--audio-dup` | 手机与电脑同时播放 |
| `--no-audio` | 不采集、不请求路由，保持手机原有播放状态 |
| `--no-audio-playback` | 不在电脑播放，录制音轨时仍采集 |
| `--audio-buffer=200` | 将默认 50 ms 播放缓冲增大 |

路由切换可能让播放器暂停一次；必要时在手机恢复播放，退出后也可能需要恢复。
程序不会自动按播放键或调整手机音量。手机音量可能影响捕获声音，不能靠调零音量
代替默认的电脑单端播放路由。

没有容器音轨录制时，关闭音频播放也会关闭音频采集。裸 HEVC 不含音轨。
`--audio-dup` 要求音频采集保持启用。截图轮询模式不采集音频。

## 录制与方向

```sh
scrctl -r capture.mp4
scrctl -r capture.mkv --no-audio
scrctl -r capture.mkv --audio-dup --no-audio-playback
scrctl -r capture --record-format=mkv
scrctl --no-window --no-playback -r capture.mp4
scrctl --no-window --no-video --audio-dup --no-audio-playback -r audio.mkv
scrctl -r capture.hevc
```

MP4/MKV 不重编码，保存设备原始 HEVC 与 AAC-ELD 数据包；裸 HEVC 只保存视频。
容器播放要求播放器支持对应编码和方向元数据；`scrctl --play file.hevc` 仅回放裸 HEVC。

录制保存起录时的固定设备方向。后续转屏保留编码画布，不更新方向元数据。
没有自动像素旋正、缩放、补黑边，也不支持 `--record-orientation=auto`。

```sh
scrctl -r capture.mp4 --record-orientation=270
scrctl -r capture.mp4 --display-orientation=90
scrctl -r capture.mp4 --orientation=90
```

分别只改变容器固定旋转元数据、只改变窗口，以及同时改变两者。
数字角度是顺时针 0/90/180/270 度，都不重编码。
裸 HEVC 不支持非零录制方向，录制不支持翻转；
显示单独支持 `flip0/90/180/270`。

传输中断、不支持的配置变化、磁盘错误或降级到截图模式，可能使录制报错结束。
程序不会悄悄将重建的媒体会话拼进同一个文件。使用文件前等待收尾完成，
失败时可能留下不完整文件。

## 从源码构建

使用包含子模块的 Git 工作区；Server 单独构建安装。

```sh
git clone --recurse-submodules https://github.com/scrctl/scrctl.git
cd scrctl
# macOS
brew install cmake sdl2-compat ffmpeg pkgconf gettext
# Debian / Ubuntu 改用：
# sudo apt install build-essential cmake pkg-config gettext python3 libsdl2-dev \
#   libavcodec-dev libavutil-dev libswscale-dev
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel 4
ctest --test-dir build --output-on-failure
```

Windows 使用 MSYS2 CLANG64（x64）或 CLANGARM64（ARM64）以及相应的
CMake、编译器、SDL2、FFmpeg、gettext 和 Python 包，精确配方见
[CI 文件](../.github/workflows/offline-selftest.yml)。

默认下载固定版本的缺失 JSON/CLI11 依赖。离线构建时，设置
`SCRCTL_FETCH_DEPENDENCIES=OFF`，提供系统安装包或
`FETCHCONTENT_SOURCE_DIR_NLOHMANN_JSON` / `FETCHCONTENT_SOURCE_DIR_CLI11`。
`SCRCTL_LIBAV=OFF` 关闭本地 FFmpeg 文件回放；
`SCRCTL_BUILD_TESTS=OFF` 关闭前端测试。实时解码仍由 xdb 负责。

运行包配套提供对应源码、依赖配方、补丁和校验和。请检查包内 FFmpeg 配置与许可；
“不重编码”不代表没有 FFmpeg 分发义务，也不证明已实现逐字节可复现构建。

## 故障排查

- **找不到设备：**解锁、确认信任、检查 USB 归属、驱动或 usbmuxd，或显式尝试 Wi-Fi。
  不要把删除配对记录当作第一步。
- **旧设备没有实时媒体：**尝试 `--video-source screenshot`，截图服务可用不代表实时音视频受支持。
- **更新系统后缺少服务：**检查、准备 DDI，必要时显式替换；机型与服务能力都需要确认。
- **声音短暂中断：**尝试 `--audio-buffer=200`，增加缓冲以延迟换连续性。
- **服务版本或安装来源不对：**检查 `command -v xdb`（Windows 用 `where xdb`）
  及 `XDB_EXECUTABLE`；切换安装来源前先停止旧 Server。
- **需要诊断：**使用 `scrctl --stats` 与 `scrctl --lang zh-CN --help`。

原始实验和验收细节仍保留在[协议研究](coredevice.md)、[重构记录](REFACTOR.md)及
[历史 README](README_HISTORY.zh-CN.md)，保持原始语言及各自提交对应的范围。
