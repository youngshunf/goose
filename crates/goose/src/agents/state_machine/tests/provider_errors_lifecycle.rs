//! provider 终态错误一律以带 kind 的 `Error` 块产出（唤星 `PATCHES.md` `#3`），
//! 经 `Agent::reply` **两条循环**（经典 / 状态机）的端到端判据。
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

async fn reply_against(error: ProviderError, use_state_machine: bool) -> Result<Outcome> {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(FailingProvider {
        error,
        calls: calls.clone(),
    });
    let (emitted, persisted) = reply_with_provider(provider, use_state_machine).await?;
    Ok(Outcome {
        provider_calls: calls.load(Ordering::SeqCst),
        emitted,
        persisted,
    })
}

async fn reply_with_provider(
    provider: Arc<dyn Provider>,
    use_state_machine: bool,
) -> Result<(Vec<Message>, Vec<Message>)> {
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
        GooseMode::Auto,
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
        retry_config: None,
    };
    let mut stream = agent
        .reply(
            Message::user().with_text("hello"),
            session_config,
            use_state_machine,
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
    use_state_machine: bool,
) {
    let label = format!("state_machine={use_state_machine} {error:?}");
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

async fn reply_against_empty_stream(
    max_retries: usize,
    use_state_machine: bool,
) -> Result<Outcome> {
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(EmptyStreamingProvider {
        calls: Arc::clone(&calls),
        max_retries,
    });
    let (emitted, persisted) = reply_with_provider(provider, use_state_machine).await?;
    Ok(Outcome {
        provider_calls: calls.load(Ordering::SeqCst),
        emitted,
        persisted,
    })
}

/// 零重试实例的空轮必须在第一次成功的空 stream 后报告具名失败，不再请求 provider。
#[tokio::test]
async fn zero_retry_provider_ends_empty_successful_turn_with_error_without_replay() -> Result<()> {
    let outcome = reply_against_empty_stream(0, false).await?;
    assert_eq!(outcome.provider_calls, 1, "零重试仍重放了非幂等的空轮请求");
    let emitted_errors: Vec<_> = outcome
        .emitted
        .iter()
        .flat_map(|message| {
            message
                .content
                .iter()
                .filter_map(|content| content.as_error())
        })
        .collect();
    assert_eq!(
        emitted_errors.len(),
        1,
        "事件流应恰有一个 Error 块：{:?}",
        outcome.emitted
    );
    assert_eq!(emitted_errors[0].kind, MessageErrorKind::Other);
    assert_eq!(
        emitted_errors[0].message,
        "The model returned an empty response. Please resend your message to continue."
    );
    assert_eq!(
        outcome
            .emitted
            .iter()
            .filter(|message| message.error_kind().is_some())
            .count(),
        1,
        "事件流不能重复发出错误"
    );
    assert!(
        outcome
            .emitted
            .iter()
            .all(|message| message.role != Role::Assistant
                || message
                    .content
                    .iter()
                    .all(|content| content.as_text().is_none())),
        "空轮不能再以普通 assistant 文本伪装成功：{:?}",
        outcome.emitted
    );
    let error_message = outcome
        .persisted
        .iter()
        .find(|message| message.error_kind() == Some(MessageErrorKind::Other))
        .expect("会话必须持久化空轮 Error 块");
    assert_eq!(
        error_message.content.len(),
        1,
        "持久化消息应只包含 Error 块"
    );
    assert_eq!(error_message.content[0].as_error(), Some(emitted_errors[0]));
    assert!(error_message.is_user_visible(), "主人必须看得见空轮错误");
    assert!(
        !error_message.is_agent_visible(),
        "下一轮模型不应读到空轮错误"
    );
    Ok(())
}

/// 默认 provider 仍按既有固定 1+3 次空轮重试，不受零重试实例行为影响。
#[tokio::test]
async fn default_retry_provider_still_replays_empty_turn_four_times() -> Result<()> {
    let outcome = reply_against_empty_stream(3, false).await?;
    assert_eq!(outcome.provider_calls, 4);
    assert!(outcome.emitted.iter().any(|message| {
        message.role == Role::Assistant
            && message.as_concat_text()
                == "The model returned an empty response. Please resend your message to continue."
    }));
    Ok(())
}

/// 状态机空轮本就只调用一次 provider，经典循环的修补不得改变它。
#[tokio::test]
async fn state_machine_does_not_replay_zero_retry_empty_turn() -> Result<()> {
    let outcome = reply_against_empty_stream(0, true).await?;
    assert_eq!(outcome.provider_calls, 1);
    assert!(outcome.emitted.iter().any(|message| {
        message.role == Role::Assistant
            && message.as_concat_text()
                == "The model returned an empty response. Please resend your message to continue."
    }));
    Ok(())
}

/// ⭐ 网络错误（relay 断连、重试耗尽）⇒ 两条循环都产出 `Error { kind: Other }`。
#[tokio::test]
async fn network_error_ends_the_turn_with_an_error_block() -> Result<()> {
    let error = ProviderError::NetworkError("connection reset by relay".to_string());
    for use_state_machine in [false, true] {
        let outcome = reply_against(error.clone(), use_state_machine).await?;
        assert_terminal_error_block(
            error.clone(),
            MessageErrorKind::Other,
            &outcome,
            use_state_machine,
        );
    }
    Ok(())
}

/// ⭐ 通配那一支（`RequestFailed`：relay 404 之类）⇒ 两条循环都产出 `Error { kind: Other }`。
#[tokio::test]
async fn other_provider_error_ends_the_turn_with_an_error_block() -> Result<()> {
    let error = ProviderError::RequestFailed("relay route not found".to_string());
    for use_state_machine in [false, true] {
        let outcome = reply_against(error.clone(), use_state_machine).await?;
        assert_terminal_error_block(
            error.clone(),
            MessageErrorKind::Other,
            &outcome,
            use_state_machine,
        );
    }
    Ok(())
}

/// 对照：`Authentication` 那一支在 `#3` 之前就已经是这个形状（上游原样），
/// 上面两条就是把另外两支拉齐到它。它在改前也绿——它红了说明判据本身坏了，不是 patch 坏了。
#[tokio::test]
async fn authentication_error_ends_the_turn_with_an_error_block() -> Result<()> {
    let error = ProviderError::Authentication("invalid relay token".to_string());
    for use_state_machine in [false, true] {
        let outcome = reply_against(error.clone(), use_state_machine).await?;
        assert_terminal_error_block(
            error.clone(),
            MessageErrorKind::Authentication,
            &outcome,
            use_state_machine,
        );
    }
    Ok(())
}
