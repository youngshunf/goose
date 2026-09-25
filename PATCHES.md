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

### 🔴 2026-09-23 裁决：批准**第二条**内核逻辑薄 patch（`#2`），⛔ 同样不构成放宽

`#2`（嵌入方显式指定上下文文件名）同样既不是 feature 裁剪也不是平台工具挂载点，
由验收方 2026-09-23 **单条裁决**批准。理由是：不修就是一条绕过嵌入方提示词权威的通道，
而不打 patch 的两条路都给不出**可判**的保证。

- **通道**：内核每轮 `agents/reply_parts.rs:233` 无条件 `.with_hints(working_dir)`，
  工具调用之后经典循环 `agents/agent.rs:3278` 再 `load_subdirectory_hints`、状态机
  `agents/state_machine/ops_toolcalling.rs:778` 另起一个 `SubdirectoryHintTracker::new()`；
  三条路的文件名表都来自 `hints/load_hints.rs::get_context_filenames()` ⇒
  `Config::global()` 的 `CONTEXT_FILE_NAMES`（缺省 `[.goosehints, AGENTS.md]`），
  向上走到 git 根、展开 `@引用`（坐标均为 `855d73e4`）；
- **为什么在唤星是真问题**：系统提示词的权威在节点（父仓 ADR
  `2026-09-22-Runtime内核自持推理循环与会话.md` **E2**，片段按 key 注入与撤销）。
  `hasn-node` `K8-1` 起内核会话 cwd 是主人可见的分身工作目录，`K8-2` 起分身能
  `write`/`edit` 它 ⇒ **分身写下的一个 `AGENTS.md` 就是它自己下一轮的系统提示词**，
  节点的装配器与撤销语义都管不到它；
- **不打 patch 的两条路都被挡死**：① 拉起方设环境变量——进程内测试与云端容器 entrypoint
  都覆盖不到，是**不可判**的保证；② 写全局配置文件——落点是 `Paths::config_dir()`，
  `GOOSE_PATH_ROOT` 缺席时在**主人 home** 下。`Config::global()` 只有配置文件与环境变量两层，
  **没有进程内覆盖层** ⇒ 只能让嵌入方在装配面上显式传。

⛔ **这条裁决只覆盖 `#2` 这一条。** 第三条内核逻辑 patch 仍要单独裁决，
⛔ 不得引用本段或上一段当作「内核逻辑可以改了」。

### 2026-09-23：本任务按零 fake 不变量实施内核逻辑薄 patch（`#3`），⛔ 不构成放宽

`#3` 由 `hasn-node` LLM 线的本机 E2E 提出，本任务的实施范围只覆盖 provider 终态错误。
relay 404 重试耗尽后，经典循环把 `ProviderError` 压成一条普通 assistant 文本再 `break`；
嵌入方只能读到正常 `Final`，派发被记成 `succeeded`。这违反「故障显式进入 UI 或日志，
不得用正常产出掩盖失败」。上游已有 `Message::from_provider_error` 与
`persist_and_push_message_with_id`：`Authentication` 分支及状态机路径已用它们产出带 kind
的 `Error` 块，而 `NetworkError` 与通配错误分支未用。只把这两支改成同形，零新 API，
不动拒答、压缩及消息转换函数。⛔ **这次实施仅覆盖 `#3`**，不得外推为允许修改其他内核逻辑；
未发生主人另行批准 `#3` 的单独裁决。

施工分支 `feat/provider-error-block`；worktree 为父仓根目录的
`.worktrees/goose-fork-provider-error`；目标仓 `hasn-apps/goose`，主分支 `hasn`。

### 2026-09-24：`#4` 施工登记

施工分支 `feat/provider-no-retry`；worktree
`/Users/mac/openclaw-workspace/huanxing/huanxing-project/.worktrees/goose-fork-provider-no-retry`；
目标仓 `hasn-apps/goose`，目标主分支 `hasn`（起点 `88bd1ee1f306f78b81d26ea146a269a1a7b20c53`）。
本片只改 fork，不合回 `hasn`、不推送；实现与证伪结果见下方 `#4` 登记。

### 2026-09-24：`#5` 施工登记

施工分支 `feat/provider-empty-turn-no-replay`；worktree
`/Users/mac/openclaw-workspace/huanxing/huanxing-project/.worktrees/goose-fork-empty-turn`；
目标仓 `hasn-apps/goose`，目标主分支 `hasn`（起点 `42e3bc09b6061ea398e92bfe12cebe1153bd82e6`）。
本片只改 fork，不合回 `hasn`、不推送；依据父仓 §2 非幂等调用及 §1 零 fake 铁律，
不记作主人单独批准。

## 改动登记

