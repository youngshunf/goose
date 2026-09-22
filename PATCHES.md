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
| 现有调用方零影响 | `git grep -n 'create_session(' upstream/main -- '*.rs' \| wc -l` 与同一条打在 `hasn` 上 | 打 patch 前 **129**，之后 **130**（📌 2026-09-22 随上游同步重测，原记的是 `124`/`125`，**差的是上游自己新增的调用点，不是我们的 patch 变胖了**——增量恒为 `+1`）。多的那一处是本 patch 自己那条非真空对照单测；**原有 129 处一处未改**。⚠️ 这个 grep **不会**匹配 `create_session_with_id(`（后者另有 5 处，全是新增）。⚠️ 判据写成 `git grep`，⛔ 别写 `grep -rn … .`：本机 `grep` 被 alias 到 `ugrep`，从父仓根递归时会**跳过全部子仓**（在本仓根跑恰好还对，换个 cwd 就静默给 0） |
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

## 待回馈上游

> ⚠️ 本节与上面那节**不是一回事**：上面那节是「我们已经打了 patch，材料备好等授权提 PR」；
> 本节是「**我们没打 patch**，而是希望上游加一个能力／修一个 bug」。
> ⛔ 本节任何一条都**还没有**向上游提过 issue 或 PR——对外动作要主人授权。
>
> **为什么不自己打 patch**：这两条动的都是**上游的公开面**（一个公开枚举、一个 Cargo
> feature 声明）。薄 patch 纪律逐字「内核逻辑非改不可时**优先回馈上游**」，
> 而公开面的改动尤其要先跟上游谈——悄悄改一个公开枚举，每次 rebase 都要重打，
> 且上游哪天自己加了一个形状不同的等价物就变成两套。

### `F-1`（唤星侧代号 `A4`）：内核把**终态原因**压成了一句英文正文，嵌入方判别不了

**登记方**：唤星 `K5`（`hasn-node` `modules/runtime-host/goose/src/events.rs`，2026-09-22）。
**现读基线**：fork `hasn` 分支 `4f1b751c`（= 上游 `5e909259` + 我们的 `#1`）。

#### 现象

内核在**若干条终局路径**上的做法是：yield 一条普通的 assistant 消息，然后 `break`。
消费 `AgentEvent` 流的嵌入方看到的与「模型正常答完了一句话」**在类型上完全一样**：

| 终局 | 内核吐什么 | 坐标 |
|---|---|---|
| 上游连续返回空应答，重试 `MAX_EMPTY_TURN_RETRIES` 次之后 | `Message::assistant().with_text(EMPTY_TURN_MESSAGE)` | `agents/agent.rs:3452`（常量在 `:94`，**私有 `const`**） |
| 到 `SessionConfig.max_turns` 上界 | `Message::assistant().with_text(MAX_TURNS_MESSAGE)` | `agents/agent.rs:2646`（常量本体在 `agents/state_machine/ops_maxturns.rs:14`，它自己虽是 `pub const`，但 `ops_maxturns` 是**私有模块**（`state_machine/mod.rs:16`），对外只有 `state_machine/mod.rs:59` 那条 `pub(super) use` ⇒ **crate 外引用不到**） |
| 压缩之后上下文仍然超限 | `SystemNotification(InlineMessage, "Unable to continue: Context limit still exceeded after compaction.")` | `agents/agent.rs:3188` |
| provider 拒答（`ProviderError::Refusal`） | `Message::assistant().with_text("The provider refused this request…")` | `agents/agent.rs:3271` |

⇒ **嵌入方会把「上游一个字节都没给」报成这一轮成功**，而主人看到的是分身突然说了一句英文。
第三行更隐蔽：那条 `InlineMessage` 与压缩进度提示（`"Compacting to continue conversation…"`）
**是同一个 `SystemNotificationType`**，一个是终局一个是进度，类型上分不开。

#### 🔴 「把那几个常量对外放出来」不是修法，已评估并否决

