use clap::Command;

pub(crate) fn localize_chinese(mut command: Command) -> Command {
    command = command
        .about("iOS 屏幕镜像与控制")
        .help_template("{about-with-newline}\n用法：{usage}\n\n选项：\n{all-args}{after-help}")
        .after_help(
            "不带参数时镜像已连接的设备。鼠标左键发送触摸，按住 Ctrl 拖动可模拟双指。\n\
             普通按键使用手机的键盘布局和输入法。MOD 默认是左 Alt 或左 Super。\n\
             MOD+Q 退出，MOD+F 或 F11 切换全屏，MOD+V 粘贴。\n\
             MOD+H 回主屏，MOD+P 锁屏，MOD+Up/Down 调节音量。\n\
             MOD+G 恢复实际尺寸，MOD+W 去除留边。\n\
             MOD+Left/Right 旋转显示，MOD+R 请求旋转设备，MOD+Shift+方向键翻转显示。\n\
             MOD+Z 暂停或刷新画面，MOD+Shift+Z 恢复播放。\n\
             默认在电脑播放音频，--audio-dup 可让手机同时播放。\n\
             无线连接需要配对。--help 和 --version 不会连接设备。",
        );
    for (id, description) in [
        ("help", "显示帮助"),
        ("version", "显示版本"),
        ("lang", "消息语言，可用 auto、en、zh-CN"),
        ("serial", "按 UDID 选择设备"),
        ("wifi", "指定局域网地址，或用 auto 自动发现已配对设备"),
        ("wifi_port", "指定手动连接的 RemotePairing 端口"),
        ("pair", "通过 USB 配对，或加 --wifi auto 通过配对码配对"),
        ("pairing_timeout", "配对超时，单位为毫秒"),
        ("repair_pairing", "允许替换不完整或失效的 USB 配对记录"),
        ("list_devices", "列出 USB 和无线设备"),
        (
            "discovery_timeout",
            "无线发现超时，单位为毫秒。0 表示只列出 USB",
        ),
        ("list_apps", "列出设备应用"),
        ("copy", "写入设备剪贴板并退出"),
        ("paste", "读取设备剪贴板"),
        ("download_ddi", "下载并验证开发者磁盘镜像"),
        ("install_ddi", "下载并挂载开发者磁盘镜像"),
        ("replace_ddi", "替换已经挂载的开发者磁盘镜像"),
        ("ddi_install_timeout", "DDI 安装超时，单位为秒"),
        ("ddi_system_version", "离线下载 DDI 时指定的系统版本"),
        ("ddi_product_type", "离线下载 DDI 时用于验证清单的机型"),
        ("ddi_directory", "DDI 缓存目录"),
        ("ddi_download_timeout", "DDI 下载超时，单位为秒"),
        ("record", "录制为 MP4、MKV 或裸 HEVC，默认按文件扩展名选择"),
        ("record_format", "覆盖录制文件格式"),
        ("record_orientation", "设置 MP4/MKV 容器顺时针旋转角度"),
        ("start_app", "启动应用。? 按名称前缀查找，+ 结束旧实例"),
        ("no_control", "关闭设备控制"),
        ("shortcut_mod", "设置快捷键修饰键，可用逗号分隔多个值"),
        ("test_button", "发送屏幕锁定、主屏、音量或静音按键"),
        ("test_touch", "发送一条归一化坐标指定的滑动后退出"),
        ("test_type", "向当前设备文本框发送 ASCII 字符"),
        ("test_degrade", "到指定秒数时交替触发截图降级与串流恢复"),
        ("stats", "每秒输出统计"),
        ("debug_input", "输出输入坐标"),
        ("debug_net", "启用网络隧道诊断"),
        ("verify", "读取指定帧的窗口内容并保存为 BMP"),
        ("window_title", "窗口标题"),
        ("window_x", "窗口横向位置，auto 表示居中"),
        ("window_y", "窗口纵向位置，auto 表示居中"),
        ("window_width", "窗口宽度，0 表示自动计算"),
        ("window_height", "窗口高度，0 表示自动计算"),
        ("window_borderless", "使用无边框窗口"),
        ("background_color", "窗口留边颜色，使用 RGB 或 RRGGBB"),
        ("always_on_top", "保持窗口置顶"),
        ("render_driver", "指定 SDL 渲染驱动"),
        ("disable_screensaver", "运行时阻止本地屏幕休眠"),
        ("fullscreen", "以全屏启动"),
        ("no_window", "无窗口运行"),
        ("scale", "指定大于零的窗口缩放倍数，默认适配屏幕"),
        ("crop", "按 WxH+X+Y 或 W:H:X:Y 裁剪显示画面"),
        ("orientation", "同时设置显示和录制方向，auto 只重置显示"),
        ("display_orientation", "设置显示方向或翻转"),
        ("no_audio", "关闭音频采集"),
        (
            "no_audio_playback",
            "关闭电脑端播放，录制 MP4/MKV 时仍可写入音频",
        ),
        ("audio_dup", "电脑转发音频时保留手机播放"),
        ("audio_buffer", "音频缓冲时长，单位为毫秒"),
        ("no_video", "关闭视频采集"),
        ("no_video_playback", "关闭视频显示，仍可录制编码视频"),
        ("no_playback", "关闭电脑端音视频播放，保留选定的录制轨道"),
        ("video_source", "选择实时视频或截图轮询"),
        ("hw_decode", "使用视频硬件解码"),
        ("no_hw_decode", "使用视频软件解码"),
        ("video_bit_rate", "请求设备编码码率上限，支持 K 或 M 后缀"),
        ("max_fps", "限制接收端输出帧率，0 表示不限"),
        ("time_limit", "限制运行秒数，0 表示不限"),
        ("exit_after", "收到指定帧数后退出，0 表示不限"),
    ] {
        command = command.mut_arg(id, |arg| arg.help(description));
    }
    command
}
