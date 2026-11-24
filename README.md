# 备忘快贴

<p align="center">
  <img src="assets/logo.svg" width="96" height="96" alt="备忘快贴 logo">
</p>

一个只做一件事的 Windows 小工具：把常用的纯文本存成备忘录，双击一下，
内容就会粘贴回刚才正在输入的输入框。logo 由备忘纸和置顶图钉融合而成，
会出现在主窗口搜索框左侧、设置窗口以及安装包 / 可执行文件图标中。

界面基于 Rust + [GPUI Kit](https://github.com/longbridge/gpui-kit)，数据来自本地
YAML 文件或 Gitea / Gitee / GitHub / GitLab 仓库里的 YAML 文件，可以直接用编辑器修改。

## 功能

- 五种数据源：本地文件、Gitea 仓库文件、Gitee 仓库文件、GitHub 仓库文件、GitLab 仓库文件；
  远端数据源可在多台设备间共享同一份 `memos.yaml`。
- 备忘录列表：从 YAML 读取纯文本、备注和标签，支持按文本、备注或标签搜索。
- 标签过滤：左侧标签栏宽度可在设置中调整（默认 184 px，窗口过窄时自动收窄）；
  点选标签后只显示包含该标签的备忘录，标签按字典序排列，过长时以省略号截断，
  点「全部」恢复完整列表；可与搜索叠加。标签文字使用更小、更淡的字号，
  避免抢走备忘录正文的注意力。
- 卡片展示：灰色背景上的白色卡片，悬停时显示底部阴影（列表滚动时隐藏）；
  文本按实际可用宽度估算换行并调整高度，默认最多展示 3 行，可在设置中调整。
  卡片正文与底部之间用浅色虚线分隔：备注靠左、标签靠右，单行显示，
  超出以省略号截断；两者都没有时不显示虚线和底部行。
- 悬停预览：未完整显示的卡片，鼠标停留后在卡片右侧的独立预览窗口中显示完整文本、
  备注、标签与使用次数，不会遮挡下方卡片。
- 多显示器：按快捷键唤出时，窗口出现在鼠标指针所在的显示器中央。
- 自动收起：主窗口失去焦点时自动隐藏，粘贴后也会立即收起；隐藏时直接消失，
  不做淡出动画，避免操作后还要等窗口慢慢退场。
- 窗口尺寸：默认 800 × 600，调整后自动记住，下次启动沿用。
- 无标题栏：主窗口顶部直接是 logo、工具名称和搜索栏，不再显示系统标题栏和窗口按钮；
  拖动品牌区、搜索栏外围、左侧标签下方空白区域或底部信息区可移动窗口。
- 排序与置顶：置顶记录排在最前，右上角显示星标；其余按使用次数从多到少排序，
  次数相同时保持文件顺序。
- 粘贴：默认双击条目，内容写入剪贴板并自动发送 `Ctrl+V` 到刚才的前台窗口；
  可切换为单击触发或仅复制到剪贴板。
- 全局热键：默认 `Ctrl+Shift+X` 显示 / 隐藏窗口，可在设置中录制任意
  `Ctrl` / `Alt` / `Shift` 组合键。
- 设置窗口：常规、数据源、预览、外观、快捷键、粘贴、关于七个页面，支持跨页面搜索。
- 主题：内置 GPUI Kit 20+ 主题，可在设置 → 外观切换，选中态跟随主题色。
- 后台常驻：关闭窗口只是隐藏，再次按热键或重新启动程序即可唤回。
- YAML 持久化：`text` / `description` / `tags` 可在程序外编辑；
  `usage_count` / `is_pinned` 由程序维护并写回本地。
- 单实例：重复启动不会开出第二个窗口，只会唤回已有窗口。

## 使用

1. 准备一份 `memos.yaml`（格式见下文），放在程序所在目录或当前工作目录；
   也可以在设置 → 数据源中指定本地路径或远端仓库文件。
2. 运行 `quick-paste.exe`，窗口会读取配置的数据源。
3. 切到任意程序的输入框，按 `Ctrl+Shift+X` 打开本窗口，双击一条备忘录。
4. 内容会粘贴到刚才的输入框，本窗口自动隐藏。

搜索框里按 `Enter` 会直接粘贴第一条匹配的备忘录（可在设置中关闭）。
搜索框启动时不会自动获得焦点，按 `Ctrl+F` 或直接点击即可输入；
点击窗口其他区域会取消聚焦。

| 操作                     | 说明                                     |
| ------------------------ | ---------------------------------------- |
| 鼠标经过条目             | 显示置顶/取消置顶星标按钮（滚动时隐藏） |
| 鼠标经过未完整显示的条目 | 悬停预览完整文本、备注、标签与使用次数   |
| 点击左侧标签             | 只看包含该标签的备忘录；点「全部」恢复   |
| 双击 / 单击条目          | 粘贴到刚才的前台窗口（触发方式可设置）   |
| `Ctrl+Shift+X`           | 显示 / 隐藏窗口（默认值，可自定义）      |
| `Ctrl+F`                 | 聚焦搜索框                               |
| `Ctrl+,`                 | 打开设置                                 |
| `Esc`                    | 清空搜索 / 隐藏窗口                      |

按 `Alt+F4` 关闭只会隐藏窗口；失去焦点时会自动收起。要完全退出程序请点右下角「退出」。

## 设置

点主窗口右下角的齿轮，或按 `Ctrl+,` 打开设置窗口。左侧是七个页面，左上角的搜索框
可以跨页面搜索设置项；大部分修改会立即保存到
`%LOCALAPPDATA%\QuickPaste\settings.yaml`，数据源页面点「应用」后保存。

- **常规**：在鼠标所在屏幕弹出、失去焦点自动隐藏、启动时隐藏窗口。
- **数据源**：本地文件、Gitea、Gitee、GitHub、GitLab 五选一；远端数据源填写 YAML 文件
  URL 与 TOKEN，点「应用」后生效。
- **预览**：悬停预览开关、悬停停留时间（默认 0 秒，可选 0–0.4 秒，0 表示立即显示）、
  卡片正文最多显示行数。
- **外观**：GPUI Kit 内置 20+ 主题或跟随系统、主窗口标签栏宽度（80–240 px）、卡片圆角、平滑滚动。
- **快捷键**：全局唤出键；点「录制」后按下组合键即可自定义，更换后立即生效，
  注册失败会在页面内提示。
- **粘贴**：双击 / 单击触发、自动粘贴 / 仅复制、统计使用次数、
  粘贴后清空搜索（默认开启）、回车粘贴第一条。
- **关于**：版本、数据目录、数据源、项目主页与许可证。

设置窗口也可以按 `Esc` 关闭；失去焦点自动隐藏只作用于主窗口。

## 数据文件

程序的设置保存在 `%LOCALAPPDATA%\QuickPaste\settings.yaml`，备忘录内容则来自
设置 → 数据源中选择的一项。

### 五种数据源

- **本地文件**：默认自动查找「当前工作目录」和「exe 所在目录」下的
  `memos.yaml`；也可以填写绝对路径。只在本机可用。
- **Gitea 仓库文件**：读取自建 Gitea 上的 YAML，支持 `http://` 与 `https://`。
- **Gitee 仓库文件**：读取 Gitee 上的 YAML；地址必须是 `gitee.com` 域名。
- **GitHub 仓库文件**：读取 GitHub 上的 YAML；地址必须是 `github.com`、
  `raw.githubusercontent.com` 或 `api.github.com` 域名。
- **GitLab 仓库文件**：读取 GitLab（含私有化部署）上的 YAML，支持自定义域名、
  端口与 `http://` / `https://`；`gitlab.com` 必须使用 HTTPS。

远端 URL 可以填仓库的文件页面（`/blob/`、Gitea 的 `/src/`）或 raw 地址，
程序会自动转换成真正的原始文件地址。分支名包含 `/` 时请直接填 raw 或 API 地址。
GitLab 安装在子路径时，请直接填 `/api/v4/projects/.../repository/files/.../raw` 地址。
私有仓库需要填写 TOKEN；公开仓库可以留空。
TOKEN 以明文保存在本机 `settings.yaml`，不会写进日志或关于页。GitLab 的 TOKEN
至少需要 `read_repository`（或 `read_api`）权限。

GitLab / Gitea 使用自签名 HTTPS 证书时，先把 CA 证书导入 Windows
「受信任的根证书颁发机构」；程序通过系统证书库校验证书。

### 本地状态与缓存

`%LOCALAPPDATA%\QuickPaste\memos.yaml` 保存合并后的完整记录，包括备注、使用次数和置顶状态。
无论数据源在本地还是远端，程序都会把数据源里的 `text` / `description` / `tags`
与本地状态里的 `usage_count` / `is_pinned` 合并，再写回本地状态文件。
可执行文件名与数据目录沿用旧名 `quick-paste.exe` / `QuickPaste`，界面与安装包显示为
「备忘快贴」，以兼容既有快捷方式、设置和备忘录。

- 远端数据源只读取 `text` / `description` / `tags`；使用次数和置顶状态按设备各自维护，不写回远端。
- 远端数据源启动时先显示本地缓存，再在后台拉取远端文件；网络失败时保留缓存并提示。
- 外部修改数据源后，点右下角刷新按钮，或重新按热键唤出窗口即可重新加载。
- 程序不会改写数据源；没有找到本地数据源时，本地状态文件同时充当数据源。

格式是纯文本 YAML：

```yaml
# 备忘快贴 · 备忘录数据
# text / description / tags 由外部编辑；usage_count / is_pinned 由本程序维护。
# 修改后回到程序点击「重新加载」即可生效。
memos:
- text: 邮箱：me@example.com
  description: 常用邮箱
  usage_count: 3
  is_pinned: false
  tags:
    - 工作
    - 常用
- text: |-
    第一行
    第二行
  usage_count: 0
  is_pinned: true
  tags: []
```

- `text` 必填；`description` 可省略，作为备注显示在卡片底部左侧；
  `tags` 可省略、为空列表或包含多个标签。
- `usage_count` 省略时按 0 处理，`is_pinned` 省略时按 `false` 处理。
- 每条记录必须是包含 `text` 的映射；旧版纯字符串条目不再支持。
- 列表顺序：置顶优先；同为置顶或同为普通记录时，按 `usage_count` 从多到少；
  次数相同保持文件顺序。
- 搜索会同时匹配 `text`、`description` 和 `tags`，忽略大小写。
- 程序不提供新建、编辑、删除入口，记录统一在数据源文件 `memos.yaml` 中维护；
  没有数据源时直接编辑本地状态文件。
- 数据源中删除的记录，重新加载后会从本地状态中移除。
- 外部修改数据源后，点右下角刷新按钮，或重新按热键唤出窗口即可重新加载。
- 程序写本地状态时采用原子替换，会重写整个文件，自定义注释不会保留。


## 日志

程序没有完整的运行日志系统，但会记录定位问题所需的关键事件：

- `%LOCALAPPDATA%\QuickPaste\app.log`：启动路径、设置读取结果、
  数据源 / 本地状态的加载条数与错误、保存失败等，时间为 UTC。
- `%LOCALAPPDATA%\QuickPaste\crash.log`：panic、设置读取失败、单实例检查失败、
  预览窗口创建失败等崩溃类错误。
- `%TEMP%\quick-paste-startup.log`：仅在数据目录创建失败时写入。

设置 → 关于 → 「运行日志」可以直接打开 `app.log`；同页的「数据目录」和
「数据源」会显示程序实际使用的本地路径或远端 URL。备忘录解析失败时，主窗口底部也会显示红色提示。
排查读不到备忘录时，重点看日志里的「工作目录」「数据源」「加载完成」几行。

## 构建

需要 Windows 10/11 x64 和 Rust 1.96（`rust-toolchain.toml` 已固定版本）。

```powershell
cargo build --release
```

原始产物位于 `target\release\quick-paste.exe`。

需要 setup 安装包和 portable 便携包时：

```powershell
cargo install cargo-packager --version 0.11.8 --locked
pwsh -File scripts/package.ps1 -Arch x64 -PackageType both
```

产物位于 `target\packages\x86_64-pc-windows-msvc\`：

- `quick-paste_<版本>_x64-setup.exe`：NSIS 安装包
- `quick-paste_<版本>_x64-portable.zip`：便携包（exe、VC 运行库、README、LICENSE）

仓库自带 GitHub Actions 工作流（`.github/workflows/build.yml`），仅支持手动触发：

- 在任意分支上手动运行：执行单元测试、构建发布版，校验 exe 使用 Windows GUI
  子系统，打包 setup 与 portable，并上传两个产物。
- 手动运行并选择与 `Cargo.toml` 版本一致的 `v*` 标签：额外创建 GitHub Release，
  上传 setup 和 portable 两个文件。

## 实现说明

```text
src/
├── main.rs             入口
├── app.rs              GPUI 启动、窗口参数、单实例唤回
├── brand.rs            应用名称与 logo 展示
├── hotkey.rs           全局快捷键注册、自定义录制、切换与状态
├── logging.rs          运行日志（app.log）
├── memo.rs             YAML 读写、字段合并、排序、搜索、数据目录
├── memory_trim.rs      隐藏后清理工作集（Windows）
├── paste.rs            粘贴流程（Windows 实现 / 其他平台空实现）
├── paste/platform.rs   前台窗口跟踪、显示器定位、显示隐藏、SendInput
├── preview.rs          悬停预览使用的右侧独立弹出窗口
├── settings.rs         设置模型、持久化与主题应用
├── settings_window.rs  设置窗口（七个页面 + 设置项搜索）
├── single_instance.rs  文件锁单实例与唤回通知
├── source.rs           本地 / Gitea / Gitee / GitHub / GitLab 数据源与 URL 校验
├── themes.rs           内置 GPUI Kit 主题注册
├── themes/             21 个内置主题 JSON
└── ui.rs               标签栏、卡片列表、搜索、置顶、悬停预览调度与粘贴界面

assets/logo.svg         备忘纸 + 图钉 logo（界面与 README 用）
assets/logo.png         备用位图（512×512）
assets/icon.ico         安装包与可执行文件图标
scripts/package.ps1     setup + portable 打包
scripts/generate-icon.ps1  从 logo 设计生成 PNG / ICO
```

`assets/logo.svg` 是界面用的矢量图；`scripts/generate-icon.ps1` 按同一套坐标重绘
PNG / ICO。修改 logo 时需要同步调整两者，再运行
`pwsh -File scripts/generate-icon.ps1` 生成 `assets/logo.png` 与 `assets/icon.ico`。

列表行高按卡片正文的实际可用宽度估算换行数（默认最多 3 行，可在设置中调整）计算，
并固定正文、备注 / 标签的行高，确保底部行不会被卡片裁切。粘贴成功后对应记录的
`usage_count` 加一，并重新按排序规则排列。

粘贴流程：后台线程每 120ms 记录一次“不属于本进程的前台窗口”。双击备忘录时，
先把文本写入剪贴板，把前台交还给记录的窗口，隐藏本窗口，再发送 `Ctrl+V`。
发送前会等待物理 `Ctrl` / `Shift` / `Alt` 松开，避免再次触发本程序热键。

隐藏窗口后，程序会等待 10–30 秒；如果窗口仍不可见且私有工作集超过 10MB，
就调用 Windows 的 `EmptyWorkingSet` 清理工作集。这只降低任务管理器里的常驻
内存数字，不会减少私有提交内存；重新打开窗口时，页面会逐渐缺页调回。

## 查看内存占用

先启动 quick-paste（保持运行），在 PowerShell 里执行：

```powershell
Get-Process quick-paste |
  Select-Object Id,
    @{n='WorkingSetMiB';e={[math]::Round($_.WorkingSet64 / 1MB, 1)}},
    @{n='PrivateCommitMiB';e={[math]::Round($_.PrivateMemorySize64 / 1MB, 1)}}
```

`WorkingSet64` 是总工作集，包含共享页；`PrivateMemorySize64` 是私有提交，
通常对应任务管理器的「提交大小」。

要看更完整的口径，用 WMI 性能类读取专用工作集：

```powershell
$p = Get-Process quick-paste
Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
  Where-Object IDProcess -eq $p.Id |
  Select-Object Name,
    @{n='WorkingSetMiB';e={[math]::Round($_.WorkingSet / 1MB, 1)}},
    @{n='PrivateWSMiB';e={[math]::Round($_.WorkingSetPrivate / 1MB, 1)}},
    @{n='CommitMiB';e={[math]::Round($_.PrivateBytes / 1MB, 1)}},
    @{n='VirtualGiB';e={[math]::Round($_.VirtualBytes / 1GB, 2)}},
    PageFaultsPerSec
```

- `WorkingSetMiB`：总工作集，当前驻留在物理内存里的页，含共享页。
- `PrivateWSMiB`：专用工作集，真正属于本进程的常驻物理页。
- `CommitMiB`：私有提交，进程已承诺的内存，可能在页面文件里。
- `VirtualGiB`：保留的虚拟地址空间，不是实际物理内存。
- `PageFaultsPerSec`：当前缺页速率，突然升高说明页面正在被调回。

任务管理器进程页里的「内存」一般显示专用工作集；「详细信息」里右键列头选择
「选择列」，可以加上「提交大小」「工作集」「专用工作集」对照。隐藏窗口
10–30 秒后再测，工作集会下降，但私有提交不会变；重新打开窗口后工作集会
逐渐涨回。

## 已知限制

- 仅支持 Windows；粘贴依赖目标程序接受 `Ctrl+V`。
- 如果目标程序以管理员权限运行，Windows 的 UIPI 可能拦截模拟按键，此时需要
  以管理员身份运行本程序。
- 粘贴后剪贴板内容会被备忘录文本覆盖，这是预期的行为。
- 列表默认展示 3 行文本，可在设置中调整；超出部分可用悬停预览查看。
- 没有剪贴板历史、图片、表情、音效、同步或自动更新。
