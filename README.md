# NeteaseSongRec_Linux

社区开发的 Linux 网易云听歌识曲客户端，使用 Rust、Slint 和 Node.js。
默认监听系统播放声音，识别成功后自动停止；支持设备切换、停止取消和复制歌曲信息。

本项目与网易云音乐官方无隶属、合作或背书关系。

## 本地开发

Arch Linux 构建依赖：

~~~sh
sudo pacman -S --needed base-devel rust clang pkgconf python nodejs \
  alsa-lib libpulse pipewire fontconfig freetype2 libxkbcommon \
  libxkbcommon-x11 libx11 libxcursor libxi libxrandr libglvnd wayland
~~~

拉取并同步官方资源，然后启动：

~~~sh
python scripts/sync-netease-resources.py
cargo run --locked
~~~

脚本固定使用已验证的网易云插件 **1.0.4**，核对版本和文件内容后再同步：

~~~text
官方 Chrome 商店
    ↓ 下载、校对
vendor/netease-extension/            官方原件，Git 忽略
    ↓ 同步
assets/netease/afp.wasm              运行副本，Git 忽略
assets/netease/sandbox.bundle.js     运行副本，Git 忽略
assets/ui/recognition-128.png        运行副本，Git 忽略
~~~

~~~sh
# 已有官方插件的解压目录时，直接导入
python scripts/sync-netease-resources.py --from-dir /path/to/build

# 重新从官方商店下载
python scripts/sync-netease-resources.py --refresh
~~~

没有官方资源也能编译页面，识曲需要先完成同步。
仓库内的 fingerprint_bridge.js 是本项目维护的桥接代码，不属于官方原件。
官方图标在运行时读取；可执行文件仅内置通用麦克风备用图标。

## CI、发布与 AUR

GitHub Actions 在推送 main、创建版本标签或手动触发时，在 Arch Linux 中编译 x86_64 发布版。
CI 不下载或打包网易云原件。每次构建生成：

- 客户端压缩包：可执行文件、Node.js 桥接脚本、桌面入口和许可证。
- 对应源码压缩包：本项目源码和固定版本的 Rust 依赖源码，供离线重建及保留第三方许可。
- AUR 提交压缩包：PKGBUILD、.SRCINFO 和打包脚本许可证。
- SHA256SUMS。

推送与 Cargo.toml 版本一致的标签（例如 v0.1.0）后，CI 会将产物发布至 GitHub Release。
普通 main 推送的产物在对应 Actions 运行的 Artifacts 中。

AUR 的包名为 **netease-songrec-linux-bin**。AUR 托管打包脚本；
其 prepare() 从官方商店下载的插件中提取资源，用同一个同步脚本放入安装目录。
用户通过 AUR 构建安装后即可识曲，无需手动复制资源。
维护者需将发布产物中的 PKGBUILD、.SRCINFO、LICENSE 提交到 AUR。

~~~sh
# 本地检查打包流程；正式发布使用 release 二进制
cargo build --release --locked
python scripts/package-release.py
~~~

直接解压客户端压缩包使用时，在解压目录中先同步资源：

~~~sh
python share/netease-songrec-linux/scripts/sync-netease-resources.py \
  --cache-dir "$HOME/.cache/netease-songrec-linux/extension" \
  --destination share/netease-songrec-linux
./bin/netease-songrec-linux
~~~

## 目录与运行配置

- src/core/：音频采集、指纹、网易云请求和会话控制。
- ui/main.slint：页面；src/main.rs：页面与识曲流程的连接。
- scripts/：官方资源同步和发布打包。
- packaging/：桌面入口与 AUR 模板。
- assets/：本项目资源及本地生成的官方资源副本。
- build.rs、Cargo.toml、Cargo.lock：构建入口与固定依赖版本，必须提交。
- target/、CHANGELOG.md、vendor/netease-extension/：本地文件，Git 忽略。

程序依次查找 NETEASE_SONGREC_DATA_DIR、可执行文件旁的 assets、
安装前缀下的 share/netease-songrec-linux，最后使用开发目录的 assets。
Node.js 默认使用 node，可通过 NETEASE_SONGREC_NODE 指定其他路径。
需要登录态时可设置 NETEASE_COOKIE；程序不读取浏览器配置。

## 许可证、来源与数据处理

原创代码采用 AGPL-3.0-only；复用的 SongRec 音频代码保留 GPL-3.0-or-later 许可。
SongRec 的来源、修改范围及 Slint 等第三方组件说明见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

网易云指纹引擎、脚本和官方图标归原权利人所有，不纳入本项目 AGPL 授权范围。
官方来源：[网易云插件下载页](https://fn.music.163.com/g/chrome-extension-home-page-beta/)、
[Chrome 商店](https://chromewebstore.google.com/detail/kemcalcncfhmdkgglekijclbomdoohkp)。
公开下载不被本项目表述为额外的修改或再分发授权。

音频以六秒、单声道 48 kHz 在内存中采集，在本机重采样至 8 kHz 后生成指纹。
识别请求将指纹及片段时长等参数发送至网易云接口，不上传原始音频，
也不将采集片段保存到磁盘。识别服务的可用性取决于网易云接口。
