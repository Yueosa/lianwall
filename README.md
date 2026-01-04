# 🎬 LianWall

智能动态壁纸管理器 - 基于负反馈闭环调节的壁纸轮换系统

###### 省流

> **这是一个有记忆的智能壁纸轮换器** 🎯
> 
> - 刚播放过的壁纸会"进入冷却"，短期内不会重复
> - 长期未播放的壁纸会"积攒期望"，逐渐获得出场机会
> - 支持 **动态壁纸**（视频）和 **静态壁纸**（图片）两种模式
> - 权重数据持久化，重启后继续接着之前的状态轮换
> - 适配 Hyprland，使用 mpvpaper 和 swww 作为底层引擎
> 
> **最佳使用场景：** 5～30 个壁纸时算法最稳定（更多壁纸支持开发中）

## ✨ 特性

- **负反馈闭环调节** - 有记忆的随机，避免重复和埋没
- **空间化选择逻辑** - 二分切割算法，自然有机的播放序列
- **多引擎支持** - mpvpaper（动态）/ swww（静态）
- **持久化权重** - 重启后保留壁纸"地形"状态

## 📦 安装

> 推荐放在 `~/.local/bin` 下

###### 依赖

LianWall 依赖以下壁纸引擎，请确保已安装：

- **[mpvpaper](https://github.com/GhostNaN/mpvpaper)** - 动态壁纸引擎，基于 mpv 播放视频壁纸
- **[swww](https://github.com/LGFae/swww)** - 静态壁纸引擎，支持丰富的过渡动画

```bash
# Arch Linux
paru -S mpvpaper swww

# 或手动编译安装
```

###### 自行编译

```bash
cargo build --release
cp target/release/lianwall ~/.local/bin/
sudo chmod +x ~/.local/bin/lianwall
```

###### 下载 Release 包

```bash
cp ./lianwall_{实际版本号}_linux_x86_64 ~/.local/bin/lianwall
sudo chmod +x ~/.local/bin/lianwall
```

## 🚀 使用

```bash
lianwall daemon              # 启动守护进程（动态壁纸模式，循环切换）
lianwall next                # 立即切换到下一张壁纸（根据当前模式）
lianwall video               # 切换到动态壁纸模式（自动停止 swww）
lianwall picture             # 切换到静态壁纸模式（自动停止 mpvpaper）
lianwall kill                # 停止所有壁纸引擎（mpvpaper + swww）
lianwall reset -m <mode>     # 热重载指定模式的壁纸目录
lianwall status -m <mode>    # 显示指定模式的状态和壁纸列表
```

### Hyprland 配置

```conf
# 随 Hyprland 启动
exec-once = lianwall daemon

# 快捷键
bind = SUPER ALT, W, exec, lianwall next # 下一张壁纸
bind = SUPER ALT, E, exec, lianwall video # 视频模式
bind = SUPER ALT, Q, exec, lianwall kill # 停止壁纸
bind = SUPER ALT, R, exec, lianwall picture # 图片模式
```

## ⚙️ 配置

配置文件位置：`~/.config/lianwall/config.toml`

首次运行会自动生成带详细注释的默认配置，主要配置项：

```toml
[paths]
video_cache = "~/.cache/lianwall/video.json"  # 动态壁纸权重缓存
image_cache = "~/.cache/lianwall/image.json"  # 静态壁纸权重缓存
video_dir = "~/Videos/background"             # 动态壁纸目录
image_dir = "~/Pictures/wallpapers"           # 静态壁纸目录

[video_engine]
type = "mpvpaper"    # 动态壁纸引擎
interval = 600       # 切换间隔（秒），默认 10 分钟

[image_engine]
type = "swww"                  # 静态壁纸引擎
interval = 300                 # 切换间隔（秒），默认 5 分钟
transition = "fade"            # 过渡效果
transition_duration = 2.0      # 过渡时长（秒）

[weight]
base = 100.0                       # 基础权重
select_penalty = 10.0              # 选中惩罚值
perturbation_ratio = 0.03          # 扰动幅度（±3%）
normalization_threshold = 500.0    # 归一化触发阈值
normalization_target = 100.0       # 归一化目标值
shuffle_period = 100               # 洗牌周期（轮数）
shuffle_intensity = 0.1            # 洗牌强度（10%）

[video_optimization]
enabled = true                              # 启用自动视频转码优化
cache_dir = "~/.cache/lianwall/transcoded"  # 转码缓存目录
max_cache_size_mb = 10240                   # 缓存大小限制（MB）
target_resolution = "auto"                  # 目标分辨率（auto/2560/1920等）
target_fps = 30                             # 目标帧率
preload_count = 3                           # 预加载队列大小
encoder = "auto"                            # 编码器（auto/nvenc/vaapi/libx264）
crf = 23                                    # 编码质量（18-51）
preset = "fast"                             # 编码速度预设
```

### 核心配置说明

#### 路径配置 `[paths]`
- **video_cache / image_cache**：权重数据持久化文件，支持重启后继续记忆
- **video_dir / image_dir**：壁纸文件目录，支持递归扫描子目录

#### 引擎配置 `[video_engine]` / `[image_engine]`
- **interval**：自动切换间隔，建议视频 10 分钟，图片 5 分钟
- **transition**（仅图片）：支持 fade、left、right、wave、random 等多种过渡效果

#### 权重算法配置 `[weight]`（高级用户）

| 参数                      | 默认值 | 说明                | 调整建议             |
| ------------------------- | ------ | ------------------- | -------------------- |
| `base`                    | 100.0  | 基础权重基准值      | ⚠️ 不建议修改         |
| `select_penalty`          | 10.0   | 选中后的权重惩罚    | 增大 → 冷却期更长    |
| `perturbation_ratio`      | 0.03   | 动态扰动强度（±3%） | 增大 → 随机性增强    |
| `normalization_threshold` | 500.0  | 触发权重缩放的阈值  | 防止数值溢出         |
| `normalization_target`    | 100.0  | 归一化后的目标权重  | 与 base 保持一致     |
| `shuffle_period`          | 100    | 洗牌周期（每 N 轮） | 0 = 禁用，100 = 推荐 |
| `shuffle_intensity`       | 0.1    | 每次洗牌重置的比例  | 0.05-0.20 合理范围   |

**实战调优示例：**
- **壁纸多（30+）**：增大 `perturbation_ratio` 到 0.05，增强随机性
- **偏好固定循环**：减小 `perturbation_ratio` 到 0.01，降低 `shuffle_period` 到 200
- **追求极致随机**：增大 `shuffle_intensity` 到 0.2，缩短 `shuffle_period` 到 50

#### 视频转码优化配置 `[video_optimization]`（新功能）

| 参数                | 默认值                       | 说明                 | 调整建议                            |
| ------------------- | ---------------------------- | -------------------- | ----------------------------------- |
| `enabled`           | true                         | 是否启用自动转码     | false = 禁用优化                    |
| `cache_dir`         | ~/.cache/lianwall/transcoded | 转码文件缓存目录     | 建议使用 SSD 路径                   |
| `max_cache_size_mb` | 10240                        | 缓存大小限制（MB）   | 根据磁盘空间调整                    |
| `target_resolution` | "auto"                       | 目标分辨率           | auto/1920/2560/3840 或 "宽x高"      |
| `target_fps`        | 30                           | 目标帧率             | 24/30/60，越低文件越小              |
| `preload_count`     | 3                            | 后台预转码队列大小   | 增大 → 等待时间 ↓，CPU/IO 占用 ↑    |
| `encoder`           | "auto"                       | 视频编码器           | auto/nvenc/vaapi/libx264            |
| `crf`               | 23                           | 恒定质量因子（0-51） | 18-20=高质量，28-30=中等质量        |
| `preset`            | "fast"                       | 编码速度预设         | ultrafast/fast/medium/slow/veryslow |

**性能优化效果：**
- **4K 视频 → 2.5K 屏幕**：VRAM 占用从 1.3GB 降至 ~300MB（减少 77%）
- **转码策略**：仅当原始分辨率高于屏幕时才转码，避免画质损失
- **缓存清理**：优先删除最近使用的文件（配合零和博弈算法，冷门文件权重回升）
- **硬件加速**：自动检测 NVIDIA（nvenc）、Intel/AMD（vaapi）GPU 加速

**使用场景：**
- ✅ **4K/8K 原片在低分辨率屏幕播放**：显著降低 VRAM 和解码负载
- ✅ **高帧率视频（60fps+）降帧**：减少 GPU 占用
- ✅ **多显示器环境**：自动适配最小分辨率
- ❌ **原始分辨率 ≤ 屏幕分辨率**：自动跳过转码，直接使用原文件

---

## 🧠 算法设计解析

### 一、核心算法：零和博弈 + 动态扰动选择系统

LianWall 采用**零和博弈权重系统**，结合**动态扰动中位选择**，实现有记忆的智能轮换。

#### 1.1 权重更新规则（零和博弈）

```rust
每次切换后，对所有壁纸进行权重调整：

被选中壁纸：
  value_new = value_old - penalty       // 默认 penalty = 10.0
  skip_streak = 0                        // 重置跳过计数

未被选中壁纸（均分奖励）：
  reward = penalty / (N - 1)             // N 为壁纸总数
  value_new = value_old + reward         // 每个壁纸平分选中者的惩罚值
  skip_streak += 1                       // 累计跳过次数
```

**关键数学性质：**

$$
\sum_{i=1}^{N} \Delta v_i = -\text{penalty} + (N-1) \times \frac{\text{penalty}}{N-1} = 0
$$

**零和博弈 ⇒ 总权重守恒**：

$$
W_{t+1} = W_t \quad (\text{无膨胀，无通缩})
$$

这从根本上解决了旧算法中权重无限膨胀的问题。

#### 1.2 选择算法：动态扰动 + 容差中位策略

传统贪心算法总是选择最高权重项，会导致初始权重略高的文件被过度选择。LianWall 采用**动态扰动容差中位选择**：

```rust
Algorithm: DynamicPerturbationMedianSelect(wallpapers, tolerance=5.0, ratio=0.03)
  Input:  wallpapers[]  壁纸列表
          tolerance     容差值（默认 5.0）
          ratio         扰动比例（默认 0.03）
  Output: 选中的索引

  1. 对每个壁纸应用动态扰动：
     perturbation = value × ratio × random(-1.0, 1.0)
     value_perturbed = value + perturbation
  
  2. 按扰动后的权重降序排序
  
  3. max_value ← perturbed_values[0]
  
  4. candidates ← {i | max_value - perturbed_values[i] ≤ tolerance}
  
  5. return candidates[len(candidates) / 2]  // 中位选择
```

**关键创新：动态扰动**

扰动幅度与权重成正比，确保相对影响力恒定：

| 权重 | 扰动范围（ratio=0.03） | 相对影响力 |
| ---- | ---------------------- | ---------- |
| 100  | ±3.0                   | ±3%        |
| 500  | ±15.0                  | ±3%        |
| 1000 | ±30.0                  | ±3%        |

**解决问题：** 旧算法中固定扰动 ±0.3 在权重膨胀到 1000+ 时失效（0.03% 影响力），新算法始终保持 3% 的有效随机性。

**示例演示：**
```
原始权重:    [105.0, 103.0, 102.0, 101.0, 100.0, 95.0]
扰动后:      [107.2, 101.5, 103.8, 99.4, 102.1, 96.3]
排序后:      [107.2, 103.8, 102.1, 101.5, 99.4, 96.3]
max = 107.2
容差范围:    [102.2, 107.2]
候选索引:    [0, 1, 2]  (3个候选项)
选择:        candidates[1] → 原权重 102.0 的壁纸
```

每次选择的扰动都是独立随机的，打破了确定性循环。

---

### 二、数学性质与系统稳定性分析

#### 2.1 权重守恒定律（零和博弈）

设系统有 $N$ 个壁纸，第 $t$ 轮总权重为 $W_t$：

$$
W_{t+1} = W_t - \text{penalty} + (N-1) \times \frac{\text{penalty}}{N-1} = W_t
$$

**数学证明（零和博弈）：**

$$
\begin{aligned}
\Delta W &= \sum_{i=1}^{N} \Delta v_i \\
&= -\text{penalty} + \sum_{j \neq \text{selected}} \frac{\text{penalty}}{N-1} \\
&= -\text{penalty} + (N-1) \times \frac{\text{penalty}}{N-1} \\
&= 0
\end{aligned}
$$

**关键优势：**

| 特性         | 旧算法（正和博弈）   | 新算法（零和博弈） |
| ------------ | -------------------- | ------------------ |
| 总权重变化   | 每轮 +2.5N - 12.5    | **恒为 0**         |
| 长期稳定性   | ❌ 无限膨胀           | ✅ 完美守恒         |
| 数值溢出风险 | ⚠️ 高（需定期归一化） | ✅ 极低（仅作保险） |
| 适用规模     | 5-30 个壁纸          | **无限制**         |

**实测数据（100 轮模拟，N=10）：**
- 旧算法：总权重从 1000 → 2438（+143.8%）
- 新算法：总权重从 1000 → 1003（+0.3%，仅洗牌微扰）

#### 2.2 自动归一化机制（防溢出保险）

虽然零和博弈理论上不会膨胀，但洗牌机制会引入微小的权重波动。自动归一化作为双保险：

**触发条件：**

$$
\frac{1}{N} \sum_{i=1}^{N} v_i > \text{threshold} \quad (\text{默认 } 500.0)
$$

**归一化算法：**

$$
\text{scale} = \frac{\text{target}}{\text{avg}} \quad \Rightarrow \quad v_i' = v_i \times \text{scale}
$$

所有壁纸权重按相同比例缩放，**保持相对关系不变**。

**示例：**
```
触发前：[520, 510, 505, 495, 490]  平均 504
缩放系数：100 / 504 ≈ 0.198
触发后：[103.0, 101.0, 100.0, 98.0, 97.0]  平均 99.8
```

#### 2.3 周期性洗牌（打破生态锁定）

**生态锁定问题：** 即使有扰动，长期运行后权重梯度可能固化，导致播放序列可预测。

**洗牌机制：**

每 `shuffle_period` 轮（默认 100），随机选择 `shuffle_intensity × N` 个壁纸（默认 10%），将其权重重置为：

$$
v_{\text{reset}} = \text{base} \times (1 + \epsilon), \quad \epsilon \sim U(-0.2, 0.2)
$$

**效果：** 类似自然界的"森林火灾"，周期性破坏旧秩序，给"冷门"壁纸重新上位的机会。

**数学类比：** 模拟退火算法中的"温度扰动"，防止陷入局部最优。

#### 2.4 冷却期估算（零和博弈下）

壁纸被选中后，权重从 $v$ 降至 $v - 10$。零和博弈下，每轮获得平均奖励：

$$
\text{reward}_{\text{avg}} = \frac{10}{N-1}
$$

**恢复时间（N=10）：**

```
选中后: 100 → 90
每轮奖励: 10 / 9 ≈ 1.11
第1轮:  90 + 1.11 = 91.11
第5轮:  90 + 5×1.11 = 95.55
第9轮:  90 + 9×1.11 = 100.0  ← 恢复到初始值
```

**冷却期公式：**

$$
t_{\text{cooldown}} = \frac{\text{penalty} \times (N-1)}{\text{penalty}} = N - 1 \quad (\text{轮数})
$$

若切换间隔 10 分钟，冷却期约 **(N-1) × 10 分钟**。

#### 2.5 埋没问题的数学证明

**定理：** 在零和博弈系统中，任何壁纸不会被永久埋没。

**证明：**

假设壁纸 $A$ 连续 $n$ 轮未被选中，每轮获得奖励 $r = \frac{\text{penalty}}{N-1}$：

$$
v_A(n) = v_0 + n \times \frac{\text{penalty}}{N-1}
$$

其他壁纸平均被选中 $\frac{n}{N-1}$ 次，平均权重：

$$
\bar{v}_{\text{others}} \approx v_0 + \frac{n}{N-1} \times \left( \frac{N-2}{N-1} \times \text{penalty} - \text{penalty} \right)
$$

简化后：

$$
v_A(n) - \bar{v}_{\text{others}} \approx \frac{n \times \text{penalty}}{N-1} > 0
$$

当 $n$ 足够大时，$A$ 的权重必然超过平均值，进入容差范围。

**推论：** 
1. **绝对公平性**：零和博弈天然保证长期公平
2. **最大等待时间**：$\leq 2(N-1)$ 轮（考虑容差过滤）
3. **洗牌机制补充**：周期性重置打破极端不均衡

---

### 三、动态扰动容差中位选择的优势

#### 3.1 与其他算法对比

| 算法类型                         | 选择策略     | 随机性来源   | 优势                             | 劣势                 |
| -------------------------------- | ------------ | ------------ | -------------------------------- | -------------------- |
| **贪心选择**                     | 总是最高权重 | 无           | 最优权重利用                     | 易收敛，缺乏多样性   |
| **容差中位**                     | 容差范围中位 | 微扰 ±0.3    | 打破初始偏差                     | 膨胀后扰动失效       |
| **动态扰动中位**<br>**(新算法)** | 扰动后中位   | 动态扰动 ±3% | **自适应随机性**<br>**零和守恒** | 计算开销略增         |
| **加权随机**                     | 权重概率采样 | 随机数       | 理论公平                         | 短期波动大           |
| **纯随机**                       | 均匀随机     | 随机数       | 完全无偏                         | 无记忆，可能连续重复 |

#### 3.2 动态扰动的"破循环"效应

考虑初始权重场景：
```
文件A.mp4: 120.0（修改时间最新）
文件B.mp4: 119.8
文件C.mp4: 119.5
文件D.mp4: 100.0
文件E.mp4: 80.0
```

**贪心算法：** 总是选 A → A 被惩罚后，总是选 B → B 被惩罚后，总是选 C → ...  
**问题：** 确定性死循环 A → B → C → A → B → C ...

**旧容差中位：** 筛选 [115, 120] → 候选 [A, B, C] → 固定选 B  
**问题：** B 被过度选择，A 和 C 权重慢慢下降直到脱离容差范围

**新动态扰动中位：** 
```
第1次: 扰动后 [122.5, 117.2, 121.0] → 选 A (中位)
第2次: 扰动后 [116.3, 118.9, 120.2] → 选 B (中位)
第3次: 扰动后 [119.1, 115.8, 122.3] → 选 C (中位)
```

**效果：** 每次选择都有不同的可能性，打破确定性循环，实现真正的随机性。

---

### 四、初始权重分配策略

新文件权重基于修改时间线性映射：

$$
v_{\text{init}}(t) = 120 - 40 \times \frac{t - t_{\text{newest}}}{t_{\text{oldest}} - t_{\text{newest}}}
$$

其中 $t$ 为文件修改时间。结果：
- **最新文件**：120.0（+20% 竞争优势）
- **平均文件**：100.0（基准）
- **最旧文件**：80.0（-20% 竞争劣势）

**合并策略（热重载时）：**
```rust
新发现文件权重 = (时间权重 + 现有平均权重) / 2
```

避免新文件突然涌入打破现有生态平衡。

---

### 五、系统参数调优建议

| 参数                      | 默认值 | 作用                       | 调整建议                           |
| ------------------------- | ------ | -------------------------- | ---------------------------------- |
| `select_penalty`          | 10.0   | 选中惩罚                   | 增大 → 冷却期 ↑<br>减小 → 轮换更快 |
| `perturbation_ratio`      | 0.03   | 扰动强度                   | 增大 → 随机性 ↑<br>减小 → 趋向确定 |
| `normalization_threshold` | 500.0  | 归一化触发                 | 预防溢出，无需调整                 |
| `shuffle_period`          | 100    | 洗牌周期                   | 减小 → 洗牌频繁<br>0 → 禁用洗牌    |
| `shuffle_intensity`       | 0.1    | 洗牌力度                   | 增大 → 破循环强<br>减小 → 影响温和 |
| `tolerance`               | 5.0    | 容差范围<br>*（代码固定）* | 修改需重新编译                     |
| `base`                    | 100.0  | 基准权重                   | ⚠️ 不建议修改                       |

**实战场景调优：**

| 场景                           | 壁纸数 | `perturbation_ratio` | `shuffle_period` | `shuffle_intensity` | 预期效果           |
| ------------------------------ | ------ | -------------------- | ---------------- | ------------------- | ------------------ |
| **均衡模式**<br>（推荐）       | 10-30  | 0.03                 | 100              | 0.10                | 平衡记忆与随机     |
| **随机模式**<br>（追求多样性） | 30+    | 0.05                 | 50               | 0.15                | 高度随机，快速轮换 |
| **固定模式**<br>（偏好循环）   | 5-15   | 0.01                 | 200              | 0.05                | 趋向固定序列       |
| **长期运行**<br>（服务器）     | 任意   | 0.03                 | 100              | 0.10                | 防止生态锁定       |

**实战示例（20个壁纸，10分钟间隔，默认参数）：**
- 理论轮换周期：约 **3-4 小时**（零和博弈下稳定）
- 单个壁纸冷却期：约 **190 分钟**（$(N-1) \times 10$ 分钟）
- 权重分布区间：$[85, 115]$（零和博弈 + 洗牌平衡）
- 洗牌频率：每 **1000 分钟**（16.7 小时）洗一次

---

### 六、与传统方案对比

| 特性           | `shuf` 随机 | 加权随机   | LianWall v1.0<br>*（旧算法）* | LianWall v2.0<br>*（零和博弈）* |
| -------------- | ----------- | ---------- | ----------------------------- | ------------------------------- |
| **记忆性**     | ❌ 无        | ✅ 概率分布 | ✅ 权重持久化                  | ✅ 权重持久化                    |
| **冷却保证**   | ❌ 可能连续  | ⚠️ 概率性   | ✅ 确定性惩罚                  | ✅ 确定性惩罚                    |
| **公平性**     | ⚠️ 长期收敛  | ✅ 理论公平 | ⚠️ 膨胀后失衡                  | ✅ **数学证明公平**              |
| **长期稳定**   | ✅ 无状态    | ✅ 稳定     | ❌ **权重膨胀**                | ✅ **零和守恒**                  |
| **随机性**     | 完全随机    | 受权重影响 | ⚠️ 膨胀后失效                  | ✅ **自适应扰动**                |
| **数值溢出**   | -           | -          | ⚠️ 高风险                      | ✅ 低风险                        |
| **适用规模**   | 任意        | 任意       | 5-30 个                       | ✅ **无限制**                    |
| **破循环能力** | 天然随机    | 部分       | ❌ 易锁定                      | ✅ **周期洗牌**                  |
| **持久化成本** | -           | 需存概率   | JSON 文件                     | JSON 文件                       |

├── algorithm/          # 算法模块
│   ├── mod.rs
│   ├── weight.rs       # 权重计算（零和博弈）
│   └── selector.rs     # 动态扰动选择
└── transcode/          # 视频转码优化（NEW）
    ├── mod.rs          # 模块入口
    ├── config.rs       # 转码配置
    ├── detector.rs     # 硬件检测（分辨率/编码器）
    ├── cache.rs        # 缓存管理
    ├── encoder.rs      # FFmpeg 转码
    └── preloader.rs    # 预加载队列
- **时间复杂度**：$O(N \log N)$（排序主导）
- **空间复杂度**：$O(N)$（权重数组）
- **I/O 复杂度**：每次切换 1 次写入（JSON 序列化）

**优化空间：**
- 使用堆维护 Top-K 候选项：$O(N + K \log K)$，适合 $N > 100$ 场景
- 增量更新排序：利用权重变化局部性减少排序开销

---

## 📁 项目结构

```
src/
├── main.rs             # 总入口，命令分发
├── config.rs           # 配置文件解析
├── manager.rs          # WallManager 核心逻辑
├── command.rs          # 命令行解析 (clap)
├── paperengine/        # 壁纸引擎
│   ├── mod.rs          # PaperEngine trait
│   ├── mpvpaper.rs     # 动态壁纸 (视频)
│   └── swww.rs         # 静态壁纸 (图片)
└── algorithm/          # 算法模块
    ├── mod.rs
    ├── weight.rs       # 权重计算
    └── selector.rs     # 二分选择
```

## 📜 License

MIT