| | 抄一份字面量 | 常量对外可见之后引用它 |
|---|---|---|
| 上游改文案 | 静默失配 | 静默失配（**除非**同批 bump rev 且有人盯着） |
| 那句话由别的原因产生 | 误判 | **一样误判** |

三条理由，缺一条都还能争：

1. **它没有换掉判据的性质**，只换掉了「字面量从哪抄来」——两侧都是字符串相等判定；
2. **按构造就分不干净**：`MAX_TURNS_MESSAGE` 是 `last_assistant_text`，会沿子配方/子分身
   那条链变成**父分身的 `ToolResponse`**（现读
   `agents/state_machine/tests/recipe_scheduling_lifecycle.rs:162` 逐字
   `assert_message(-2, ToolResponse, MAX_TURNS_MESSAGE)`）。同一字面量在两种角色里都合法
   ⇒ 相等判定天然有假阳；
3. **它只闭得了半格**：`EMPTY_TURN_MESSAGE` 连 `pub(super)` 都不是，要用同一手法就得
   再提一次可见性。**一条禁令要靠开两个口子来绕过，说明禁的是对的。**

#### 诉求：一个**类型上分得开**的信号

两种形状都能解决，**优先 (a)**：

```rust
// (a) 给 AgentEvent 加一格（goose-agent/src/events.rs）
pub enum AgentEvent {
    Message(Message),
    Usage(ProviderUsage),
    MessageUsage { message_id: Option<String>, usage: MessageUsage },
    McpNotification((String, ServerNotification)),
    HistoryReplaced(Conversation),
    /// 这一轮为什么结束。正常答完时是 `Completed`。
    TurnEnded(TurnEndReason),
}

#[non_exhaustive]
pub enum TurnEndReason {
    Completed,
    MaxTurnsReached,
    EmptyTurnRetriesExhausted,
    ContextLimitExceededAfterCompaction,
    ProviderRefused,
    CreditsExhausted,
}
```

```text
(b) 退而求其次：保留那条正文，把原因挂到消息元数据上
    （MessageMetadata 加一个 end_reason: Option<TurnEndReason>）。
```

**为什么 (a) 更好**：`AgentEvent` 加一格之后，任何写了**穷举 `match`** 的消费方会在
**编译期**被点名。唤星这一侧的 `agent_event_signals` 正是无通配臂的穷举 `match`
⇒ 上游合了这一格，我们当场编译失败并被逼着回答它。(b) 是一个可选字段，
漏读它的消费方继续静默。

⚠️ **这是公开枚举的改动**，因此流程是**先开 issue 讨论形状、再提 PR**，
⛔ 不适合塞进一条薄 patch 悄悄改。

#### 唤星这一侧在等它的那条登记

`hasn-node` `modules/runtime-host/goose/tests/it/dispatch.rs::上游静默关流在新形态下退化成一条正文这是登记在案的缺口`
是一条**会说话的**断言：它**故意断言当前这个退化形态**（终态是 `Final`），
上游给了可判别信号、我们接上之后终态变回 `Failed`，它当场红并提醒删掉那段登记。
⛔ 不要因为「它看起来在断言一个 bug」就把它删掉。

同族还有 `modules/runtime-host/goose/src/events.rs` 头注里那条：`InlineMessage` 里混着终局，
我们今天只能把整类 `InlineMessage` 当进度处理（否则每一轮自动压缩都会失败）。

### `F-2`：`crates/goose` 独立构建缺 `process-wrap/process-session` feature 声明

**登记方**：唤星 `K2`（施工文档 §1.5 `B3`）。**性质**：上游的一个 bug，不是我们的需求。

`crates/goose/src/agents/platform_extensions/developer/shell.rs:216` 逐字
`use process_wrap::std::{CommandWrap, ProcessSession};`，而 `ProcessSession` 在
process-wrap 9.1.0 里挂在 **`process-session`** feature 后面；
`crates/goose/Cargo.toml` 只声明了 `features = ["std"]`。