| # | 改动点 | 类别 | 理由 | 上游回馈可能性 |
|---|---|---|---|---|
| 1 | `crates/goose/src/session/session_manager.rs`：新增 `SessionManager::create_session_with_id` 与其存储层同名实现（**纯新增 166 行，0 删除**） | 内核逻辑（经 2026-09-22 单条裁决，见上） | 既有 `create_session` 的 id 是**在 SQL 里现算**的 `YYYYMMDD_N`，公开面上没有任何一条路能用调用方给的 id 建会话；而 `Agent::reply` 紧接着 `get_session(&session_config.id, true)`（`agents/agent.rs`），查不到就 `Err`。⇒ 嵌入 goose 的宿主（唤星 daemon 已有自己的 `RuntimeSessionId`）**无法让 `SessionConfig.id` 等于自己的会话身份**。另一条路（映射表）被 ADR E4 与施工文档 §2.7 同时挡死 | 🟢 **高**。让嵌入方自带 session id 是通用需求，与唤星业务零耦合。上游 PR 材料见下节，⚠️ 尚未提交（对外动作需主人授权） |
| 2 | `crates/goose/src/agents/agent.rs`（`AgentConfig.context_file_names` ＋ `with_context_file_names`；`Agent::with_config` 与状态机装配各转交一次）、`agents/prompt_manager.rs`（`PromptManager::with_context_file_names`，`with_hints` 读它）、`agents/state_machine/ops_toolcalling.rs`（`ToolExecutionOperation::with_context_file_names`）、`hints/load_hints.rs`（`SubdirectoryHintTracker::with_context_filenames`）：**嵌入方显式指定上下文文件名**（**+483 / −8**，其中生产代码 **+86 / −8**、测试 +397；删除的 8 行全是 4 处被替换的原表达式，每处的 `None` 分支逐字就是原表达式） | 内核逻辑（经 2026-09-23 单条裁决，见上） | 内核读 hints 的三条路文件名表全部来自 `Config::global()`，**没有进程内覆盖层**；嵌入方持有系统提示词权威、agent 又能写工作根时，agent 写下的 `AGENTS.md` 就是它自己下一轮的系统提示词。环境变量与写全局配置两条路都给不出可判的保证 | 🟢 **高**。「嵌入方决定读哪些上下文文件」是通用需求，缺省行为逐字节不变，与唤星业务零耦合。上游 PR 材料见下节，⚠️ 尚未提交 |
| 3 | `crates/goose/src/agents/agent.rs`：`ProviderError::NetworkError` 与通配 `Err(ref provider_err)` 两支在 `break` 前改用现成 `persist_and_push_message_with_id(…, Message::from_provider_error(provider_err))`，生产仅 **+16 / −10**；测试在 `agents/state_machine/tests/provider_errors_lifecycle.rs`，模块登记在 `tests/mod.rs` | 内核逻辑（本任务据零 fake 不变量实施，见上；不是新增的主人单条裁决） | relay 404 重试耗尽时普通 assistant 文本被嵌入方当 `Final`，派发假成功；身份验证错误与状态机路径已有 `Error` 块先例。只改两支调用、零新 API；不动拒答及 compact | 🟢 **高**。经典循环对齐已有状态机行为，不含唤星业务。上游 PR 材料见下节，⚠️ 尚未提交 |
| 4 | `crates/goose-providers/src/openai_compatible.rs`：实例级 `with_retry_config(RetryConfig)` 覆写 `Provider::retry_config()`；`api_client.rs`：实例级 `with_no_transport_retry()` 在所有客户端重建时复施 reqwest `retry::never()`；测试覆盖 provider、真实本地 HTTP/SSE、真实 h2 NACK 与经典空轮反例 | 内核接入的窄公开挂点（本任务依据父仓 §2 非幂等 POST 不自动重放及 §1 零假回落；**没有**主人另行单条裁决） | 本地 relay chat POST 未见稳定去重键与服务端保证；缺省 provider 额外重试 3 次，Agent 首流项前可额外重发，reqwest 0.13.5 默认协议 NACK 还可重发 2 次。两处逐实例装配，其他 provider/默认实例保持原行为；⚠️ 经典 200 空轮和认证刷新仍有独立重发，故 #4 **不等于**全链零自动重放 | 🟢 **高**。通用嵌入方按实例选重试策略，默认零变化；见下方 PR 草稿，尚未向上游提交 |
| 5 | `crates/goose/src/agents/agent.rs`：只在经典循环 `RetryResult::Skipped` 的空轮分支检查现有 `provider.retry_config().max_retries == 0`；首次空轮即通过既有 `persist_and_push_message_with_id` 发出并保存 `Message::assistant().with_error(MessageErrorKind::Other, EMPTY_TURN_MESSAGE)`，不再调用第二次 `Provider::stream`；原 #4 反例翻面并增默认及状态机对照 | 内核逻辑（据父仓 §2 非幂等 POST 不重发及 §1 零 fake 实施，**没有**主人另行单条裁决） | 200 成功空轮也是已发送的非幂等请求；零重试实例原额外发 3 次，且普通 assistant 文本被当正常答复。现闭集里 `Authentication`、`ContextLengthExceeded`、`CreditsExhausted` 均与事实不符，因此取 `Other`；`with_error` 为主人可见、模型不可见，错误在事件流和会话中均可判 | 🟢 **高**。与宿主业务无关，复用现有配置和错误消息接口；上游 PR 仅备材料，尚未对外提交 |

> 加一条 patch 就在上表加一行，**不要攒着**。评审判据是：
> 这张表的行数 == `git diff upstream/main...hasn` 里非裁剪类改动的处数。

### `#1` 的最小性与零影响，逐条可复跑

| 判据 | 命令 | 事实 |
|---|---|---|
| 纯新增，一行未删 | `git diff upstream/main...hasn -- crates/goose/src/session/session_manager.rs --stat` | `166 insertions(+), 0 deletions(-)` |
| 既有 `create_session` 的签名与函数体一字未动 | `git diff upstream/main...hasn -- crates/goose/src/session/session_manager.rs \| grep -c '^-[^-]'` | **`0`**（⚠️ 判据写成 `^-[^-]`，不是「没有以 `-` 开头的行」——`--- a/…` 那行就是以 `-` 开头的，照后者写会永远假红） |
| 现有调用方零影响 | `git grep -n 'create_session(' upstream/main -- '*.rs' \| wc -l` 与同一条打在 `hasn` 上 | 打 patch 前 **129**，之后 **130**（📌 2026-09-22 随上游同步重测，原记的是 `124`/`125`，**差的是上游自己新增的调用点，不是我们的 patch 变胖了**——增量恒为 `+1`）。多的那一处是本 patch 自己那条非真空对照单测；**原有 129 处一处未改**。⚠️ 这个 grep **不会**匹配 `create_session_with_id(`（后者另有 5 处，全是新增）。⚠️ 判据写成 `git grep`，⛔ 别写 `grep -rn … .`：本机 `grep` 被 alias 到 `ugrep`，从父仓根递归时会**跳过全部子仓**（在本仓根跑恰好还对，换个 cwd 就静默给 0） |
| 不动 schema / 不动迁移 | `git diff upstream/main...hasn --stat` | 只有 `session_manager.rs` 与 `PATCHES.md` 两个文件；`migrate_to_version` 的 `match` 分支一条未加。📌 2026-09-23 起同一条命令还会列出 `#2` 的 6 个文件（见 `#2` 的表）——`#1` 这一格改看 `-- crates/goose/src/session/`，仍只有 `session_manager.rs` |
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

### `#2` 的最小性与零影响，逐条可复跑

基点是 `f822c2276`（`#2` 之前的 `hasn`），`#2` 的实现提交是 `70deb52a7`。

