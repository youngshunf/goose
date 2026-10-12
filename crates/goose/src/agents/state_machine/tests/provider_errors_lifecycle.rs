//! provider 终态错误一律以带 kind 的 `Error` 块产出（唤星 `PATCHES.md` `#3`），
//! 经 `Agent::reply` 的端到端判据（上游已删除经典循环，状态机是唯一路径）。
//!
//! 判定面是 `Agent::reply` 发出的 `AgentEvent` 流与落库的会话，⛔ 不是内部函数返回值。
//! 单元夹具向 `Provider::stream` 注入确定错误，不冒称真实 relay E2E；
//! 嵌入方只能从类型上区分「上游失败了」与「模型正常答完了一句话」，
//! 一条纯文本 assistant 消息在它那里就是正常终态。
//!
//! 期望正文按**改前经典循环的写法逐字复原**（`legacy_text`），⛔ 不调用
//! `Message::from_provider_error` 求期望值——否则「面向人类的文字不变」这一条就是自证。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use rmcp::model::{Role, Tool};
use tokio_util::sync::CancellationToken;

use crate::agents::{Agent, AgentConfig, AgentEvent, GoosePlatform, SessionConfig};
use crate::config::permission::PermissionManager;
use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageErrorKind};
use crate::providers::base::{MessageStream, Provider};
use crate::session::{SessionManager, SessionType};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

/// 每次推理都在建流时返回同一个错误。建流失败不进首包重试（`reply_parts.rs`），
/// 两条循环都在第一次推理就走到终态错误那一支，不依赖退避时长。
struct FailingProvider {
    error: ProviderError,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Provider for FailingProvider {
    fn get_name(&self) -> &str {
        "failing-test-provider"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> std::result::Result<MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(self.error.clone())
    }
}

/// 改前经典循环三支的原文（`agents/agent.rs`，`#3` 之前）。
fn legacy_text(error: &ProviderError) -> String {
    match error {
        ProviderError::NetworkError(_) => {
            format!("{error}\n\nPlease resend your message to try again.")
        }
        _ => format!(
            "Ran into this error: {error}.\n\nPlease retry if you think this is a transient or recoverable error."
        ),
    }
}

struct Outcome {
    provider_calls: usize,
    emitted: Vec<Message>,
    persisted: Vec<Message>,
}

async fn reply_against(error: ProviderError) -> Result<Outcome> {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(FailingProvider {
        error,
        calls: calls.clone(),
    });
    let (emitted, persisted) = reply_with_provider(provider).await?;
    Ok(Outcome {
        provider_calls: calls.load(Ordering::SeqCst),
        emitted,
        persisted,
    })
}

async fn reply_with_provider(provider: Arc<dyn Provider>) -> Result<(Vec<Message>, Vec<Message>)> {
    let temp_dir = tempfile::tempdir()?;
    let state_dir = temp_dir.path().join("state");
    let session_manager = Arc::new(SessionManager::new(state_dir.clone()));
    let session = session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "provider-errors".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await?;
    let agent = Agent::with_config(AgentConfig::new(
        session_manager.clone(),
        Arc::new(PermissionManager::new(state_dir.join("permissions"))),
        None,
        true,
        GoosePlatform::GooseCli,
    ));
    agent
        .update_provider(provider, ModelConfig::new("test-model"), &session.id)
        .await?;

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: None,
        max_turns: Some(4),
    };
    let mut stream = agent
        .reply(
            Message::user().with_text("hello"),
            session_config,
            Some(CancellationToken::new()),
        )
        .await?;
    let mut emitted = Vec::new();
    while let Some(event) = stream.next().await {
        if let AgentEvent::Message(message) = event? {
            emitted.push(message);
        }
    }
    drop(stream);

    let persisted = session_manager
        .get_session(&session.id, true)
        .await?
        .conversation
        .map(|conversation| conversation.messages().clone())
        .unwrap_or_default();
    Ok((emitted, persisted))
}