它在上游自己的 workspace 里能过，是因为同一 workspace 里的 `crates/goose-mcp`
声明了 `features = ["std", "process-session"]`，**cargo 的 feature 合一把那一格捎带打开了**。
把 `goose` 单独当依赖拉出来时没有那个捎带 ⇒ **`E0432`**。

**修法**：`crates/goose/Cargo.toml` 那一行补上 `"process-session"`，一个词。
唤星今天的绕法是在**消费方**声明同一个 feature（一条我们一行都不 `use` 的
`process-wrap` 依赖），rev 一字未动；上游修掉之后那条依赖可以删。

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

## 上游同步记录

> 每合一次上游就在这里加一节，**不要覆盖上一节**——「上次同步到哪」是判断 patch 老化速度的唯一依据。

### 2026-09-22：第一次上游同步（建仓后 18 天）

📌 **月度 rebase 此前一次都没发生过，这是第一次。** 且它不是 rebase 是 **merge**：
`hasn` 已 push 到 `origin`，rebase 会改写已发布历史，而父仓 `CLAUDE.md` §3 禁 force-push。

| 项 | 值 |
|---|---|
| 上游 HEAD | `96009644e1a6ef75b97d15560419d9456a005d46`（2026-09-22 04:17:16 +0000，`feat(recipe): enforce parameter limits (#12259)`） |
| `main` ff-only 后 | 同上（`5e9092596` 是它的祖先，真 fast-forward） |
| `hasn` 合并提交 | `bb4451fa0b1f759318b6d8ec69bc9561318a595e` |
| 上游提交数 | **101**，`374 files changed, 38350 insertions(+), 14579 deletions(-)` |
| 冲突 | **0 个**（`git diff --name-only --diff-filter=U` 空集） |
| 上游版本 | `1.49.0` → **`1.51.0`** |
| `edition` / `rust-version` / `rust-toolchain.channel` | `2021` / `1.94.1` / `1.96.1`，**三项一字未动** |
| workspace crates | **仍是 15 个**，集合一字未变（没有新增也没有删除） |

#### 薄 patch `#1` 的去向：**还在，且仍然有效**——⛔ 不是「机械 resolve 出来的」

上游**没有**提供等价能力：`git grep 'create_session_with_id\|create_with_id' upstream/main` 零命中，
`SessionStorage::create_session` 的 id 仍在 SQL 里现算 `YYYYMMDD_N`，签名与函数体一字未动。

之所以 0 冲突，是因为上游这 101 笔里只有 **2 笔**碰过 `session_manager.rs`
（`d213a3b13` live voice、`7d446c848` session 目录 0700），净 `+125/−1`，四个 hunk 分别落在
`:460` / `:938` / `:2010` / `:2860`，**与我们两处插入点（`create_session` 之后、
`SessionStorage::create_session` 之后）和测试模块尾部都不相邻**。

⚠️ **「merge 干净」不等于「语义相容」**，所以另外核了两条会让这条 patch 静默失效的：

- `sessions` 表**没有**新增列（我们那条 `INSERT` 写死七列，多一个 `NOT NULL` 就当场炸）；
- `migrate_to_version` 的 `match` **没有**新增分支。

判据全部重跑见上文「`#1` 的最小性与零影响」表（`166 insertions(+), 0 deletions(-)`、
`^-[^-]` 计数仍为 `0`、diff 仍只有两个文件）。两条单测在合并后原样重跑：

```
$ cargo test -p goose --lib session::session_manager::tests::create_session
test session::session_manager::tests::create_session_still_generates_its_own_dated_id ... ok
test session::session_manager::tests::create_session_with_id_keeps_the_caller_id_and_rejects_a_duplicate ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 2301 filtered out
rc=0
```

⚠️ **rc 要单独抓**：接 `| tail` 之后拿到的是 `tail` 的退出码（本机 zsh 连
`PIPESTATUS` 都是空的），⛔ 别拿管道尾的 0 当测试通过。