| 判据 | 命令 | 事实 |
|---|---|---|
| 改动面 | `git diff f822c2276 70deb52a7 --stat -- crates/` | 6 个文件 **+483 / −8**：生产代码 4 个文件 **+86 / −8**（`agent.rs` +30/−6、`prompt_manager.rs` 非测试部分 +30/−1、`ops_toolcalling.rs` +14/−1、`load_hints.rs` +12/−0）；测试 +397（`prompt_manager.rs` 测试模块 +168、新文件 `state_machine/tests/context_files_lifecycle.rs` +228、`tests/mod.rs` +1） |
| 删除的只有 4 处原表达式，一个签名都没动 | `git diff f822c2276 70deb52a7 -- crates/ \| grep '^-[^-]'` | 恰好 **8** 行：`PromptManager::new()`（`Agent::with_config`）、`get_context_filenames()`（`with_hints`）、`SubdirectoryHintTracker::new()`（状态机 `prompt_parts`）、以及 `ToolExecutionOperation::new(…)` 那一个表达式被 rustfmt 为接上链式调用而重排的 5 行 |
| `None` 逐字是原行为 | 读 4 处替换 | `PromptManager::with_context_file_names(None)` ≡ `PromptManager::new()`；`None.unwrap_or_else(get_context_filenames)` ≡ `get_context_filenames()`；状态机 `None` 分支 ≡ `SubdirectoryHintTracker::new()`；`ToolExecutionOperation::new` 之后 `.with_context_file_names(None)` 字段仍是 `None` |
| 既有构造入口零改动 | 同上 diff | `AgentConfig::new`（六参）、`PromptManager::new`、`SubdirectoryHintTracker::new`、`ToolExecutionOperation::new` 的签名与函数体一字未动；`new()` 只多一行 `context_file_names: None` |
| 读 hints 的生产路径被穷举 | `git grep -n -e get_context_filenames -e 'SubdirectoryHintTracker::new' -e 'load_hint_files(' -e 'PromptManager::new' f822c2276 -- 'crates/*.rs'` | 生产读点恰好 3 条（`with_hints`、`PromptManager` 的跟踪器、状态机 `prompt_parts` 的跟踪器）；`PromptManager` 的生产构造点恰好 1 个（`agent.rs:445`）。其余命中全在测试与 `hints/` 自身 |
| 结构体字面量构造 `AgentConfig` 的调用方 | `git grep -n 'AgentConfig {' -- 'crates/*.rs'` | 只有 2 行：`pub struct AgentConfig {` 与 `impl AgentConfig {`——**0** 处字面量构造（全仓都走 `AgentConfig::new`）⇒ 新增 `pub` 字段不破坏任何现有构造 |

### `#2` 的行为契约

- **`None`（缺省，不调用 `with_context_file_names`）⇒ 与改前逐字节相同**；
- **`Some(names)` ⇒ 三条读路都只认 `names`**：每轮系统提示词的 `with_hints`（工作目录向上到
  git 根每一层、全局配置目录 `Paths::in_config_dir(name)`；`~/.agents/AGENTS.md` 仅当 `names`
  含 `AGENTS.md`）、经典循环工具调用之后的子目录 hints、状态机工具调用之后的子目录 hints；
  全局配置的 `CONTEXT_FILE_NAMES` 不再生效；
- **`Some(vec![])` ⇒ 三条路一个 hints 文件都不读**，system prompt 与「没有 hints」逐字节相同；
- ⚠️ **不覆盖**：`summon` 派生的子 Agent 自建 `AgentConfig`，**不继承**本字段
  （唤星不挂 `summon`，`hasn-node` 的挂载闭集判着）；`PromptManager::new()` 构造时仍读一次全局
  `CONTEXT_FILE_NAMES`（`SubdirectoryHintTracker::new()`），`Some` 时结果被替换丢弃——只读不写，
  不影响提示词；
- 📌 **`@引用` 拉不进 `.git/` 元数据，这是上游既有能力，不是本 patch**
  （`hints/import_files.rs::git_metadata_directories`，上游单测
  `project_git_metadata_does_not_reach_system_prompt`）。本 patch 关的是 hints 文件本身及其
  `@引用` 的**全部**合法目标。

判据（`cargo test -p goose --lib`）：

- `agents::prompt_manager::tests` 三条：`explicit_empty_context_file_names_read_no_hint_files`
  （空表 ⇒ 输出逐字节等于 `"BASE"`，子目录跟踪器读不出）、
  `explicit_context_file_names_replace_the_global_list`（非空表只认表内）、
  `unset_context_file_names_match_the_global_config_behaviour_byte_for_byte`
  （`None` 与 `PromptManager::new()` 都逐字节等于**照改前代码逐步复原**的参照物；
  参照物一步都不经过被改过的 `with_hints`；含非真空前提）；
- `agents::state_machine::tests::context_files_lifecycle` 两条，**经 `Agent::reply` 跑两条循环**
  （`use_state_machine` 取 `false` / `true`），判 provider 真收到的每一次 system prompt：
  git 根、工作目录 `AGENTS.md`、`.goosehints`、`@引用`、工具调用后才加载的子目录 `AGENTS.md`
  五处——空表 ⇒ 一处都没有；不指定 ⇒ 前四处首轮就在、第五处工具调用后出现（非真空对照）。

**它们被证伪过**（2026-09-23，`cargo test -p goose --lib -- context_file`，即上面 5 条 ＋ 上游 1 条；每条变异单独一次编译，跑完 `git checkout --` 还原，源码已先提交）：

| 源码 | 结果 |
|---|---|
| 原样（`70deb52a7`） | `ok. 6 passed; 0 failed`，同一二进制连跑 3 次、FM4 还原重编后再跑 1 次，全部 rc=0 |
| **FM1** `with_hints` 无视嵌入方的表（`.map(\|_\| get_context_filenames())`） | rc=101，3 条红：两条空表/非空表单测，与两循环用例的空表那条——失败行逐字「state_machine=false 第 0 次推理：嵌入方给的是空表，hints 文件却进了 system prompt：[PARENT…, WORKING_DIR_AGENTS_MD…, WORKING_DIR_GOOSEHINTS…, AT_IMPORTED_FILE…]」 |
| **FM2** `PromptManager::with_context_file_names` 不换子目录跟踪器 | rc=101，3 条红：两条单测（`load_subdirectory_hints` 在空表下读出了东西）＋ 两循环用例「state_machine=false 第 1 次推理：…[NESTED_SUBDIRECTORY_AGENTS_MD…]」——**只有工具调用之后那一次**漏 |
| **FM3** `Agent` 的状态机装配改传 `.with_context_file_names(None)` | rc=101，两循环用例红在**状态机那一支**：「state_machine=true 第 1 次推理：…[NESTED_SUBDIRECTORY_AGENTS_MD…]」；三条 `PromptManager` 单测照绿（它们判不到装配，正说明两循环用例不可省） |
| **FM4** `None` 不再是原行为（`with_hints` 的缺省分支改成 `unwrap_or_default()`，即空表） | rc=101，2 条红：逐字节用例（`left == right`）＋ 两循环用例的缺省那条「state_machine=false：缺省行为丢了 PARENT_GIT_ROOT_AGENTS_MD…：[]」 |
| **FM5** `Agent::with_config` 不转交（仍 `PromptManager::new()`） | rc=101，两循环用例空表那条红：「state_machine=false 第 0 次推理：…[四处工作目录 hints]」 |