fn assert_terminal_error_block(
    error: ProviderError,
    expected_kind: MessageErrorKind,
    outcome: &Outcome,
) {
    let label = format!("{error:?}");
    let expected_text = legacy_text(&error);
    assert!(
        outcome.provider_calls >= 1,
        "{label}：provider 一次都没被调用——错误那一支根本没走到，下面的断言全是假绿"
    );

    let emitted_errors: Vec<_> = outcome
        .emitted
        .iter()
        .flat_map(|message| message.content.iter().filter_map(|c| c.as_error()))
        .collect();
    assert_eq!(
        emitted_errors.len(),
        1,
        "{label}：嵌入方收到的事件流里应当恰好一个 Error 块（终态错误在类型上可判别），\
         实收消息：{:?}",
        outcome.emitted
    );
    assert_eq!(emitted_errors[0].kind, expected_kind, "{label}：kind 不对");
    assert_eq!(
        emitted_errors[0].message, expected_text,
        "{label}：面向人类的正文变了"
    );
    assert!(
        outcome
            .emitted
            .iter()
            .all(|message| message.as_concat_text() != expected_text),
        "{label}：同一句错误还以纯文本 assistant 消息发了一遍——嵌入方会把它当正常终态"
    );

    let persisted = outcome
        .persisted
        .iter()
        .find(|message| message.error_kind() == Some(expected_kind))
        .unwrap_or_else(|| {
            panic!(
                "{label}：会话里没有落下这条 Error 消息，实存：{:?}",
                outcome.persisted
            )
        });
    assert!(persisted.is_user_visible(), "{label}：落库的错误主人看不见");
    assert!(
        !persisted.is_agent_visible(),
        "{label}：落库的错误会进下一轮模型上下文"
    );
}

struct EmptyStreamingProvider {
    calls: Arc<AtomicUsize>,
    max_retries: usize,
}

#[async_trait]
impl Provider for EmptyStreamingProvider {
    fn get_name(&self) -> &str {
        "empty-stream-provider"
    }

    fn retry_config(&self) -> goose_providers::retry::RetryConfig {
        goose_providers::retry::RetryConfig {
            max_retries: self.max_retries,
            ..Default::default()
        }
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> std::result::Result<MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(futures::stream::empty()))
    }
}

async fn reply_against_empty_stream(max_retries: usize) -> Result<Outcome> {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(EmptyStreamingProvider {
        calls: Arc::clone(&calls),
        max_retries,
    });
    let (emitted, persisted) = reply_with_provider(provider).await?;
    Ok(Outcome {
        provider_calls: calls.load(Ordering::SeqCst),
        emitted,
        persisted,
    })
}

/// 状态机空轮只调用一次 provider（`#5` 的经典循环修补已随经典循环一起删除）。
#[tokio::test]
async fn state_machine_does_not_replay_zero_retry_empty_turn() -> Result<()> {
    let outcome = reply_against_empty_stream(0).await?;
    assert_eq!(outcome.provider_calls, 1);
    assert!(outcome.emitted.iter().any(|message| {
        message.role == Role::Assistant
            && message.as_concat_text()
                == "The model returned an empty response. Please resend your message to continue."
    }));
    Ok(())
}

/// ⭐ 网络错误（relay 断连、重试耗尽）⇒ 产出 `Error { kind: Other }`。
#[tokio::test]
async fn network_error_ends_the_turn_with_an_error_block() -> Result<()> {
    let error = ProviderError::NetworkError("connection reset by relay".to_string());
    let outcome = reply_against(error.clone()).await?;
    assert_terminal_error_block(error.clone(), MessageErrorKind::Other, &outcome);
    Ok(())
}

/// ⭐ 通配那一支（`RequestFailed`：relay 404 之类）⇒ 产出 `Error { kind: Other }`。
#[tokio::test]
async fn other_provider_error_ends_the_turn_with_an_error_block() -> Result<()> {
    let error = ProviderError::RequestFailed("relay route not found".to_string());
    let outcome = reply_against(error.clone()).await?;
    assert_terminal_error_block(error.clone(), MessageErrorKind::Other, &outcome);
    Ok(())
}

/// 对照：`Authentication` 那一支在 `#3` 之前就已经是这个形状（上游原样），
/// 上面两条就是把另外两支拉齐到它。它在改前也绿——它红了说明判据本身坏了，不是 patch 坏了。
#[tokio::test]
async fn authentication_error_ends_the_turn_with_an_error_block() -> Result<()> {
    let error = ProviderError::Authentication("invalid relay token".to_string());
    let outcome = reply_against(error.clone()).await?;
    assert_terminal_error_block(error.clone(), MessageErrorKind::Authentication, &outcome);
    Ok(())
}