#### 🔴 对消费方的**唯一**破坏性变更：`Agent::reply` 多了一个参数

`b8b17c626 feat(agent): route Desktop to state-machine loop via ACP prompt meta (#11247)`：

```rust
// 旧（af1e505e，agents/agent.rs:2020）
pub async fn reply(&self, user_message: Message, session_config: SessionConfig,
                   cancel_token: Option<CancellationToken>) -> …
// 新（96009644，agents/agent.rs:2044）
pub async fn reply(&self, user_message: Message, session_config: SessionConfig,
                   use_state_machine: bool,                      // ← 新增，插在第 3 位
                   cancel_token: Option<CancellationToken>) -> …
```

`state_machine::enabled()` 本身**一字未改**（仍读 `GOOSE_STATE_MACHINE`，缺省 `false`，
`agents/state_machine/mod.rs:72`，`pub mod state_machine` 仍是公开的）——上游只是把
「读环境变量」从 `reply` 内部**提到了调用方**。⇒ 想保持旧语义，传
`goose::agents::state_machine::enabled()`；想确定性关掉，传 `false`。
上游自己的 CLI 走的是前者（`goose-cli/src/session/mod.rs:1456`）。

**除它之外，我们消费的那一整片公开面一字未动**（逐条现读核过）：`AgentConfig::new` 六参、
`extend_system_prompt` / `remove_system_prompt_extra`、`PromptManager.system_prompt_extras`
（仍是 `IndexMap`，有序）、`SessionManager::new` / `get_session`、`PermissionManager::new`、
`AgentEvent` 五格、`ApiClient::new_with_tls`、`OpenAiCompatibleProvider::new`、
`update_provider` / `recreate_provider_for_session`、`ExtensionConfig::StreamableHttp` 的字段集、
`add_extension` / `add_extensions_bulk` / `list_tools`、`submit_tool_confirmation`、
`GooseMode` 四格、`Paths::get_dir` 的 `GOOSE_PATH_ROOT`（`config/paths.rs` 本轮**零改动**）、
`SessionConfig` / `GoosePlatform` / `SessionType` / `AuthMethod` / `ModelConfig::new`、
`goose::session::mod` 与 `goose::config::mod` 的 re-export（两份文件**逐字未动**）。

⚠️ **另有一处公开面收缩，我们恰好没在用**：`goose::agents` 的 re-export 去掉了
`MCP_PROTOCOL_VERSION`（`agents/mod.rs:29`）。`hasn-node` 全仓零命中 ⇒ 不受影响。
⛔ 但别把「没影响」记成「没变化」——下次谁想用它会发现它不在了。

⚠️ 还有一处**只在非默认 feature 后面**的变化：`ExtensionError::InitializeError` /
`ProcessExit` 的载荷从裸类型换成了 `Box<…>`（`extension.rs`）。两个 `From` impl 补上了，
所以 `?` 照常；**但按变体解构那两个错误的代码会红**。我们今天不解构它们。

#### 依赖形态：两条决定我们仓依赖形状的，**都没变**

| 问题 | 现读 | 结论 |
|---|---|---|
| `crates/goose` 的 `sqlx` | 仍是 **`0.9.0`**，`Cargo.lock` 里 `libsqlite3-sys` 仍是 **`0.37.0`** | 🔴 **不能把 `rusqlite` 升回 `0.40`**。`libsqlite3-sys` 带 `links = "sqlite3"`（全图唯一），`sqlx 0.9.0 → libsqlite3-sys <0.38.0` 这条上界一字未松，我们整仓那次退版（`rusqlite 0.39.0` / `libsqlite3-sys =0.37.0`）**必须原样保留** |
| `crates/goose` 的 `[features] default` | 仍是 **`[]`** | ✅ `default-features = false` 免裁剪的前提成立。本次只新增了一个 **opt-in** feature `live-voice`（并被加进 `portable-default`）；⛔ **没有**任何重依赖被挪进 `default` |
| `crates/goose-providers` 的 `[features] default` | 仍是 **`[]`** | ✅ 同上 |