🔴 **第一轮变异里撞到一条真的假红，已修，记下来**：FM3、FM5 与第一次「原样」复跑时，
逐字节用例也红了——而那两条变异根本不碰 `prompt_manager.rs`。失败正文里多出来的是
`### Global Hints … Global agents home instructions`：上游用例
`hints::load_hints::tests::test_global_agents_md_skipped_when_not_in_context_file_names`
**裸 `std::env::set_var("GOOSE_PATH_ROOT", …)`、不持锁**，与本用例并行时，本用例前后两次读到了
不同的全局 hints 块。⇒ 逐字节用例比较前剥掉全局块（`without_global_hints`，理由写在它的头注里），
⛔ 没有改上游那条用例。修后原样连跑 4 次全绿，FM4 在修后的用例上复验仍红（上表 FM4 行即修后结果）。
⚠️ FM1–FM3、FM5 跑在修竞态之前的那一版（`0c67aa76f`，未推送、已被 amend 掉）上；
它们各自红的那几条（两条单测与两循环用例）在两版之间**一字未变**，变的只有逐字节用例的比较方式。

### `#3` 的最小性与行为契约

- **最小性**：以 `8f1ef1db1` 为基点，生产只改 `agents/agent.rs` 的
  `NetworkError` 与通配 `Err(ref provider_err)` 两支（**+16 / −10**）；沿用
  `Authentication` 原有的 `persist_and_push_message_with_id` 与
  `Message::from_provider_error`。零新 API、零字段、零依赖；`Refusal`、
  `ContextLengthExceeded` 的自动压缩及 `from_provider_error` 本体**一字未动**。
  两支能在相同作用域访问 `session_manager`、`session_config.id` 和可变 `conversation`；
  `Authentication` 相邻分支现成编译、测试通过的调用形状就是非真空对照。
- **行为**：错误之后仍 `break`；事件流有一个 `MessageContentBlock::Error`，
  `MessageErrorKind::Other`（`NetworkError` / `RequestFailed` 在现有映射里均是 `Other`），
  消息逐字落会话且主人可见、下一轮模型不可见；人看的正文仍逐字等于改前两支。
  认证失败维持 `Authentication` kind。状态机路径原已走同一个转换函数，
  不改它的生产代码，但同组测试经两条 `Agent::reply` 路都核类型和落库。
- **不在本片**：`ProviderError::Refusal`、compaction、空回答和 max turns 仍可能
  不带 `Error` 块（原 `F-1` 余项）；非幂等 LLM POST 的重试与去重策略另片处理，
  此处只修**重试耗尽之后的错误载体**。
- 🔴 **剩余的非幂等重放缺口**：`goose-providers/src/openai_compatible.rs::stream_payload`
  将 `response_post(&payload)` + `handle_status` 放进 `with_retry`；HTTP 404 经
  `http_status.rs` 映成 `ProviderError::RequestFailed`，`RetryConfig::default()` 允许
  至多 3 次**额外**重放。它在 `Provider::stream()` 返回前发生，**不等于**
  `agents/reply_parts.rs` 只在首包错误时启用的另一层 `transient_only` 重试；
  空应答又有独立的 agent loop 重试。未见 NewAPI chat 的稳定去重键与服务端
  去重合同凭据；`#3` 不调整任何一层重试，也不宣称零重放，另片收口。

### `#3` 的红绿与变异

`cargo test -p goose --lib -- provider_errors_lifecycle`：先在生产代码未改时编译跑
`network_error_ends_the_turn_with_an_error_block`、
`other_provider_error_ends_the_turn_with_an_error_block`、
`authentication_error_ends_the_turn_with_an_error_block`：前两条各因
`state_machine=false` 收到普通 `Text`、Error 块数 `0` 而**红**（rc=101），
认证对照 **绿**。两支改完同一组 3 条 **全绿**（rc=0）。这是对旧代码的
反事实变异：取消两支的改动，恢复原有的 `Message::assistant().with_text(...)`
会被事件流断言抓住，且失败正文指向改动处。**测试夹具只向 `Provider::stream`
注入确定的错误以证伪消息类型，不冒称真实 relay 404 整链已验。** 真实 relay
404 的端到端结局归 `hasn-node` 的真实 E2E 场景，依主人对 E2E 的单独授权执行；
本片不运行。

### `#4` 的最小性、证伪与仍未闭的入口

- 只给 `OpenAiCompatibleProvider` 新增可选的实例配置，`new()` 的 `None` 仍走
  `RetryConfig::default()`（`max_retries=3`）；`stream_payload` 原有 `with_retry` 与
  `agents/reply_parts.rs` 首项读取都通过**同一个** `Provider::retry_config()` 取值，
  它们的重试条件与退避实现一字未改；其他 provider 不变。`ApiClient` 默认建造不调用
  `.retry()`，仍保留 reqwest 自带策略；仅调用 `with_no_transport_retry()` 的实例将
  `.retry(reqwest::retry::never())` 装入 client，后续 `with_header` / transport policy
  重建保持配置。**没有改变 `agents/agent.rs` 中 `#3` 两支或其它重试实现。**
- 最小消费方式：`ApiClient::new_with_tls(...)?.with_loopback_http_only()?.with_no_transport_retry()?`，
  再 `OpenAiCompatibleProvider::new(...).with_retry_config(RetryConfig { max_retries: 0, ..Default::default() })`。
  ⛔ 两处必须同时装配，仅设置后者挡不住 reqwest 的 protocol NACK。
- 测试先红后绿：`retry_policy_can_be_set_per_provider_without_changing_the_default`
  先因无 `with_retry_config` 编译红（E0277/E0061，rc=101），落实例覆盖后
  `3/0/3` 绿；真实 `TcpListener` 的 404/429/500 三种 `ProviderError` 均一次 POST，
  真实 200 SSE 首解析项前 `ServerError` 经 `stream_response_from_provider` 仍一次 POST，
  它证明 Agent 确实读到同一实例配置（不是只测 getter）。
- reqwest 0.13.5 的 h2 `REFUSED_STREAM` 对照：**默认 3 次**（首次 + 协议 NACK 额外 2 次），
  禁用实例 1 次，禁用后再 `with_loopback_http_only`/`with_header` 重建仍 1 次；
  真 `h2::server::handshake` 监听、每次 POST 复位，测试增量只加 dev `h2`（已有锁定包）。
  变异把**生产** `ApiClient::client_builder` 的分支改成 `if false && no_transport_retry`：
  同一用例实际 3、期望 1，rc=101；恢复后 rc=0。测试客户端用同一生产 builder
  追加 `.http2_prior_knowledge()` 才能在无 TLS 的真本地端口触发 h2，
  不在测试代码复制 `.retry(never)`；生产装配未加 h2 的默认配置。
