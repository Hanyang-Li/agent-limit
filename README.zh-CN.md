# agent-limit 📊

> 在终端里实时查看 Claude Code 与 Kimi 的套餐用量 —— 直接读取本地凭证。

[English](README.md) | **简体中文**

![license](https://img.shields.io/badge/license-MIT-blue.svg)
![platform](https://img.shields.io/badge/platform-macOS-lightgrey.svg)

`agent-limit` 是一个小巧的终端 UI，展示你的 Claude Code 与 Kimi 编程套餐已经用掉了多少
（会话、每周、每月各个额度窗口），并配有实时进度条、进度基准标记，以及一眼可辨的颜色——
告诉你当前是领先还是落后于配额消耗节奏。它直接读取 `claude` 和 `kimi` CLI 已经存在本机上的
凭证，无需登录，也不用粘贴任何 API Key。

- **两个 provider，自动检测。** 你登录了哪个（Claude Code / Kimi）就显示哪个；两个都登录
  就变成可切换的标签页。
- **看的是节奏，不只是百分比。** 每个进度条都有一个 `|` 标记，指向当前时间点你「应该」用到
  的位置；当你落后或超前于该节奏时，进度条会在绿 / 黄 / 红之间变化。
- **安静且实时。** 只请求你正在查看的那个标签页的数据，并按计时刷新——不写入磁盘。

## 依赖

- **仅支持 macOS**（读取 Claude Code 的钥匙串条目与 Kimi 的凭证文件）。
- 以下至少满足其一：
  - **已登录 Claude Code** —— 能运行 `claude` 即可。
  - **已登录 Kimi** —— 能运行 `kimi` 即可。

若两者都未登录，`agent-limit` 会提示你去运行哪个 CLI。

## 安装

一行命令安装最新发布版（macOS，Apple Silicon）：

```sh
curl -fsSL https://raw.githubusercontent.com/Hanyang-Li/agent-limit/main/install.sh | sh
```

脚本会下载发布二进制、校验其 checksum，并安装到 `~/.local/bin`（无需 `sudo`）。
若该目录不在 `PATH` 上，脚本会写入你的 shell 配置文件（zsh/bash/fish）。可用
`AGENT_LIMIT_VERSION`、`AGENT_LIMIT_INSTALL_DIR` 覆盖默认行为，或设置
`AGENT_LIMIT_NO_MODIFY_PATH=1` 跳过对 `PATH` 的修改。

或使用 Cargo 安装：

```sh
cargo install --git https://github.com/Hanyang-Li/agent-limit --tag v0.2.3 --locked
```

## 使用

```sh
agent-limit
```

```
Options:
  -i, --interval <SECONDS>   刷新间隔，单位秒（默认：300，最小：60）
  -p, --provider <PROVIDER>  首个打开的标签页：claude | kimi（默认：claude）
  -h, --help                 打印帮助
  -V, --version              打印版本
```

以 Kimi 标签页打开，每两分钟刷新一次：

```sh
agent-limit -p kimi -i 120
```

如果指定的 provider 未登录，`agent-limit` 会回退到第一个可用的 provider；标签顺序始终是
先 Claude、后 Kimi。

### 按键

| 按键 | 作用 |
| --- | --- |
| `h` / `←`、`l` / `→` | 切换标签页（两个 provider 都存在时） |
| `R` | 立即刷新当前标签页（有短暂冷却） |
| `Q` | 退出（`Esc` 与 `Ctrl+C` 也可退出） |

也支持**鼠标**：点击标签页即可切换，点击右下角的 **[R]efresh** / **[Q]uit** 即可刷新 / 退出。

## 开发

```sh
git clone https://github.com/Hanyang-Li/agent-limit
cd agent-limit
cargo build --release   # 二进制位于 target/release/agent-limit
cargo test
```

推送 `v*` 标签时，GitHub Actions 会自动构建并发布（`.github/workflows/release.yml`）：
构建并测试 `aarch64-apple-darwin` 二进制，打包为 tar 并附上校验和，然后连同自动生成的发布说明
一起附加到 GitHub Release。

## 许可证

[MIT](LICENSE) © Hanyang Li