🔴 **上游本轮新长出一条出站传输，今天只被 `default-features = false` 挡在门外。**
`goose-providers` 新增了 `tokio-tungstenite`（WebSocket 客户端，`openai_live.rs` +1265 行、
`openai_live_voice_provider.rs` +425 行），它挂在 **opt-in** 的 `live-websocket` 后面，
而 `live-websocket` 只被 `goose/live-voice` 打开、`live-voice` 只在 `portable-default` 里。
⇒ 消费方保持 `default-features = false` 时它**不进依赖树**。

⚠️ 这正好是 `hasn-node` `modules/runtime-host/goose/Cargo.toml` 里那句
「上游哪天把某个 feature 挪进 `default`，这一行就是那道拦截」**第一次真的拦到东西**。
⛔ 别把 `default-features = false` 当成可有可无的形式——删掉它，daemon 依赖树里就会
多出一个 WebSocket 出站客户端，而 `check-goose-outbound-isolation.py` 判的是**直接**依赖
与直接 `use`，**判不到这条传递依赖**（那正是它头注登记的缺口 16）。

#### `process-wrap` 那条上游 bug：**上游没修**，绕法照旧

`crates/goose/Cargo.toml:217` 仍是 `process-wrap = { version = "9", default-features = false, features = ["std"] }`，
而 `agents/platform_extensions/developer/shell.rs:216` 仍逐字
`use process_wrap::std::{CommandWrap, ProcessSession};`。

实测（2026-09-22，合并后）：

```
$ cargo check -p goose
error[E0432]: unresolved import `process_wrap::std::ProcessSession`
  --> crates/goose/src/agents/platform_extensions/developer/shell.rs:216:42
note: found an item that was configured out … gated behind the `process-session` feature
error: could not compile `goose` (lib) due to 1 previous error
rc=101
```

⚠️ **为什么本仓自己的 workspace 构建照样绿**：`crates/goose-mcp/Cargo.toml:41` 声明了
`features = ["std", "process-session"]`，cargo 的 feature 合一把那一格捎带打开了。
而 `crates/goose` 对 `goose-mcp` 的那条依赖在 **`[dev-dependencies]`**（`Cargo.toml:270`），
resolver 2 在只编 lib 时**不做 dev-dep 的 feature 合一** ⇒ `cargo check -p goose` 红、
`cargo test -p goose` 绿。**把 `goose` 单独当依赖拉出来的消费方（我们）永远落在红的那一侧。**

⇒ `hasn-node` 消费方补声明同一个 feature 的绕法（`modules/runtime-host/goose/Cargo.toml`
那条一行都不 `use` 的 `process-wrap`）**继续需要，不能删**。这仍是应当回馈上游的一条
（⚠️ 与 `#1` 的 PR 一样，对外动作要主人授权，尚未提交）。

#### 建仓基线那节里两处坐标**已随本次同步过期**（登记，不回改历史）

- `agent.rs` 里 platform tool 的分发坐标 `:4369` / `:4407`：现跑
  `rg -n manage_schedule crates/goose/src/agents/agent.rs` **零命中**，先例整体在
  `platform_extensions/scheduler.rs`。**施工时按当时源码重新定位**这条纪律照旧成立；
- 「`goose-agent` 可能是上游演进方向，每次 rebase 都要重看」这一条**本次重看结论**：
  它仍是独立 crate（`events/inference/lib/machine/operation/tool` 六个文件），本轮只动了
  `inference.rs`（`+48/−47`）；**`Agent::reply` 仍在 `crates/goose/src/agents/agent.rs`，没有搬家。**

### 2026-09-22 晚：上游又走了 1 笔，**本次有意不合**（登记，不是漏了）