- 307/308 的**边界对照不是跨仓证明**：本 fork 的真 loopback 响应无 `Location` 时
  `ApiClient` 不跟随且只发一次；node 的生产 `passthrough.rs` 回程调用
  `filter_response_for`，`headers.rs` 将 `location` 放在 `RESPONSE_STRIP` 且不在
  `RESPONSE_ALLOW` 闭集里。**node 侧真出站回程是否逐次剥除的测试/变异由 node 施工方负责，
  此处 not_run**；非 relay 用法若响应自带同源 Location，reqwest 仍可自动跟随
  307/308，本 patch 不改变它的全局 redirect 策略。
- 🔴 **仍未闭，不可宣称本地 relay 所有 POST 单次**：经典循环
  `agents/agent.rs` 的 200 空应答 `MAX_EMPTY_TURN_RETRIES=3` 与 `retry_config()` 无关。
  同组反例 `zero_retry_provider_still_replays_empty_successful_turn_in_classic_loop`
  将 provider `max_retries=0` 并送入成功空 stream，`Agent::reply(false)` **调了 4 次**，
  用例 rc=0，证明独立重放仍在；`ProviderRetry::with_retry_config` 遇到 401 且
  `refresh_credentials()` 成功时同样可独立重发一次（`retry.rs:199-216`，
  本片未做 Auth 成功刷新网络证据）。后两入口均由主会话另片判定/收口。
  `hasn-node` 的真实 relay E2E 属于主会话，未经本片运行；本片没有合回 `hasn` 或 push。

### `#5` 的最小性、行为与证伪

- 生产只改 `agents/agent.rs` 的一个空轮分支及 `MessageErrorKind` import，零新类型、
  零新 retry 配置、零其它重试分支；只有 `RetryResult::Skipped && empty_response` 且
  `Provider::retry_config().max_retries == 0` 才提前报 `Other` 错误并结束。保留原
  `EMPTY_TURN_MESSAGE` 的人类正文；不把普通 assistant `Text` 当失败载体。
  `MessageErrorKind` 现有闭集没有 EmptyResponse：此处不是认证、额度或上下文超限，
  因此只能选 `Other`。现成 `with_error` 设定主人可见、模型不可见；
  `persist_and_push_message_with_id` 同步落会话与内存消息，事件流也发该条消息。
- **范围边界**：默认 provider（`max_retries=3`）经典循环仍按固定
  `MAX_EMPTY_TURN_RETRIES=3` 发 **1+3** 次、保留原普通文本；recipe retry
  和 goal/grind 分支未动。状态机原只发一次，生产一字未动，用例锁住它。
  ⚠️ **仍未闭**：默认/其他非零配置的 Provider 自动重试仍在；401 成功
  `refresh_credentials()` 后是否补发（`goose-provider-types/src/retry.rs`）尚未收口。
  本片不能宣称所有 relay POST 都不重发；node 侧真 relay E2E 未运行。
- **先红后绿**：将 `#4` 的反例改为
  `zero_retry_provider_ends_empty_successful_turn_with_error_without_replay`，直接走
  `Agent::reply(false)`；旧代码成功编译、实际跑 **1 条** 后 stream 调用 **4≠1**
  而红（rc=101），不是 `--exact` 筛空 0 条（第一次误用 `--exact` 得 rc=0、
  `0 passed`，已明确作废）。修后同文件 6 条用例 rc=0：零配置 1 次、事件流恰
  1 个 `Error { kind: Other }` 且文案不变、纯文本 Final 消失、会话落库且主人可见
  模型不可见；默认经典 4 次及既有普通文本；状态机 1 次及既有普通文本；
  `#3` 的三种 provider 终态错误仍绿。
- **旧 #4 登记的时间口径**：上节 `#4` 的「仍未闭」及其 PR 草稿是
  `#4` 施工当时的证据，不能再当本分支现状；该空轮缺口现由 `#5` **只对零配置**闭合。
  默认/其他非零配置的空轮重放和认证刷新仍按上述边界保留。
- **变异**：仅把新判定 `== 0` 临时旁路为 `== usize::MAX`，同一零配置用例
  在真实运行中再次 **4≠1**、rc=101；还原源码再跑同文件 6 条，rc=0。
  `cargo fmt --all -- --check`、`cargo clippy -p goose --lib --tests -- -D warnings`
  与 `git diff --check` 均 rc=0，Clippy 输出包含 `Checking goose v1.51.0`。
  未运行全量 gate/E2E，也未合主 clone 或 push。

📌 影响面更宽的一轮 `cargo test -p goose --lib -- agents:: hints::`（跑在 `d628fd95f`，与 `70deb52a7` 只差一个测试辅助函数的写法——为过 clippy 的 `string_slice` 改成 `split_once`）：**624 passed / 1 failed**，
红的是 `agents::prompt_manager::tests::test_all_platform_extensions`，**与 `#2` 无关**：
快照里有 `## code_execution` 一节，而那个平台扩展挂在 `code-mode` feature 后面
（`platform_extensions/mod.rs:4`/`:146`），`cargo test -p goose` 不带它 ⇒ 快照缺那一节。
差异只有那一节；⚠️ 未在 `--features code-mode` 下复跑（`not_run`）。

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

### `#2` 的上游 PR 材料（⚠️ 尚未提交）

⛔ 与 `#1` 相同：**还没有向上游提任何 issue 或 PR**，对外动作要主人授权。
⚠️ 上游 `AGENTS.md` 的贡献流程要求「外部 PR 必须链接一个 Board 上状态为 **Ready** 的 issue」
⇒ 顺序是**先开 issue 讨论形状、等 Ready、再提 PR**，⛔ 别直接提 PR。
⚠️ 提交前把 `#2` 的中文 doc comment 与测试断言消息换成英文（下面给好了对应英文版）；
测试文件里的 `唤星 PATCHES.md #2` 字样一并删掉。
⚠️ 上游 `AGENTS.md` 逐字「changes to agent-loop behavior must be implemented and tested in
**both paths**」——本 patch 两条循环都改了、测了，PR 正文里要点明。

**目标仓**：`aaif-goose/goose`　**类型**：feature（附测试）　**预计 diff**：+483 / −8（生产 +86 / −8，其余是测试）

**先开的 issue**

