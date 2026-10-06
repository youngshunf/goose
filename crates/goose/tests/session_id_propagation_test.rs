use goose::conversation::message::Message;
use goose::providers::api_client::{ApiClient, AuthMethod};
use goose::providers::base::Provider;
use goose::providers::openai::OpenAiProvider;
use goose::session_context::{session_id_request_builder, SESSION_ID_HEADER};
use goose_providers::model::ModelConfig;
use serde_json::json;
use std::sync::Arc;
use std::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

#[derive(Clone, Default)]
struct HeaderCapture {
    captured_headers: Arc<Mutex<Vec<Option<String>>>>,
}

impl HeaderCapture {
    fn new() -> Self {
        Self {
            captured_headers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn capture_session_header(&self, req: &Request) {
        let session_id = req
            .headers
            .get(SESSION_ID_HEADER)
            .map(|v| v.to_str().unwrap().to_string());
        self.captured_headers.lock().unwrap().push(session_id);
    }

    fn get_captured(&self) -> Vec<Option<String>> {
        self.captured_headers.lock().unwrap().clone()
    }
}

fn create_test_provider(mock_server_url: &str) -> Box<dyn Provider> {
    let api_client = ApiClient::new_with_tls(
        mock_server_url.to_string(),
        AuthMethod::BearerToken("test-key".to_string()),
        None,
    )
    .unwrap()
    .with_request_builder(session_id_request_builder());
    Box::new(OpenAiProvider::new(api_client))
}

async fn setup_mock_server() -> (MockServer, HeaderCapture, Box<dyn Provider>) {
    let mock_server = MockServer::start().await;
    let capture = HeaderCapture::new();
    let chat_capture = capture.clone();
    let responses_capture = capture.clone();

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |req: &Request| {
            chat_capture.capture_session_header(req);
            // Return SSE streaming format
            let sse_response = format!(
                "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                json!({
                    "choices": [{
                        "delta": {
                            "content": "Hi there! How can I help you today?",
                            "role": "assistant"
                        },
                        "index": 0
                    }],
                    "created": 1755133833,
                    "id": "chatcmpl-test",
                    "model": "gpt-5-nano"
                }),
                json!({
                    "choices": [],
                    "usage": {
                        "completion_tokens": 10,
                        "prompt_tokens": 8,
                        "total_tokens": 18
                    }
                })
            );
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream")
        })
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |req: &Request| {
            responses_capture.capture_session_header(req);
            let sse_response = format!(
                "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                json!({
                    "type": "response.created",
                    "sequence_number": 1,
                    "response": {
                        "id": "resp_test",
                        "object": "response",
                        "created_at": 1755133833,
                        "status": "in_progress",
                        "model": "gpt-5-nano",
                        "output": []
                    }
                }),
                json!({
                    "type": "response.output_text.delta",
                    "sequence_number": 2,
                    "item_id": "msg_test",
                    "output_index": 0,
                    "content_index": 0,
                    "delta": "Hi there! How can I help you today?"
                }),
                json!({
                    "type": "response.completed",
                    "sequence_number": 3,
                    "response": {
                        "id": "resp_test",
                        "object": "response",
                        "created_at": 1755133833,
                        "status": "completed",
                        "model": "gpt-5-nano",
                        "output": [],
                        "usage": {
                            "input_tokens": 8,
                            "output_tokens": 10,
                            "total_tokens": 18
                        }
                    }
                })
            );
            ResponseTemplate::new(200)
                .set_body_string(sse_response)
                .insert_header("content-type", "text/event-stream")
        })
        .mount(&mock_server)
        .await;

    let provider = create_test_provider(&mock_server.uri());
    (mock_server, capture, provider)
}

async fn make_request(provider: &dyn Provider, session_id: &str) {
    let message = Message::user().with_text("test message");
    let model_config = ModelConfig::new("gpt-5-nano");
    let _ = goose::session_context::with_session_id(
        Some(session_id.to_string()),
        provider.complete(
            &model_config,
            "You are a helpful assistant.",
            &[message],
            &[],
        ),
    )
    .await
    .unwrap();
}

#[tokio::test]
#[cfg(feature = "otel")]
async fn test_session_id_propagates_to_log_records() {
    use opentelemetry::logs::AnyValue;
    use opentelemetry::Key;
    use opentelemetry_appender_tracing::layer::{
        OpenTelemetryTracingBridge, TracingSpanAttributes,
    };
    use opentelemetry_sdk::logs::{InMemoryLogExporterBuilder, SdkLoggerProvider};
    use tracing_subscriber::prelude::*;

    let exporter = InMemoryLogExporterBuilder::default().build();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();

    let layer = OpenTelemetryTracingBridge::builder(&provider)
        .with_tracing_span_attributes(TracingSpanAttributes::allowlist(["session.id"]))
        .build();
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("test", session.id = "test-session-42");
    let _enter = span.enter();
    tracing::info!("hello from test");
    drop(_enter);
    drop(_guard);

    provider.force_flush().unwrap();
    let logs = exporter.get_emitted_logs().unwrap();
    assert_eq!(logs.len(), 1);
    let log = &logs[0];

    let has_session_id = log.record.attributes_iter().any(|(k, v)| {
        k == &Key::new("session.id")
            && matches!(v, AnyValue::String(s) if s.as_str() == "test-session-42")
    });
    assert!(has_session_id);
}

#[tokio::test]
async fn test_session_id_propagation_to_llm() {
    let (_, capture, provider) = setup_mock_server().await;

    make_request(provider.as_ref(), "integration-test-session-123").await;

    assert_eq!(
        capture.get_captured(),
        vec![Some("integration-test-session-123".to_string())]
    );
}

