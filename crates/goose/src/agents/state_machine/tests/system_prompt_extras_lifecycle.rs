//! 嵌入方在**同一轮**里改 system prompt extras（唤星 `PATCHES.md` `#7`）的端到端判据。
//!
//! 场景：一次 `Agent::reply` 里模型先调一个工具，工具执行期间嵌入方经
//! `Agent::extend_system_prompt` 换掉一格 extras，模型再收尾。
//!
//! 判定面是 **provider 真收到的 system prompt**（`DummyApi` 记下的请求体）：
//!
//! - extras 变了 ⇒ 同一轮第二次推理的 system prompt 已是新正文、旧正文不在；
//! - extras 没变（同 key 同正文重写）⇒ 第二次推理**没有重建** system prompt。
//!
//! 「没有重建」怎么判：本文件的扩展每被问一次 instructions 就换一个 `<build-N>` 标记，
//! 而 instructions 只在建 system prompt 时被读。第二次推理仍是 `<build-1>` ⇒ 没重建；
//! 出现 `<build-2>` ⇒ 重建了。⛔ 不比对字段，比对上游真收到的字节。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::dummy_api::{DummyApi, ProviderFeatures};
use crate::agents::extension::ExtensionConfig;
use crate::agents::mcp_client::{Error as McpError, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::agents::{Agent, AgentConfig, GoosePlatform, SessionConfig};
use crate::config::permission::PermissionManager;
use crate::config::GooseMode;
use crate::conversation::message::Message;
use crate::providers::base::Provider;
use crate::session::{SessionManager, SessionType};
use goose_providers::model::ModelConfig;

const EXTRA_KEY: &str = "expert";
const EXPERT_A: &str = "EXPERT_A_SYSTEM_PROMPT_MARKER";
const EXPERT_B: &str = "EXPERT_B_SYSTEM_PROMPT_MARKER";
const SELECT: &str = "expert__select";
const TOOL_RESULT: &str = "expert switched";

fn build_marker(generation: usize) -> String {
    format!("<build-{generation}>")
}

/// 工具执行期间改 extras 的测试扩展：`select` 被调时把 [`EXTRA_KEY`] 写成 `write_on_call`。
struct ExpertSwitchExtension {
    info: InitializeResult,
    agent: OnceLock<Weak<Agent>>,
    write_on_call: &'static str,
    instruction_reads: AtomicUsize,
    selects: AtomicUsize,
}

impl ExpertSwitchExtension {
    fn new(write_on_call: &'static str) -> Self {
        Self {
            info: InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
                .with_server_info(Implementation::new(
                    "expert".to_string(),
                    "test".to_string(),
                )),
            agent: OnceLock::new(),
            write_on_call,
            instruction_reads: AtomicUsize::new(0),
            selects: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl McpClientTrait for ExpertSwitchExtension {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, McpError> {
        let schema = json!({ "type": "object", "properties": {} })
            .as_object()
            .unwrap()
            .clone();
        Ok(ListToolsResult::with_all_items(vec![Tool::new(
            "select",
            "Select an expert for this session",
            Arc::new(schema),
        )]))
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, McpError> {
        self.selects.fetch_add(1, Ordering::SeqCst);
        let agent = self
            .agent
            .get()
            .and_then(Weak::upgrade)
            .expect("测试必须先把 agent 交给扩展");
        agent
            .extend_system_prompt(EXTRA_KEY.to_string(), self.write_on_call.to_string())
            .await;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            TOOL_RESULT,
        )]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    /// 每读一次换一个标记 ⇒ system prompt 每重建一次，上游收到的字节就不同。
    fn get_instructions(&self) -> Option<String> {
        let generation = self.instruction_reads.fetch_add(1, Ordering::SeqCst) + 1;
        Some(build_marker(generation))
    }
}

/// 跑一轮「先调 `select`、再收尾」，交回 provider 两次推理各自收到的请求。
async fn run_turn(
    write_on_call: &'static str,
    use_state_machine: bool,
) -> Result<Vec<super::dummy_api::ApiCall>> {
    let api = Arc::new(DummyApi::start(ProviderFeatures::default()).await);
    let api_client = goose_providers::api_client::ApiClient::new_with_tls(
        api.uri(),
        goose_providers::api_client::AuthMethod::NoAuth,
        None,
    )?;
    let provider: Arc<dyn Provider> = Arc::new(
        goose_providers::openai::OpenAiProviderBuilder::new(api_client)
            .name("openai")
            .build(),
    );

    let temp_dir = tempfile::tempdir()?;
    let working_dir = temp_dir.path().join("work");
    std::fs::create_dir_all(&working_dir)?;
    let state_dir = temp_dir.path().join("state");
    let session_manager = Arc::new(SessionManager::new(state_dir.clone()));
    let session = session_manager
        .create_session(
            working_dir,
            "extras-rebuild".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await?;
    let config = AgentConfig::new(
        session_manager,
        Arc::new(PermissionManager::new(state_dir.join("permissions"))),
        None,
        GooseMode::Auto,
        true,
        GoosePlatform::GooseCli,
    )
    // 不读任何 hints 文件：排除「子目录 hints 触发重建」这条既有路径的干扰。
    .with_context_file_names(Vec::new());
    let agent = Arc::new(Agent::with_config(config));
    agent
        .update_provider(
            provider,
            ModelConfig::new(goose_providers::openai::OPEN_AI_DEFAULT_MODEL)
                .with_canonical_limits("openai"),
            &session.id,
        )
        .await?;
    let extension = Arc::new(ExpertSwitchExtension::new(write_on_call));
    extension
        .agent
        .set(Arc::downgrade(&agent))
        .expect("只设一次");
    agent
        .extension_manager
        .add_client(
            "expert".to_string(),
            ExtensionConfig::Platform {
                name: "expert".to_string(),
                description: "Expert switch test extension".to_string(),
                display_name: None,
                bundled: None,
                available_tools: vec![],
            },
            extension.clone(),
            extension.get_info().cloned(),
        )
        .await;
    agent
        .extend_system_prompt(EXTRA_KEY.to_string(), EXPERT_A.to_string())
        .await;

    api.on("pick an expert").call(SELECT, json!({}));
    api.on(TOOL_RESULT).reply("done with the new expert");

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: None,
        max_turns: Some(4),
        retry_config: None,
    };
    let mut stream = agent
        .reply(
            Message::user().with_text("pick an expert"),
            session_config,
            use_state_machine,
            Some(CancellationToken::new()),
        )
        .await?;
    while let Some(event) = stream.next().await {
        event?;
    }
    drop(stream);

    assert_eq!(
        extension.selects.load(Ordering::SeqCst),
        1,
        "state_machine={use_state_machine}：select 没被调到——下面的断言全是假绿"
    );
    let calls = api.calls();
    assert_eq!(
        calls.len(),
        2,
        "state_machine={use_state_machine}：应当恰好两次推理：一次出工具调用、一次收尾"
    );
    Ok(calls)
}

/// ⭐ 经典循环：工具执行期间换了专家 ⇒ **同一轮**第二次推理就是新专家，旧专家不在。
#[tokio::test]
async fn classic_loop_rebuilds_system_prompt_within_the_turn_when_extras_change() -> Result<()> {
    let calls = run_turn(EXPERT_B, false).await?;
    assert!(calls[0].system_contains(EXPERT_A));
    assert!(!calls[0].system_contains(EXPERT_B));
    assert!(calls[0].system_contains(&build_marker(1)));
    assert!(
        calls[1].system_contains(EXPERT_B),
        "extras 在本轮工具执行期间换了，第二次推理的 system prompt 仍是旧的"
    );
    assert!(!calls[1].system_contains(EXPERT_A));
    assert!(
        calls[1].system_contains(&build_marker(2)),
        "第二次推理应当走了一次重建"
    );
    Ok(())
}

/// ⭐ 经典循环：工具执行期间同 key 同正文重写 ⇒ extras 内容没变 ⇒ **不重建**。
#[tokio::test]
async fn classic_loop_keeps_system_prompt_when_extras_are_rewritten_unchanged() -> Result<()> {
    let calls = run_turn(EXPERT_A, false).await?;
    assert!(calls[1].system_contains(EXPERT_A));
    assert!(
        calls[1].system_contains(&build_marker(1)),
        "第二次推理的 system prompt 应当就是第一次建的那份"
    );
    assert!(
        !calls[1].system_contains(&build_marker(2)),
        "extras 内容没变，却重建了 system prompt"
    );
    Ok(())
}

/// 对照：状态机循环本来就每次推理前重建，两条循环在「换专家同轮生效」上行为一致。
#[tokio::test]
async fn state_machine_loop_also_applies_the_new_extras_within_the_turn() -> Result<()> {
    let calls = run_turn(EXPERT_B, true).await?;
    assert!(calls[0].system_contains(EXPERT_A));
    assert!(calls[1].system_contains(EXPERT_B));
    assert!(!calls[1].system_contains(EXPERT_A));
    Ok(())
}
