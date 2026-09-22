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

### 🔴 2026-09-22 裁决：批准**一条**内核逻辑薄 patch（`#1`），⛔ 不构成放宽

`#1`（`create_session_with_id`）既不是 feature 裁剪也不是平台工具挂载点，命中的是
上面那句「内核逻辑非改不可时**优先回馈上游**」的口子。批准它的理由是**另一条路被两条
判据同时挡死**：

- 父仓 ADR `2026-09-22-Runtime内核自持推理循环与会话.md` **E4** 逐字：
  「一个 IM 会话 = 一个 runtime session，一个工作会话 = 一个 runtime session。
  **派生入口唯一，不得再有第二种**」；
- 施工文档 `10-goose内核完整接入施工文档.md` **§2.7** 逐字：
  「⛔ 不得在本文任何一片里新增第四种派生方式」。

⇒ 「在 `owner.db` 里存一张 `runtime_session_id ↔ goose session id` 映射表」正是那个
**第二个身份、第四种派生**，不可接受。薄 patch 是唯一不违规的出路。

⛔ **这条裁决只覆盖 `#1` 这一条。** 再出现第三类 patch 仍要单独裁决，
⛔ 不得引用本段当作「内核逻辑可以改了」。

## 改动登记

| # | 改动点 | 类别 | 理由 | 上游回馈可能性 |
|---|---|---|---|---|
| 1 | `crates/goose/src/session/session_manager.rs`：新增 `SessionManager::create_session_with_id` 与其存储层同名实现（**纯新增 166 行，0 删除**） | 内核逻辑（经 2026-09-22 单条裁决，见上） | 既有 `create_session` 的 id 是**在 SQL 里现算**的 `YYYYMMDD_N`，公开面上没有任何一条路能用调用方给的 id 建会话；而 `Agent::reply` 紧接着 `get_session(&session_config.id, true)`（`agents/agent.rs`），查不到就 `Err`。⇒ 嵌入 goose 的宿主（唤星 daemon 已有自己的 `RuntimeSessionId`）**无法让 `SessionConfig.id` 等于自己的会话身份**。另一条路（映射表）被 ADR E4 与施工文档 §2.7 同时挡死 | 🟢 **高**。让嵌入方自带 session id 是通用需求，与唤星业务零耦合。上游 PR 材料见下节，⚠️ 尚未提交（对外动作需主人授权） |

> 加一条 patch 就在上表加一行，**不要攒着**。评审判据是：
> 这张表的行数 == `git diff upstream/main...hasn` 里非裁剪类改动的处数。

### `#1` 的最小性与零影响，逐条可复跑

| 判据 | 命令 | 事实 |
|---|---|---|
| 纯新增，一行未删 | `git diff upstream/main...hasn -- crates/goose/src/session/session_manager.rs --stat` | `166 insertions(+), 0 deletions(-)` |
| 既有 `create_session` 的签名与函数体一字未动 | `git diff upstream/main...hasn -- crates/goose/src/session/session_manager.rs \| grep -c '^-[^-]'` | **`0`**（⚠️ 判据写成 `^-[^-]`，不是「没有以 `-` 开头的行」——`--- a/…` 那行就是以 `-` 开头的，照后者写会永远假红） |
| 现有调用方零影响 | `grep -rn "create_session(" --include='*.rs' . \| wc -l` | 打 patch 前 **124**，之后 **125**。多的那一处是本 patch 自己那条非真空对照单测；**原有 124 处一处未改**。⚠️ 这个 grep **不会**匹配 `create_session_with_id(`（后者另有 5 处，全是新增） |
| 不动 schema / 不动迁移 | `git diff upstream/main...hasn --stat` | 只有 `session_manager.rs` 与 `PATCHES.md` 两个文件；`migrate_to_version` 的 `match` 分支一条未加 |
| id 生成规则本身未动 | 单测 `create_session_still_generates_its_own_dated_id` | 非真空对照：既有 `create_session` 仍给出 `YYYYMMDD_1` |

### `#1` 的行为契约

- `id` 逐字落库，⛔ 不做任何规范化、不加前缀；
- `id` 已存在时**确定失败**——`sessions.id` 的主键约束当场拒绝，`sqlx` 的
  `UNIQUE constraint failed: sessions.id` 原样上抛。
  ⛔ **不会退回自动生成的 id**：调用方给的 id 就是它自己的身份，静默改写等于让它
  再也拿不回这条会话；
- 其余六列的取值、事务隔离级别（`BEGIN IMMEDIATE`）与 telemetry 埋点与
  `create_session` 逐字相同。

判据：`crates/goose/src/session/session_manager.rs` 的两条单测
`create_session_with_id_keeps_the_caller_id_and_rejects_a_duplicate`（四段断言：
逐字落库 / 第二次 `Err` / 首条未被改动 / 库里恰好一条）与
`create_session_still_generates_its_own_dated_id`（非真空对照）。

**这两条不是「跑绿了」而已，它们被证伪过一次**（2026-09-22，`cargo test -p goose --lib
session::session_manager::tests::create_session`）：

| 源码 | 结果 |
|---|---|
| 原样 | `ok. 2 passed; 0 failed`（rc=0） |
| **变异**：`create_session_with_id` 撞 id 时静默 `return self.create_session(...)` | `FAILED. 1 passed; 1 failed`（rc=101），失败行逐字是「同一个 id 第二次居然建成了——调用方的会话身份被静默改写了」 |
| 变异经 `git checkout --` 还原后 | 再次 `ok. 2 passed; 0 failed`（rc=0） |

⇒ 「静默改写 id」这条**唯一**危险行为确实会被这条用例抓住，而不是恰好绿。

## 上游 PR 材料（⚠️ 尚未提交）

