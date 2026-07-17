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
- **安静且实时。** 只请求你正在查看的那个标签页的数据，缓存在内存中并按计时刷新——不写入磁盘。

## 特性

- **Provider 标签页。** 从本地凭证自动检测 Claude 与 Kimi。两者都存在时显示标签栏
  （Claude 在前），用 `h`/`l` 或方向键切换；只有一个时不显示标签栏。
- **圆角标题框。** 每个 provider 的用量都用 `╭─ Claude ─╮` 圆角框包住，标题带上套餐名
  （例如 `╭─ Claude · Max ─╮`、`╭─ Kimi · pro ─╮`）。
- **顶部信息栏。** 顶部一行显示最近更新时间、距今多久、以及刷新频率——例如
  `Updated 22:41:07 (Asia/Shanghai) · 12s ago · every 5m`。
- **带节奏的进度条。** `|` 标记表示当前窗口已过去的比例（即「应有」进度）；进度条在你处于/低于
  节奏时为绿色，略微超前为黄色，明显超前为红色，并附带 `↑`/`↓` 差值。
- **关键额度窗口。** Claude：当前会话（5 小时）、当前一周（全模型）、以及各模型的每周额度；
  Kimi：当前会话（5 小时）、当前一周、以及每月配额。
- **只请求当前标签页。** 切换标签页时，若缓存数据未超过刷新频率则直接复用；两次运行之间不做持久化。
- **Kimi 令牌自动刷新。** Kimi 的短时效访问令牌会自动刷新（与 `kimi` CLI 行为一致），并以
  `0600` 权限写回。

## 依赖

- **仅支持 macOS**（读取 Claude Code 的钥匙串条目与 Kimi 的凭证文件）。
- 以下至少满足其一：
  - **已登录 Claude Code** —— 其 OAuth 凭证存于登录钥匙串。能运行 `claude` 即可。
  - **已登录 Kimi** —— 凭证位于 `~/.kimi-code/credentials/kimi-code.json`。能运行 `kimi` 即可。

若两者都未登录，`agent-limit` 会提示你去运行哪个 CLI。

## 安装

### 下载发布二进制（Apple Silicon）

```sh
VERSION=v0.2.1
curl -fsSL -o agent-limit.tar.gz \
  "https://github.com/Hanyang-Li/agent-limit/releases/download/${VERSION}/agent-limit-${VERSION}-aarch64-apple-darwin.tar.gz"
tar -xzf agent-limit.tar.gz
sudo mv agent-limit /usr/local/bin/    # 或放到你的 PATH 上任意位置
```

每个发布还附带 `.sha256`，可用 `shasum -a 256 -c` 校验。

### 使用 Cargo

```sh
cargo install --git https://github.com/Hanyang-Li/agent-limit --tag v0.2.1 --locked
```

### 从源码构建

```sh
git clone https://github.com/Hanyang-Li/agent-limit
cd agent-limit
cargo build --release
# 二进制位于 target/release/agent-limit
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

## 工作原理

- **检测与标签页。** 启动时 `agent-limit` 检测 Claude Code 凭证（登录钥匙串）与 Kimi 凭证
  （`~/.kimi-code/credentials/kimi-code.json`）。可用的 provider 以固定顺序成为标签页——
  先 Claude，后 Kimi。
- **只请求正在看的那个。** 当前标签页在启动时请求一次，之后在数据超过刷新频率（默认 5 分钟）
  时自动刷新。切换到其它标签页时，若其内存数据仍新鲜则复用，过期或从未加载则请求。后台标签页
  从不请求，且不写入磁盘。
- **节奏标记。** 对于已知重置时间的窗口，`|` 落在窗口已过去比例的位置——即你的「按节奏应有」
  位置。进度条颜色将实际用量与之比较：绿色（持平/低于）、黄色（超前约 20 个百分点内）、
  红色（更超前）。
- **Kimi 令牌。** Kimi 访问令牌很快过期。每次请求前 `agent-limit` 会检查其新鲜度，必要时
  向 `auth.kimi.com` 发起 `refresh_token` 授权（与 `kimi` CLI 完全一致），随后以 `0600`
  权限原子写回轮换后的令牌。遇到 `401` 时强制刷新一次并重试。除用量接口与令牌接口外，不联系
  任何其它服务。

## 开发

```sh
cargo build --release
cargo test
```

推送 `v*` 标签时，GitHub Actions 会自动构建并发布（`.github/workflows/release.yml`）：
构建并测试 `aarch64-apple-darwin` 二进制，打包为 tar 并附上校验和，然后连同自动生成的发布说明
一起附加到 GitHub Release。

## 许可证

[MIT](LICENSE) © Hanyang Li