```
Title: Let embedders choose which context files (AGENTS.md / .goosehints) the agent reads

When goose is embedded as a library, the host may own the system prompt and let
the agent write to its working directory. Today every reply calls
`with_hints(working_dir)` and, after tool calls, loads subdirectory hints; the
file names always come from `Config::global()`'s `CONTEXT_FILE_NAMES`. Config has
only two layers (config file + environment), so a host cannot turn this off for
its own Agent without mutating process-wide environment or writing the user's
global config file. As a result an `AGENTS.md` the agent writes becomes part of
its own next system prompt. Would you accept an additive
`AgentConfig::with_context_file_names(Vec<String>)` (unset = today's behaviour,
empty = read no context files) that covers `with_hints` and both subdirectory
hint trackers (classic loop and state machine)?
```

**PR 标题**

```
feat(agents): let embedders choose the context file names the agent reads
```

**PR 正文**

```markdown
### Problem

Context files (`.goosehints`, `AGENTS.md`) are read on three paths:

- every reply: `SystemPromptBuilder::with_hints(working_dir)`, walking up to the
  git root and expanding `@` references;
- after tool calls in the classic loop: `PromptManager::load_subdirectory_hints`;
- after tool calls in the state machine: `ToolExecutionOperation::prompt_parts`.

All three take their file names from `Config::global()`'s `CONTEXT_FILE_NAMES`.
`Config` has no in-process override layer, so an application embedding goose
cannot choose the file names for its own `Agent`: the only knobs are a
process-wide environment variable or the user's global config file.

That matters when the host owns the system prompt and the agent can write to its
working directory: an `AGENTS.md` the agent writes becomes part of its own next
system prompt, bypassing the host.

### Change

- `AgentConfig::context_file_names: Option<Vec<String>>` plus
  `AgentConfig::with_context_file_names(Vec<String>)`.
- `Agent::with_config` passes it to `PromptManager::with_context_file_names`,
  which `with_hints` and the classic-loop subdirectory tracker read, and to
  `ToolExecutionOperation::with_context_file_names` for the state machine.
- `SubdirectoryHintTracker::with_context_filenames(Vec<String>)`.

Unset (`None`) is byte-for-byte today's behaviour: each of the four replaced
expressions keeps the original expression as its `None` branch, and no existing
constructor signature changes. An empty list reads no context files at all.

Subagents created by `summon` build their own `AgentConfig` and do not inherit
the setting; happy to thread it through if you prefer.

### Tests

Both agent loops are covered.

- `prompt_manager::tests::explicit_empty_context_file_names_read_no_hint_files`
- `prompt_manager::tests::explicit_context_file_names_replace_the_global_list`
- `prompt_manager::tests::unset_context_file_names_match_the_global_config_behaviour_byte_for_byte`
  — the reference is rebuilt from the pre-change primitives and never goes
  through the modified `with_hints`.
- `state_machine::tests::context_files_lifecycle::*` — drives `Agent::reply` with
  `use_state_machine` false and true against the dummy provider and checks the
  system prompt it receives: with an empty list none of the five hint sources
  (git-root `AGENTS.md`, working-dir `AGENTS.md`, `.goosehints`, an `@`-imported
  file, a nested `AGENTS.md` loaded after a tool call) appears; unset, all of
  them do.
```

**提交前要替换的 doc comment（英文版，逐段对应）**

`AgentConfig::context_file_names`：

```rust
/// Context file names (hints) chosen by the embedder. `None` reads
/// `CONTEXT_FILE_NAMES` from the global config (today's behaviour);
/// `Some(vec![])` reads no context files. See
/// [`AgentConfig::with_context_file_names`].
```

`AgentConfig::with_context_file_names`：

```rust
/// Lets the embedder choose which file names count as context files (hints),
/// overriding `CONTEXT_FILE_NAMES` from the global config.
///
/// The same list governs all three places the agent reads hints: the
/// working-directory hints in every system prompt (walking up to the git root,
/// including the global config directory), subdirectory hints after tool calls
/// in the classic loop, and subdirectory hints after tool calls in the state
/// machine. An empty list reads no context files.
///
/// Leaving it unset keeps today's behaviour byte for byte.
///
/// Subagents created by `summon` build their own `AgentConfig` and do not
/// inherit this setting.
```

`PromptManager::context_file_names` / `ToolExecutionOperation::context_file_names`：

```rust
/// Context file names chosen by the embedder. `None` reads `CONTEXT_FILE_NAMES`
/// from the global config (today's behaviour).
```

`PromptManager::with_context_file_names`：

```rust
/// Lets the embedder choose which file names count as context files (hints).
///
/// - `None`: identical to [`PromptManager::new`]; file names come from
///   `CONTEXT_FILE_NAMES` in the global config (default
///   `[.goosehints, AGENTS.md]`).
/// - `Some(names)`: only `names` are used, for the working directory up to the
///   git root, the global config directory, and subdirectories
///   ([`SystemPromptBuilder::with_hints`] and
///   [`PromptManager::load_subdirectory_hints`]).
/// - `Some(vec![])`: no context file is read.
///
/// `Config::global()` has no in-process override layer, so a host that owns the
/// system prompt and lets the agent write to its working directory otherwise has
/// no way to stop an agent-written `AGENTS.md` from becoming its next system
/// prompt.
```

`ToolExecutionOperation::with_context_file_names`：

```rust
/// Restricts subdirectory hints loaded after tool calls to the file names chosen
/// by the embedder (the same list as `AgentConfig::context_file_names`). `None`
/// keeps today's behaviour; an empty list reads nothing.
```

`SubdirectoryHintTracker::with_context_filenames`：

```rust
/// Builds a tracker from context file names chosen by the embedder, without
/// reading `CONTEXT_FILE_NAMES` from the global config.
///
/// The only difference from [`SubdirectoryHintTracker::new`] is where the file
/// names come from. An empty list means no subdirectory ever yields hints.
```

测试辅助 `without_global_hints`（`prompt_manager.rs` 测试模块）：

```rust
/// Strips the global hints block (from `### Global Hints` up to `### Project Hints`).
///
/// The global block reads the process-wide `GOOSE_PATH_ROOT` / home, and
/// `hints::load_hints::tests::test_global_agents_md_skipped_when_not_in_context_file_names`
/// mutates `GOOSE_PATH_ROOT` with a bare `set_var` and no lock, so two reads in the
/// same test can see different global blocks when tests run in parallel. The
/// stripped block comes from the same `load_hint_files` call and the same file
/// name list as the project block, so a wrong list still breaks the comparison.
```

📌 那条上游用例裸 `set_var` 不持锁本身是上游的测试隔离缺陷（实测让本 patch 的逐字节用例
并行时偶发红，见上面变异表下的说明）。PR 里可以顺带提一句，⛔ 不在本 patch 里改它。

### `#3` 的上游 PR 材料草稿（⚠️ 尚未提交）