⛔ **还没有向上游提任何 issue 或 PR**——对外动作要主人授权。本节是**准备好的材料**，
授权之后原样用。⚠️ 提交前要把 `#1` 那两段中文 doc comment 换成本节末尾给好的英文版
（唤星仓内代码注释一律中文是父仓 `CLAUDE.md` §1 的硬规则，上游不适用该规则）；
测试里的中文断言消息同理，⛔ 别漏。

**目标仓**：`aaif-goose/goose`　**类型**：feature（附测试）　**预计 diff**：+166 / −0

**标题**

```
feat(session): let embedders create a session with a caller-supplied id
```

**正文**

```markdown
### Problem

`SessionManager::create_session` mints the session id inside the SQL statement
(`YYYYMMDD_N`, see `session_manager.rs`). There is no public path to create a
session under an id the caller already owns.

That blocks embedding goose as a library. A host process that already has its
own conversation/session identity cannot make `SessionConfig.id` equal to it,
because `Agent::reply` immediately calls `get_session(&session_config.id, true)`
and errors out when the row is missing. The only workarounds are (a) keeping a
second id-to-id mapping table on the host side — a second source of truth for
"which conversation is this" — or (b) reading back the generated id and
rewriting the host's own identity, which is not possible when that identity is
already on the wire.

### Change

Adds `SessionManager::create_session_with_id(id, working_dir, name,
session_type, goose_mode)` plus the matching storage method. It is the existing
`create_session` with one difference: the id is bound as a parameter instead of
being computed in SQL. Everything else — the remaining six columns, the
`BEGIN IMMEDIATE` transaction, the telemetry hook — is identical.

`create_session` is untouched: same signature, same body, same generated-id
behaviour. The change is additive only (+166 / −0); all 124 existing call sites
keep using it.

### Duplicate ids fail loudly

Inserting an id that already exists hits the `sessions.id` primary key and the
error propagates unchanged. It deliberately does **not** fall back to a
generated id: the caller's id is its identity, and silently rewriting it would
leave the caller unable to find its own session.

### Tests

- `create_session_with_id_keeps_the_caller_id_and_rejects_a_duplicate` — the id
  is stored verbatim; a second create under the same id returns `Err`; the first
  row is unchanged; the store holds exactly one session afterwards.
- `create_session_still_generates_its_own_dated_id` — negative control proving
  the existing `create_session` still mints `YYYYMMDD_N`.
```

**如果上游希望先开 issue 讨论**，标题与一段话正文：

```
Title: No public API to create a session with a caller-supplied id

Embedding goose as a library requires `SessionConfig.id` to be the host's own
session identity, but `SessionManager::create_session` always mints
`YYYYMMDD_N` in SQL and `Agent::reply` then looks the session up by that id, so
an embedder has no way to create the row it needs. Would you accept a small
additive `create_session_with_id` that binds the id as a parameter and leaves
`create_session` untouched (duplicate ids failing on the primary key rather
than falling back to a generated id)?
```

**提交前要替换的两段 doc comment（英文版，逐段对应）**

`SessionManager::create_session_with_id`：

```rust
/// Creates a session under an id supplied by the caller.
///
/// [`SessionManager::create_session`] mints a `YYYYMMDD_N` id inside the SQL
/// statement, so a host that embeds goose — and already owns a session
/// identity of its own — has no way to create the row it needs: `Agent::reply`
/// immediately calls `get_session(&session_config.id, true)` and errors out
/// when the row is missing. This is that missing path.
///
/// [`SessionManager::create_session`] is untouched: same signature, same body,
/// same generated-id behaviour, and every existing caller keeps using it.
///
/// # Errors
///
/// Fails deterministically when `id` already exists: the `sessions.id` primary
/// key rejects the insert and the error propagates unchanged
/// (`UNIQUE constraint failed: sessions.id`). It deliberately does **not** fall
/// back to a generated id — the caller's id is its identity, and silently
/// rewriting it would leave the caller unable to find its own session.
```

`SessionStorage::create_session_with_id`：

```rust
/// Creates a session under an id supplied by the caller.
///
/// The only difference from [`SessionStorage::create_session`] is where the id
/// comes from: that one computes `YYYYMMDD_N` in SQL, this one binds `id` as a
/// parameter. The remaining six columns, the `BEGIN IMMEDIATE` transaction and
/// the telemetry hook are identical.
///
/// A duplicate `id` is rejected by the `sessions.id` primary key and the error
/// propagates unchanged.
```

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

`hasn-node` 经 cargo git 依赖消费本仓，`rev` 钉死、**不跟分支**：

```toml
# modules/runtime-host/goose/Cargo.toml（现读）
goose                = { git = "https://github.com/youngshunf/goose.git", rev = "da0ee4c3…", default-features = false }
goose-providers      = { git = "https://github.com/youngshunf/goose.git", rev = "da0ee4c3…", default-features = false }
goose-provider-types = { git = "https://github.com/youngshunf/goose.git", rev = "da0ee4c3…" }
```

📌 **2026-09-22 订正**：本节原写「**只引 `goose` 一个 crate**」——那是 RT-0 建仓时的
预期。`K2` 真内核进编译图之后是**三个**（白名单在 `hasn-node`
`scripts/guards/goose-crates.allow`，由 `check-goose-outbound-isolation.py` 判），
其中只有 `goose-providers` 会真的发出站请求。⚠️ 取回协议是 `https://`（`K1` 改），
⛔ 不是 `ssh://`。

本 fork 是**公开**仓，因此所有构建者（开发者本机、打包机、CI、云端镜像）**零凭据**即可拉取。
它同时是父仓 `CLAUDE.md`「Rust 版本策略」里那道**升级门的构建期验证对象**——
升级 toolchain 前在目标版本下把本仓 `hasn` 分支编一遍。
