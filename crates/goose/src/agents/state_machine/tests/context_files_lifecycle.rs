//! 嵌入方显式指定上下文文件名（`AgentConfig::with_context_file_names`，唤星 `PATCHES.md` `#2`）
//! 经 `Agent::reply` **两条循环**（经典 / 状态机）的端到端判据。
//!
//! 判定面是 **provider 真收到的 system prompt**（`DummyApi` 记下的请求体），
//! ⛔ 不是 `PromptManager` 里的某个字段——字段对了而请求体里还有 hints，正是要抓的那种错。
//!
//! 布局（覆盖内核读 hints 的全部四种来源）：
//!
//! ```text
//! <tmp>/project/.git/                 ← git 根（`find_git_root` 向上走到这里）
//! <tmp>/project/AGENTS.md             ← 工作目录上一级、git 根里的 hints
//! <tmp>/project/work/AGENTS.md        ← 工作目录里的 hints，末行 `@referenced.md`
//! <tmp>/project/work/referenced.md    ← 经 `@引用` 展开进来的文件
//! <tmp>/project/work/.goosehints      ← 另一个缺省文件名
//! <tmp>/project/work/nested/AGENTS.md ← 工具调用之后才加载的子目录 hints
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::calculator_extension::{CalculatorExtension, ADD};
use super::dummy_api::{DummyApi, ProviderFeatures};
use crate::agents::extension::ExtensionConfig;
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::{Agent, AgentConfig, GoosePlatform, SessionConfig};
use crate::config::permission::PermissionManager;
use crate::config::GooseMode;
use crate::conversation::message::Message;
use crate::providers::base::Provider;
use crate::session::{SessionManager, SessionType};
use goose_providers::model::ModelConfig;

const PARENT_MARKER: &str = "PARENT_GIT_ROOT_AGENTS_MD_MARKER";
const ROOT_MARKER: &str = "WORKING_DIR_AGENTS_MD_MARKER";
const GOOSEHINTS_MARKER: &str = "WORKING_DIR_GOOSEHINTS_MARKER";
const REFERENCED_MARKER: &str = "AT_IMPORTED_FILE_MARKER";
const NESTED_MARKER: &str = "NESTED_SUBDIRECTORY_AGENTS_MD_MARKER";

/// 每轮一开始就该在 system prompt 里的四份（缺省行为下）。
const WORKING_DIR_MARKERS: [&str; 4] = [
    PARENT_MARKER,
    ROOT_MARKER,
    GOOSEHINTS_MARKER,
    REFERENCED_MARKER,
];
const ALL_MARKERS: [&str; 5] = [
    PARENT_MARKER,
    ROOT_MARKER,
    GOOSEHINTS_MARKER,
    REFERENCED_MARKER,
    NESTED_MARKER,
];

fn lay_out_hint_files(base: &Path) -> PathBuf {
    let project = base.join("project");
    let work = project.join("work");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::create_dir_all(work.join("nested")).unwrap();
    std::fs::write(project.join("AGENTS.md"), PARENT_MARKER).unwrap();
    std::fs::write(
        work.join("AGENTS.md"),
        format!("{ROOT_MARKER}\n@referenced.md"),
    )
    .unwrap();
    std::fs::write(work.join("referenced.md"), REFERENCED_MARKER).unwrap();
    std::fs::write(work.join(".goosehints"), GOOSEHINTS_MARKER).unwrap();
    std::fs::write(work.join("nested").join("AGENTS.md"), NESTED_MARKER).unwrap();
    work
}