#[tokio::test]
async fn test_session_id_always_present() {
    let (_, capture, provider) = setup_mock_server().await;

    make_request(provider.as_ref(), "test-session-id").await;

    assert_eq!(
        capture.get_captured(),
        vec![Some("test-session-id".to_string())]
    );
}

#[tokio::test]
async fn test_session_id_matches_across_calls() {
    let (_, capture, provider) = setup_mock_server().await;

    let session_id = "consistent-session-456";
    make_request(provider.as_ref(), session_id).await;
    make_request(provider.as_ref(), session_id).await;
    make_request(provider.as_ref(), session_id).await;

    assert_eq!(
        capture.get_captured(),
        vec![Some(session_id.to_string()); 3]
    );
}

// 只包装既有本地 HTTP 夹具的真实 provider；观察用途，不替换循环或网络解析。
struct MainRequestWitness {
    inner: Arc<dyn Provider>,
    main_requests: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl Provider for MainRequestWitness {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[rmcp::model::Tool],
    ) -> Result<goose::providers::base::MessageStream, goose_providers::errors::ProviderError> {
        self.inner
            .stream(model_config, system, messages, tools)
            .await
    }

    async fn stream_main(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[rmcp::model::Tool],
    ) -> Result<goose::providers::base::MessageStream, goose_providers::errors::ProviderError> {
        self.main_requests
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner
            .stream(model_config, system, messages, tools)
            .await
    }
}

#[tokio::test]
async fn main_request_witness_is_shared_by_both_loops_but_not_auxiliary_requests(
) -> anyhow::Result<()> {
    use futures::StreamExt;
    use goose::agents::{Agent, AgentConfig, AgentEvent, GoosePlatform, SessionConfig};
    use goose::config::permission::PermissionManager;
    use goose::config::GooseMode;
    use goose::providers::base::collect_stream;
    use goose::session::{SessionManager, SessionType};
    use std::sync::atomic::Ordering;
    use tokio_util::sync::CancellationToken;

    const RESPONSE: &str = "Hi there! How can I help you today?";
    for use_state_machine in [false, true] {
        let (_server, capture, inner) = setup_mock_server().await;
        let model_config = ModelConfig::new("gpt-5-nano");
        let messages = [Message::user().with_text("验证请求用途")];

        // 未覆写新方法的真实 provider 仍经原 stream 返回原消息与 usage。
        let default_stream = inner.stream_main(&model_config, "", &messages, &[]).await?;
        let (default_response, default_usage) = collect_stream(default_stream).await?;
        assert_eq!(default_response.as_concat_text(), RESPONSE);
        assert_eq!(default_usage.usage.total_tokens, Some(18));

        let provider = Arc::new(MainRequestWitness {
            inner: Arc::from(inner),
            main_requests: std::sync::atomic::AtomicUsize::new(0),
        });
        let temp_dir = tempfile::tempdir()?;
        let state_dir = temp_dir.path().join("state");
        let session_manager = Arc::new(SessionManager::new(state_dir.clone()));
        let session = session_manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "主辅助请求用途".to_string(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await?;

        let (auxiliary_response, _) = goose::model_config::complete_one_shot(
            provider.as_ref(),
            &model_config,
            &session.id,
            "辅助摘要",
            &messages,
            &[],
        )
        .await?;
        assert_eq!(auxiliary_response.as_concat_text(), RESPONSE);
        let auxiliary_stream = goose::session_context::with_session_id(
            Some(session.id.clone()),
            provider.stream(&model_config, "辅助直接流", &messages, &[]),
        )
        .await?;
        let (auxiliary_response, _) = collect_stream(auxiliary_stream).await?;
        assert_eq!(auxiliary_response.as_concat_text(), RESPONSE);
        assert_eq!(provider.main_requests.load(Ordering::SeqCst), 0);

        let agent = Agent::with_config(
            AgentConfig::new(
                session_manager,
                Arc::new(PermissionManager::new(state_dir.join("permissions"))),
                None,
                GooseMode::Auto,
                true,
                GoosePlatform::GooseCli,
            )
            .with_context_file_names(Vec::new()),
        );
        agent
            .update_provider(provider.clone(), model_config, &session.id)
            .await?;
        let mut stream = agent
            .reply(
                Message::user().with_text("验证请求用途"),
                SessionConfig {
                    id: session.id.clone(),
                    schedule_id: None,
                    max_turns: Some(2),
                    retry_config: None,
                },
                use_state_machine,
                Some(CancellationToken::new()),
            )
            .await?;
        let mut assistant_text = String::new();
        while let Some(event) = stream.next().await {
            if let AgentEvent::Message(message) = event? {
                if message.role == rmcp::model::Role::Assistant {
                    assistant_text.push_str(&message.as_concat_text());
                }
            }
        }
        assert!(
            assistant_text.starts_with(RESPONSE),
            "state_machine={use_state_machine}：主响应未保留默认 provider 的真实正文，实收：{assistant_text}"
        );
        assert_eq!(
            provider.main_requests.load(Ordering::SeqCst),
            1,
            "state_machine={use_state_machine}：主入口必须提供用途见证，辅助请求不能冒领"
        );
        assert_eq!(
            capture
                .get_captured()
                .into_iter()
                .skip(1)
                .collect::<Vec<_>>(),
            vec![Some(session.id); 3],
            "主请求与辅助请求确实使用同一个 session，不能按 session 猜用途"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_different_sessions_have_different_ids() {
    let (_, capture, provider) = setup_mock_server().await;

    let session_id_1 = "session-one";
    let session_id_2 = "session-two";
    make_request(provider.as_ref(), session_id_1).await;
    make_request(provider.as_ref(), session_id_2).await;

    assert_eq!(
        capture.get_captured(),
        vec![
            Some(session_id_1.to_string()),
            Some(session_id_2.to_string())
        ]
    );
}
