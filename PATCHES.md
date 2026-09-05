# PATCHES —— 唤星对 goose 内核的改动登记

> 本文件是**我们对上游 goose 的全部改动的唯一台账**。仓根 `PATCHES.md` 逐个登记
> 「改动点、理由、上游回馈可能性」，判据见父仓
> `docs/产品与技术/技术设计/02-平台能力/Runtime与工具体系/Goose内核Runtime/01-总体设计.md` §11.1。

## 分支纪律

| 分支 | 用途 | 规则 |
|---|---|---|
| `main` | 上游 `aaif-goose/goose` 的镜像 | **禁止写入**。只做 `git merge --ff-only upstream/main` |
| `hasn` | 我们的工作分支 | 裁剪 + 薄 patch 全部落这里；主 clone 停在本分支 |

```bash
git remote -v
# origin    git@github.com:youngshunf/goose.git      （我们的 fork，公开）
# upstream  https://github.com/aaif-goose/goose.git  （上游）
```

## 薄 patch 纪律（破一次就再也回不去）

对内核的改动**只许两类**：

1. **feature 裁剪**（`crates/goose/Cargo.toml` 等）；
2. **平台工具挂载点**（宿主进程内工具注入）。

内核逻辑非改不可时**优先回馈上游**。这是 rebase 成本可控的**唯一**前提——
月度 rebase 的预估（0.5–1 人日/月）完全建立在「patch 少且都在固定位置」之上。

⛔ **不许**为了省事直接改内核逻辑；⛔ **不许**重排上游目录结构（每次 rebase 的冲突地狱）。

## 改动登记

| # | 改动点 | 类别 | 理由 | 上游回馈可能性 |
|---|---|---|---|---|
| —— | **当前为空** | —— | RT-0 只建仓，未开始施工 | —— |

> 加一条 patch 就在上表加一行，**不要攒着**。评审判据是：
> 这张表的行数 == `git diff upstream/main...hasn` 里非裁剪类改动的处数。

## 建仓基线（2026-09-04，RT-0）

fork 自上游 `aaif-goose/goose`，建仓时 `origin/main` 与 `upstream/main` 完全同步。

| 项 | 建仓时实测 |
|---|---|
| 上游版本 | `1.49.0` |
| edition / rust-version | `2021` / `1.94.1` |
| workspace crates | 15 个 |
| 建仓时 HEAD | `5e9092596 Support GPT-6 Astra models (#11869)` |

⚠️ **与 2026-08-03 PoC 快照的漂移**（PoC 那份在父仓 `external/goose`，已停更且带我们自己的
PoC patch，**不是干净上游**）：

- 版本 `1.45.0` → `1.49.0`，其间 **385 个上游提交**；
- crates `12` → `15`，新增 `goose-agent` / `goose-context-management` / `goose-roaming`。
  ⚠️ `goose-agent` 是**新增的独立 crate**（events / inference / machine / operation / tool），
  **不是**把现有 agent loop 挪走——`Agent::reply` 仍在 `crates/goose/src/agents/agent.rs`。
  但它可能是上游的演进方向，**每次 rebase 都要重看这一条**；
- 🔴 **PoC 的两处挂载点坐标已部分失效**：PoC patch 打在
  `agent.rs`(+27) + `platform_tools.rs`(+26)；今天 platform tool 的先例
  （`manage_schedule`）已迁到 `crate::agents::platform_extensions::scheduler`，
  `agent.rs` 仍是分发处（`:4369` / `:4407`），且 `agents/` 下新增了整个 `state_machine/` 子系统。
  **施工时必须按当时的源码重新定位挂载点，不得照抄 PoC 坐标。**

## 消费方

`hasn-node` 经 cargo git 依赖消费本仓，**只引 `goose` 一个 crate**、`rev` 钉死：

```toml
goose = { git = "https://github.com/youngshunf/goose.git", branch = "hasn", rev = "<commit>" }
```

本 fork 是**公开**仓，因此所有构建者（开发者本机、打包机、CI、云端镜像）**零凭据**即可拉取。
它同时是父仓 `CLAUDE.md`「Rust 版本策略」里那道**升级门的构建期验证对象**——
升级 toolchain 前在目标版本下把本仓 `hasn` 分支编一遍。
