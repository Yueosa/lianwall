# LianWall 6.0.0

发布日期：2026-11-15

## 内置播放引擎

- 正式构建默认包含 Vulkan/libplacebo/FFmpeg 图片与视频引擎，daemon持有播放状态，不依赖外部播放器进程。
- 新建配置默认内置图片/视频后端、自动优先Vulkan硬解、GPU淡化；硬解首帧准备失败时自动回退软件并报告原因。也可显式选择software或vulkan。
- 支持图片↔视频、视频↔视频交叉淡化；有界缓存/解码队列、取消和首帧呈现确认，失败保留当前壁纸。
- awww、mpvpaper保留为可选后端，外部参数仍按argv元素传递；不同后端之间不保证交叉淡化。

## 管理与接口

- 新增引擎/输出发现、分页图库、收藏、成功切换次数与累计驻留统计；索引独立于缩略图缓存，采用原子保存。
- 新增只读显存命令测试、有界异步监控、服务日志保存/轮转、即时日志级别与查询；GUI可查看和导出日志。
- CLI增加engine、outputs、library、favorite、logs、vram test及对应JSON。新增engine_changed/library_changed为显式订阅，旧all事件集合与status字段保留。
- 现有Hook继续可用；本版不包含GUI规则编辑、试运行或执行历史。

## 升级与首次运行

- 请配套升级lianwall/lianwalld **6.0.0** 与lianwall-gui **2.0.0**，先正常停止旧daemon，再启动新版；仅替换GUI会连接旧服务。
- 旧配置保留现有后端；缺少backend字段仍解析为awww/mpvpaper。升级后可在GUI引擎页主动选择内置后端。旧swww_args仍按既有备份迁移规则转为awww_args。
- 新配置的目标输出留空，首次请在GUI引擎页分别选择图片/视频的单个输出，或先运行`lianwall outputs`再设置`video_engine.display`与`image_engine.outputs`。内置后端不接受空值、*或多输出列表；未配置时服务继续提供接口，但壁纸应用会返回配置错误。
- 收藏索引：`$XDG_DATA_HOME/lianwall/library.json`；日志：`$XDG_STATE_HOME/lianwall/daemon.jsonl`。视频暂停轮换不暂停播放；临时暂停只在本次daemon会话有效，间隔0持久禁用自动轮换。

## 支持范围与依赖

- 已验证作者本机Hyprland/NVIDIA、单输出、SDR；不承诺全部Linux/Wayland环境。多输出、HDR、音频和其他合成器不属于本次内置引擎支持承诺，长期稳定性继续随实际使用跟进。
- 运行需要Vulkan驱动/loader、Wayland、libplacebo、FFmpeg及Little CMS。当前x86_64预编译产物链接GLIBC≥2.39、libplacebo.so.360、FFmpeg 9（avcodec/avformat63、avutil61、swscale10）；其他ABI请从源码编译。
- 源码默认启用native-image；需要C编译器、pkg-config、wayland-scanner/protocols与对应开发头文件。根`./build.sh`生成正式产物；`LIANWALL_NATIVE_IMAGE=0 ./build.sh`生成external目录的外部专用构建，需自行选择外部后端。

## 验证

工作区与GUI回归、图片/软硬视频/跨媒体动画、失败回退、真实daemon/GUI联调及GUI退出保留服务已通过；长期运行及其他机器兼容性不作为已完成测试宣称。