⛔ 此处只是草稿：**没有向上游提 issue 或 PR**；对外动作仍需主人另行授权，
还须按上游 `AGENTS.md` 的 Board **Ready** issue 流程。向上游提交时把本仓
中文测试说明与断言文本改成英文；只摘取本条差异，不混入 `#1`、`#2`。

**目标仓**：`aaif-goose/goose`　**类型**：bug fix　**范围**：经典循环两条分支 + 双循环回归

**issue 标题与正文草稿**

```text
Title: Legacy agent loop reports terminal provider errors as successful assistant text

When the legacy Agent::reply loop receives a NetworkError or another provider
error after retries are exhausted, it yields a plain assistant text message
and ends the turn. Embedders cannot distinguish that from a successful answer
without matching prose. Authentication errors already use a typed Error block;
the state-machine path uses Message::from_provider_error for provider errors.
Could we align the two legacy branches with these existing paths while keeping
their user-facing message text unchanged?
```

**PR 标题**

```text
fix(agents): emit typed Error blocks for terminal provider errors
```

**PR 正文草稿**

```markdown
### Problem

The legacy loop emits plain assistant text for `NetworkError` and its catch-all
provider-error branch, then breaks. Embedders observe a successful-looking
message even when no model answer was produced. The adjacent Authentication
branch and the state-machine inference path already emit typed Error content.

### Change

In the two legacy branches, use the existing `persist_and_push_message_with_id`
with `Message::from_provider_error`. This preserves the exact existing text for
both errors while exposing `MessageErrorKind::Other` to consumers and persisting
the message consistently with Authentication. No new API or message variant;
refusal, compaction, and provider retry policy are unchanged.

### Verification

Drive `Agent::reply` through both classic and state-machine paths with injected
NetworkError, RequestFailed, and Authentication errors. Assert exactly one typed
Error in emitted events, the expected kind and unchanged text, and persistence
with user-visible / agent-invisible metadata. Before the change, the first two
cases fail on the classic path while Authentication is a passing control.
```

### `#4` 的上游 PR 材料草稿（⚠️ 尚未提交）

⛔ 只作本地材料，不向上游发送 issue/PR；按上游 `AGENTS.md`，对外 PR 须先链接
Board 状态 **Ready** 的 issue。向上游提稿前把本仓新增的中文 doc comment/断言说明换成英文，
仅摘取 `#4` 的差异，不混 `#1`～`#3`。本片的根据是本任务的非幂等与零假回落边界，
**不虚构主人单独批准 `#4` 的裁决**。

**issue 草稿**

```text
Title: Let embedded OpenAI-compatible providers disable retries per instance

An embedding host may send non-idempotent chat/completions POSTs to a gateway
without a deduplication contract. The OpenAI-compatible provider retries failed
HTTP responses by default, the agent can retry a failure before its first stream
item, and reqwest itself retries HTTP/2 protocol nacks. A host currently cannot
turn off these retries for just one provider/client without changing defaults for
all other users. Would you accept two additive opt-in builders: provider-level
RetryConfig and ApiClient-level transport retry disablement? Unset keeps the
existing behavior. Auth refresh and the classic loop's 200-empty-turn retry are
separate concerns; this change deliberately does not claim to address them.
```

**PR 标题**

```text
feat(providers): allow embedders to disable retries per instance
```

**PR 正文草稿**

```markdown
### Problem

OpenAI-compatible chat completions may be non-idempotent. Today the provider
retries failed response statuses, the agent retries a transient error before
its first stream item, and reqwest may replay an HTTP/2 protocol nack. An
embedder has no per-instance option to disable all three for its local relay.

### Change

- `OpenAiCompatibleProvider::with_retry_config(RetryConfig)` overrides its
  existing `Provider::retry_config()` source. Both provider send and agent
  first-item processing already read that method; no retry algorithm changes.
- `ApiClient::with_no_transport_retry()` opts this client into reqwest
  `retry::never()` and preserves it through client rebuilds.
- Neither builder affects an unconfigured instance or any other provider.

### Verification and limits

A real loopback listener sees one POST per 404/429/500 status and per transient
in-stream failure before the first item. A real HTTP/2 listener sends
REFUSED_STREAM; the default client makes three requests, the opt-in client one,
even after a rebuild. Disabling the production retry branch fails that test
(3 observed versus 1 expected). A 307/308 *without Location* does not redirect;
this is a client-side conditional, not proof that any gateway strips Location.

The classic agent loop still retries an HTTP 200 empty turn three times even
with `max_retries=0` (four provider calls in a counterexample). Credential
refresh after 401 is also independent. Neither behavior is claimed as solved.
```

### `#5` 的上游 PR 材料草稿（⚠️ 尚未提交）

⛔ 只备本地材料，不对外发送 issue 或 PR；按上游 `AGENTS.md`，提 PR 前须先有
Board 状态为 **Ready** 的 issue。对外提交前将新增中文注释与测试断言说明译为英文，
仅摘取 `#5` 差异，不混入 `#1`～`#4`；未发生主人对 `#5` 的单独批准。

**issue 草稿**

```text
Title: Respect a provider's zero-retry policy for HTTP-200 empty agent turns

The classic Agent::reply loop retries an empty successful stream three times
regardless of Provider::retry_config().max_retries. An embedding host can disable
provider and transport retries for a non-idempotent chat POST, yet the classic
loop still sends it four times when the response body is empty. Would you
accept a narrow change that ends an empty turn immediately, with a typed Error
block persisted to the conversation, when the provider has max_retries=0?
Providers using the default retry config would retain the existing 1+3 path;
recipe retry, goal/grind, and state-machine behavior would remain unchanged.
```

**PR 标题**

```text
fix(agents): honor zero-retry providers on empty successful turns
```

**PR 正文草稿**

```markdown
### Problem

The classic agent loop retries a successful but empty model stream up to three
additional times even when the provider disables retries. These may replay a
non-idempotent chat POST; after exhaustion, the plain assistant text also looks
like a normal completed answer to embedding clients.

### Change

In the classic `RetryResult::Skipped` empty-turn branch only, check the existing
`Provider::retry_config().max_retries`. If zero, persist and emit an assistant
`Error` block of kind `Other` and end without another stream request. `Other` is
the existing kind for errors that are not authentication, context-length, or
credits failures. The existing user-facing text stays the same; `with_error`
marks the message user-visible and agent-invisible. Nonzero providers retain
the existing bounded retry logic. No new retry type or public API.

### Verification

Drive `Agent::reply` against a provider yielding an empty stream. Before the
change, the zero-retry case makes four calls and fails its one-call assertion;
afterward it makes one and emits exactly one persisted typed Error (not plain
assistant text). The default provider still makes four calls and preserves its
original fallback, while the state-machine path still makes one. Bypassing the
zero-retry branch makes the same test fail with four calls again.
```