/// 跑一轮「先调一次带 `path` 的工具、再收尾」，逐次交回 provider 收到的 system prompt 里
/// 出现了哪些标记。
async fn hint_markers_per_inference(
    context_file_names: Option<Vec<String>>,
    use_state_machine: bool,
) -> Result<Vec<Vec<&'static str>>> {
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
    let working_dir = lay_out_hint_files(temp_dir.path());
    let state_dir = temp_dir.path().join("state");
    let session_manager = Arc::new(SessionManager::new(state_dir.clone()));
    let session = session_manager
        .create_session(
            working_dir,
            "context-files".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await?;
    let mut config = AgentConfig::new(
        session_manager,
        Arc::new(PermissionManager::new(state_dir.join("permissions"))),
        None,
        GooseMode::Auto,
        true,
        GoosePlatform::GooseCli,
    );
    if let Some(names) = context_file_names {
        config = config.with_context_file_names(names);
    }
    let agent = Agent::with_config(config);
    agent
        .update_provider(
            provider,
            ModelConfig::new(goose_providers::openai::OPEN_AI_DEFAULT_MODEL)
                .with_canonical_limits("openai"),
            &session.id,
        )
        .await?;
    let calculator = Arc::new(CalculatorExtension::new(
        agent.config.session_manager.action_required(),
    ));
    agent
        .extension_manager
        .add_client(
            "calculator".to_string(),
            ExtensionConfig::Platform {
                name: "calculator".to_string(),
                description: "Stateful test calculator".to_string(),
                display_name: None,
                bundled: None,
                available_tools: vec![],
            },
            calculator.clone(),
            calculator.get_info().cloned(),
        )
        .await;

    // `path` 指向 `nested/` 下的文件 ⇒ 两条循环各自的子目录跟踪器都会把 `nested/` 记为待加载。
    api.on("work in nested")
        .call(ADD, json!({ "value": 1, "path": "nested/file.rs" }));
    api.on("result: 1").reply("nested work complete");

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: None,
        max_turns: Some(4),
        retry_config: None,
    };
    let mut stream = agent
        .reply(
            Message::user().with_text("work in nested"),
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
        calculator.total(),
        1,
        "工具调用没跑成——子目录 hints 那条路根本没被触发，下面的断言全是假绿"
    );
    let calls = api.calls();
    assert_eq!(calls.len(), 2, "应当恰好两次推理：一次出工具调用、一次收尾");
    Ok(calls
        .iter()
        .map(|call| {
            ALL_MARKERS
                .into_iter()
                .filter(|marker| call.system_contains(marker))
                .collect()
        })
        .collect())
}

/// ⭐ 显式空表 ⇒ 两条循环的每一次 system prompt 里**一个 hints 片段都没有**。
#[tokio::test]
async fn explicit_empty_context_file_names_keep_every_hint_out_of_the_system_prompt() -> Result<()>
{
    for use_state_machine in [false, true] {
        let seen = hint_markers_per_inference(Some(Vec::new()), use_state_machine).await?;
        for (index, found) in seen.iter().enumerate() {
            assert!(
                found.is_empty(),
                "state_machine={use_state_machine} 第 {index} 次推理：嵌入方给的是空表，\
                 hints 文件却进了 system prompt：{found:?}"
            );
        }
    }
    Ok(())
}

/// 非真空对照：不指定 ⇒ 缺省行为照旧——工作目录向上到 git 根的四份一开始就在，
/// 工具调用之后子目录那份也进来。少了这一条，上一条在「内核根本不读 hints」的实现上也会绿。
#[tokio::test]
async fn unset_context_file_names_keep_the_default_hint_loading() -> Result<()> {
    for use_state_machine in [false, true] {
        let seen = hint_markers_per_inference(None, use_state_machine).await?;
        let first = &seen[0];
        for marker in WORKING_DIR_MARKERS {
            assert!(
                first.contains(&marker),
                "state_machine={use_state_machine}：缺省行为丢了 {marker}：{first:?}"
            );
        }
        assert!(
            !first.contains(&NESTED_MARKER),
            "state_machine={use_state_machine}：子目录 hints 只该在工具调用之后加载"
        );
        assert!(
            seen[1].contains(&NESTED_MARKER),
            "state_machine={use_state_machine}：工具调用之后子目录 hints 没有加载：{:?}",
            seen[1]
        );
    }
    Ok(())
}
