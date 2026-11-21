# 快速粘贴

一个只做一件事的 Windows 小工具：把常用的纯文本存成备忘录，双击一下，
内容就会粘贴回刚才正在输入的输入框。

界面基于 Rust + [GPUI Kit](https://github.com/longbridge/gpui-kit)，数据保存在
本地 YAML 文件里，可以直接用编辑器修改。

## 功能

- 备忘录列表：新建、编辑、删除、搜索，条目只支持纯文本。
- 双击粘贴：双击条目，内容写入剪贴板并自动发送 `Ctrl+V` 到刚才的前台窗口。
- 全局热键：`Ctrl+Shift+X` 显示 / 隐藏窗口；被占用时自动改用 `Ctrl+Alt+V`。
- 后台常驻：关闭窗口只是隐藏，再次按热键或重新启动程序即可唤回。
- YAML 持久化：数据文件可直接编辑，程序内点「重新加载」即可生效。
- 单实例：重复启动不会开出第二个窗口，只会唤回已有窗口。

## 使用

1. 运行 `quick-paste.exe`，窗口打开。
2. 点「新建」输入内容并保存。
3. 切到任意程序的输入框，按 `Ctrl+Shift+X` 打开本窗口，双击一条备忘录。
4. 内容会粘贴到刚才的输入框，本窗口自动隐藏。

搜索框里按 `Enter` 会直接粘贴第一条匹配的备忘录。

| 操作           | 说明                                     |
| -------------- | ---------------------------------------- |
| 双击条目       | 粘贴到刚才的前台窗口                     |
| `Ctrl+Shift+X` | 显示 / 隐藏窗口（冲突时用 `Ctrl+Alt+V`） |
| `Ctrl+N`       | 新建备忘录                               |
| `Ctrl+F`       | 聚焦搜索框                               |
| `Ctrl+Enter`   | 保存正在编辑的备忘录                     |
| `Esc`          | 取消编辑 / 清空搜索 / 隐藏窗口           |

关闭窗口（`Alt+F4` 或标题栏 ×）只会隐藏窗口，要退出程序请点右下角「退出」。

## 数据文件

默认位置：

```text
%LOCALAPPDATA%\QuickPaste\memos.yaml
```

格式是纯文本 YAML：

```yaml
# 快速粘贴 · 备忘录数据
# 纯文本 YAML，可直接编辑；回到程序后点击「重新加载」即可生效。
memos:
- 邮箱：me@example.com
- |-
  第一行
  第二行
```

- `memos` 下的每一项就是一个字符串，也兼容顶层的字符串列表。
- 纯数字或 `true` / `false` 也会按文本读取；`null` 和空白项会被忽略。
- 其余内容保持原样（包括多行和前导空格）。
- 外部修改文件后，点右下角的刷新按钮，或重新按热键唤出窗口即可重新加载。
- 程序保存时会重写整个文件，内容会保留，自定义注释不会保留。
- 程序写文件采用原子替换，避免中途退出留下半个文件。

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
├── app.rs              GPUI 启动、窗口参数、全局热键、单实例唤回
├── memo.rs             YAML 读写、搜索、数据目录
├── memory_trim.rs      隐藏后清理工作集（Windows）
├── paste.rs            粘贴流程（Windows 实现 / 其他平台空实现）
├── paste/platform.rs   前台窗口跟踪、Win32 显示隐藏、SendInput
├── single_instance.rs  文件锁单实例与唤回通知
└── ui.rs               备忘录界面

scripts/package.ps1     setup + portable 打包
```

粘贴流程：后台线程每 120ms 记录一次“不属于本进程的前台窗口”。双击备忘录时，
先把文本写入剪贴板，把前台交还给记录的窗口，隐藏本窗口，再发送 `Ctrl+V`。
发送前会等待物理 `Ctrl` / `Shift` / `Alt` 松开，避免再次触发本程序热键。

隐藏窗口后，程序会等待 10–30 秒；如果窗口仍不可见且私有工作集超过 30MB，
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
- 没有剪贴板历史、图片、表情、音效、同步或自动更新。