| 项 | 值 |
|---|---|
| 上游新 HEAD | `bfbbf4463`（`fix(acp): prevent duplicate schedules from overwriting recipes (#12434)`） |
| `main`（上游镜像）| 已 ff 到 `bfbbf4463`，并**首次推上 origin**（此前 `origin/main` 落后 102 笔，从建仓起就没推过） |
| `hasn` | 仍停在 `b949bbcde`，**没有合这一笔** |
| `origin/hasn` | 已推到 `b949bbcde`（主人 2026-09-22 授权推 fork） |

🔴 **不合的理由是时机，不是内容**：合 `main` 进 `hasn` 会改**工作树**，而当时
`hasn-node` 有两片在制 agent 正把本仓当**只读参照**在读坐标
（`extension_manager/mod.rs::add_client`、`api_client.rs::AuthMethod`）。
改工作树会让它们读到的行号漂掉，而那种错**不会红**，只会让施工照着错坐标做。

📌 **触发条件**：那两片落地后，连同**下一次 `rev` bump 一起做**——
合上游、编一遍、bump `hasn-node` 的 5 行 `rev =`，是同一批事。
⚠️ 分开做没有收益：一次上游合并不编译就等于没验证，而编译只发生在 bump 那一刻。

⚠️ **`rev` 钉的 `855d73e4` 仍在 `origin/hasn` 的历史上**（是 `b949bbcde` 的祖先）
⇒ `cargo fetch` 取得到，本次推送**没有**动 `hasn-node` 的任何构建输入。

⛔ **本次没做的，如实记**：上游那一笔**没有编译验证**（本仓一行代码没动，
工作树与 `855d73e4` 之间只差一个 docs 提交）；薄 patch `#1` 对 `bfbbf4463` 的相容性
**未核**——归 bump 那一片，按上一节那张表逐条重跑。

## 消费方

`hasn-node` 经 cargo git 依赖消费本仓，`rev` 钉死、**不跟分支**：

```toml
# modules/runtime-host/goose/Cargo.toml（2026-09-22 现读）
goose                = { git = "https://github.com/youngshunf/goose.git", rev = "<见下表>", default-features = false }
goose-providers      = { git = "https://github.com/youngshunf/goose.git", rev = "<见下表>", default-features = false }
goose-provider-types = { git = "https://github.com/youngshunf/goose.git", rev = "<见下表>" }
```

| `hasn-node` 分支 | 钉的 rev | 说明 |
|---|---|---|
| `main` | `af1e505e` | 建仓基线（RT-0） |
| `feat/goose-k2b-session-id`、`feat/goose-k3-k4-prompt-and-retire` | `4f1b751c` | 带薄 patch `#1`，**未合回 main** |

📌 **2026-09-22 订正**：本节原写 rev 是 `da0ee4c3…` 并标「现读」。`da0ee4c3` 确实是本仓一个真提交
（同一条 K2b patch 的**被 rebase 掉的前身**，commit message 一字不差），但 `git grep da0ee4c3` 在
`hasn-node` 任何分支上**零命中**——照它去核对会核出一条不存在的依赖。⚠️ 抄 rev 要从消费方的
`Cargo.toml` 现读，⛔ 不要从本文件反向抄。

📌 **2026-09-22 订正**：本节原写「**只引 `goose` 一个 crate**」——那是 RT-0 建仓时的
预期。`K2` 真内核进编译图之后是**三个**（白名单在 `hasn-node`
`scripts/guards/goose-crates.allow`，由 `check-goose-outbound-isolation.py` 判），
其中只有 `goose-providers` 会真的发出站请求。⚠️ 取回协议是 `https://`（`K1` 改），
⛔ 不是 `ssh://`。

本 fork 是**公开**仓，因此所有构建者（开发者本机、打包机、CI、云端镜像）**零凭据**即可拉取。
它同时是父仓 `CLAUDE.md`「Rust 版本策略」里那道**升级门的构建期验证对象**——
升级 toolchain 前在目标版本下把本仓 `hasn` 分支编一遍。
