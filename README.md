# NeteaseSongRec_Linux

社区开发的 Linux 网易云听歌识曲客户端。
默认监听系统播放声音，识别成功后自动停止；支持设备切换、停止取消和复制歌曲信息。

本项目与网易云音乐官方无隶属、合作或背书关系。

## 快速上手

预编译客户端面向 **Arch Linux x86_64** 桌面环境。首次使用需要联网下载网易云识曲资源，识别歌曲时也需要联网。

### 1. 下载客户端

前往 [下载页面](https://github.com/OctoberPrayRain/NeteaseSongRec_Linux/releases)，
选择 `netease-songrec-linux-版本号-linux-x86_64.tar.gz`，解压后进入该目录。
如果暂时没有可下载的版本，可以按文末的「本地开发」运行。

### 2. 安装依赖

~~~sh
sudo pacman -S --needed nodejs python alsa-lib libpulse pipewire \
  fontconfig freetype2 gcc-libs glibc hicolor-icon-theme libglvnd \
  libx11 libxcursor libxi libxkbcommon libxkbcommon-x11 libxrandr wayland
~~~

### 3. 准备资源并启动

在解压目录中执行：

~~~sh
python share/netease-songrec-linux/scripts/sync-netease-resources.py \
  --cache-dir "$HOME/.cache/netease-songrec-linux/extension" \
  --destination share/netease-songrec-linux
./bin/netease-songrec-linux
~~~

资源准备只需完成一次，以后直接运行 `./bin/netease-songrec-linux`。

## 如何使用

1. 在电脑上播放想识别的歌曲。
2. 点击「开始识别」，保持音乐播放，等待结果。
3. 找到歌曲后会自动停止，点击「复制结果」即可复制歌曲信息。
4. 想识别另一首歌时，点击「再识别一首」。

识别过程中可以随时点击「停止识别」，取消本次识别。
默认监听系统播放声音；如果没有检测到声音，可以在声音来源菜单中选择当前播放设备对应的
**Monitor of …**。使用耳机、音箱或切换输出设备后，也可以在这里调整。

## 目录与运行配置

### 源码目录

| 目录或文件 | 内容 |
| --- | --- |
| `src/core/` | 音频采集、指纹生成与歌曲识别 |
| `src/main.rs` | 桌面程序入口与界面交互 |
| `ui/main.slint` | 页面布局、样式与动效 |
| `assets/` | 识曲脚本、指纹引擎和界面图标 |
| `vendor/netease-extension/` | 下载或导入的网易云插件原始资源 |
| `scripts/` | 资源同步与软件打包脚本 |
| `packaging/` | 桌面入口与安装包配置 |
| `Cargo.toml`、`Cargo.lock`、`build.rs` | Rust 依赖与界面构建配置 |

### 可选配置

通常无需手动配置。需要调整时，可在启动前设置以下环境变量：

| 环境变量 | 用途 |
| --- | --- |
| `NETEASE_SONGREC_NODE` | 指定 Node.js 可执行文件，默认使用 `node` |
| `NETEASE_SONGREC_DATA_DIR` | 指定包含 `netease/` 和 `ui/` 的资源目录 |
| `NETEASE_COOKIE` | 按需提供网易云登录态 |

## 许可证、来源与数据处理

原创代码采用 [AGPL-3.0-only](LICENSE)；复用的 SongRec 音频代码保留 GPL-3.0-or-later 许可。
第三方组件的来源及许可说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

网易云指纹引擎、脚本和官方图标归原权利人所有，不纳入本项目 AGPL 授权范围。
资源来自 [网易云插件下载页](https://fn.music.163.com/g/chrome-extension-home-page-beta/)
与 [Chrome 商店](https://chromewebstore.google.com/detail/kemcalcncfhmdkgglekijclbomdoohkp)，由资源同步脚本单独获取。

音频在本机生成指纹，识别时向网易云发送指纹及片段时长等参数。
程序不上传原始音频，也不将录音片段保存到磁盘。识别服务的可用性取决于网易云接口。

## 本地开发

技术栈：Rust、Slint、Node.js。

在 Arch Linux 上安装构建依赖：

~~~sh
sudo pacman -S --needed base-devel git rust clang pkgconf python nodejs \
  alsa-lib libpulse pipewire fontconfig freetype2 libxkbcommon \
  libxkbcommon-x11 libx11 libxcursor libxi libxrandr libglvnd wayland
~~~

获取源码、准备资源并启动：

~~~sh
git clone https://github.com/OctoberPrayRain/NeteaseSongRec_Linux.git
cd NeteaseSongRec_Linux
python scripts/sync-netease-resources.py
cargo run --locked
~~~

已有网易云插件 **1.0.4** 的解压目录时，可以直接导入：

~~~sh
python scripts/sync-netease-resources.py --from-dir /path/to/build
~~~