## 待回馈上游

> ⚠️ 本节与上面那节**不是一回事**：上面那节是「我们已经打了 patch，材料备好等授权提 PR」；
> 本节是「**对应公开面还没打 patch**，而是希望上游加一个能力／修一个 bug」。
> `F-1` 原登记里的 provider 终态错误一半已由 `#3` 修补；余下的终局类型信号未补。
> ⛔ 本节任何一条都**还没有**向上游提过 issue 或 PR——对外动作要主人授权。
>
> **为什么不自己打公开面 patch**：其中两条动的是**上游的公开面**（一个公开枚举、一个 Cargo
> feature 声明）。薄 patch 纪律逐字「内核逻辑非改不可时**优先回馈上游**」，
> 而公开面的改动尤其要先跟上游谈——悄悄改一个公开枚举，每次 rebase 都要重打，
> 且上游哪天自己加了一个形状不同的等价物就变成两套。

### `F-1`（唤星侧代号 `A4`）：内核把**终态原因**压成了一句英文正文，嵌入方判别不了

**登记方**：唤星 `K5`（`hasn-node` `modules/runtime-host/goose/src/events.rs`，2026-09-22）。
**现读基线**：fork `hasn` 分支 `4f1b751c`（= 上游 `5e909259` + 我们的 `#1`）。
📌 上句是 **F-1 原登记时的历史基线**，不是 `#3` 的施工基点（`8f1ef1db1`）。
`#3` 已覆盖 F-1 同族中「`NetworkError` 与通配 provider 终态错误压成普通英文正文」
这一半：经典循环现在也发 `MessageContentBlock::Error`；`Authentication` 原本就发。
⛔ **尚未覆盖**：`InlineMessage` 里的压缩后终局、`MAX_TURNS`、空应答重试耗尽、
`Refusal` 等原因的类型化终局；下述原表是这些剩余问题的历史登记，仍待上游。

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

### `F-3`：`list_tools` 失败被内核吞成一条 `warn`，嵌入方拿不到具名失败

**现读**（`crates/goose/src/agents/extension_manager/mod.rs:840`）逐字：

```rust
Err(e) => { warn!(...); return (name, vec![]); }
```

⇒ 一个扩展的 `list_tools` 失败之后，内核把它**降级成空工具表**继续往下走，
嵌入方（我们）在公开面上**看不到任何失败**。对 CLI 用户这大概是对的：
一个扩展挂了不该让整轮聊天崩掉。

🔴 **对嵌入方不对**：唤星的判据逐字是「**工具面不可达 ⇒ 这一轮具名失败，
⛔ 不得静默降级成纯文本**」（`hasn-node` 施工文档 10 号 K6e 出口判据 ②）。
空工具表与「这个分身本来就没有工具」在内核公开面上**不可区分** ⇒ 我们只能
证明**调用**那一面的具名失败（`McpClientTrait::call_tool` 的 `Err` 会原样上浮），
**目录**那一面今天没有载体。

**建议的上游形状**（两条都不破坏 CLI 的现有行为）：
- 在 `AgentEvent` 流上多发一条「某扩展的目录拉取失败」的事件，正文里带扩展名与原因；
  或
- 给 `ExtensionManager` 一个 `strict_list_tools` 开关，开启时把 `Err` 上浮而不是降级。

⚠️ **我们侧不要先自己绕**：在 `add_client` 那一层缓存一份「上次拉成功过」再对比，
等于在嵌入方复刻一份内核的目录状态机——那是第二份事实源。
⇒ 在上游给出载体之前，这一格在 `hasn-node` 侧登记为缺口
（`scripts/guards/tool-host-wiring.json` 的 ⑨），⛔ 不是通过。

📌 与 `F-1` / `F-2` 一样：**本地 exploratory 实现已提交 `8221c90b2`，未向上游提交 PR，也未 push。** Node 尚未消费严格模式，仍需后续 rev/lock 接线与生产链验证。

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

### 2026-09-23：`#2` 那次 rev bump **没有**合上游（登记，不是漏了）

| 项 | 值 |
|---|---|
| 上游 HEAD（`git fetch upstream` 现读） | `201837dff`（2026-09-23 05:59 +0000，`fix(extensions): prevent Extension Manager from disabling itself (#12270)`），版本已到 `1.52.0` |
| `git rev-list --left-right --count hasn...upstream/main`（打 `#2` 前） | `7  11` |
| 本次 | `hasn` 只多了 `#2` 两笔（实现 ＋ 本文件登记），**一笔上游都没合** |

理由：`#2` 是 `hasn-node` `K8-2`（放开写盘）之前必须先堵的口子，派工单把基点钉在 `f822c2276`；
把 11 笔未审的上游（含一次 minor 版本跳）塞进同一次 rev bump，会让这一片的验收面
从「一条 patch」扩成「一条 patch ＋ 一次上游同步」，而后者按上一节的表要逐条重跑。
⇒ 上一节登记的「连同下一次 rev bump 一起合」**顺延到再下一次**，⛔ 不是作废。

⚠️ 一条对下一次同步有用的现读：`upstream/main` 上读 hints 的三处仍是 `prompt_manager.rs:87`
（`get_context_filenames()`）、`prompt_manager.rs:188`（`SubdirectoryHintTracker::new()`）、
`ops_toolcalling.rs:778`（`SubdirectoryHintTracker::new()`），与 `#2` 打的位置一致 ⇒ 预计无文本冲突；
**合完仍要按 `#2` 的最小性表重跑**，尤其「读 hints 的生产路径被穷举」那一行。

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
| `main` | `855d73e4` | 上游同步到 `96009644` ＋ 薄 patch `#1`（2026-09-23 现读 `main` 的 `modules/runtime-host/goose/Cargo.toml`） |
| `feat/goose-k8-coding` | `hasn` 上 `#2` 登记那一笔（**即本文件随之提交的那一笔**，写不进自己的 SHA——现读该分支的 `Cargo.toml`） | 再加薄 patch `#2`（`K8-1b`），**未合回 main** |

📌 **2026-09-23 订正**：本表原写 `main` 钉 `af1e505e`、`#1` 那两条分支钉 `4f1b751c`——
那是 `K2b` 刚落地时的事实；此后 rev-bump 片已把 `main` 推到 `855d73e4`（含 `#1`），
那两条分支在 `hasn-node` 已不存在（`git rev-parse --verify` 两条都查不到）。

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
