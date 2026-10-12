//! Makes filesystem skills available to inference and slash commands.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_sdk_types::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{CallToolResult, ContentBlock, ErrorData, JsonObject, Tool};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::Value;
use tracing_futures::Instrument;

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::state_machine::ops_toolcalling::{
    emit_post_tool_use, pending_advertised_tool_requests, run_pre_tool_hooks, tool_span,
    ToolDisposition, EXPIRED_APPROVAL_RESPONSE,
};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::agents::tool_execution::{CHAT_MODE_TOOL_SKIPPED_RESPONSE, DECLINED_RESPONSE};
use crate::agents::FilesystemSkills;
use crate::config::GooseMode;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::session::Session;

const LOAD_SKILL_TOOL_NAME: &str = "load_skill";

pub struct SkillOperation {
    hook_manager: HookManager,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    /// 文件系统技能发现档位（唤星 PATCHES.md #9）。
    filesystem_skills: FilesystemSkills,
}

#[derive(Deserialize, JsonSchema)]
struct LoadSkillParams {
    /// Name of the skill to load. Use "skill-name/path" to load a supporting file.
    name: String,
    /// Optional arguments to provide when loading the skill.
    #[serde(default)]
    args: Option<String>,
}

fn skill_tool() -> Result<Tool> {
    let schema = serde_json::to_value(schema_for!(LoadSkillParams))?;
    let schema = schema
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("load_skill schema is not an object"))?;
    Ok(Tool::new(
        LOAD_SKILL_TOOL_NAME,
        "Load a skill's full content into your context so you can follow its instructions.\n\n\
         Skills are listed in your system instructions. When you need to use one, \
         load it first to get the detailed instructions.",
        schema,
    ))
}

/// 按档位发现技能（唤星 PATCHES.md #9）：`Disabled` 一个都不扫；`WorkingDirectory` 只保留
/// `SourceEntry.global == false` 的来源（工作目录与项目级插件技能目录）——home、全局配置
/// 目录与内置技能（`global == true`）被同一过滤排除。
fn discovered_skills(working_dir: &Path, filesystem_skills: FilesystemSkills) -> Vec<SourceEntry> {
    match filesystem_skills {
        FilesystemSkills::Disabled => Vec::new(),
        FilesystemSkills::WorkingDirectory => crate::skills::discover_skills(Some(working_dir))
            .into_iter()
            .filter(|source| !source.global)
            .collect(),
    }
}

fn skill_instructions(working_dir: &Path, filesystem_skills: FilesystemSkills) -> Option<String> {
    let sources = discovered_skills(working_dir, filesystem_skills);
    let mut skills: Vec<&SourceEntry> = sources
        .iter()
        .filter(|source| {
            matches!(
                source.source_type,
                SourceType::Skill | SourceType::BuiltinSkill
            )
        })
        .collect();
    skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
    if skills.is_empty() {
        return None;
    }

    let mut instructions = String::from(
        "# Skills\n\nYou have these skills at your disposal. Load one when it can help with the task or when the user asks for it:",
    );
    for skill in skills {
        instructions.push_str(&format!("\n- {}: {}", skill.name, skill.description));
    }
    Some(instructions)
}

fn execute_skill(
    working_dir: &Path,
    filesystem_skills: FilesystemSkills,
    arguments: Option<JsonObject>,
) -> CallToolResult {
    // 档位把守（唤星 PATCHES.md #9）：工具没被广告不代表调用到不了这里——无 inference
    // 元数据的请求会绕过广告面核对（`request_was_advertised` 视其为已广告），必须具名
    // 失败而不是照常读文件。
    if filesystem_skills == FilesystemSkills::Disabled {
        return CallToolResult::error(vec![ContentBlock::text(format!(
            "Tool '{LOAD_SKILL_TOOL_NAME}' is disabled: filesystem skill discovery is off for this agent."
        ))]);
    }
    let params = arguments
        .map(Value::Object)
        .ok_or_else(|| "Missing arguments".to_string())
        .and_then(|arguments| {
            serde_json::from_value::<LoadSkillParams>(arguments)
                .map_err(|error| format!("Invalid arguments: {error}"))
        });
    let params = match params {
        Ok(params) => params,
        Err(error) => return CallToolResult::error(vec![ContentBlock::text(error)]),
    };
    let skill_name = params.name.as_str();
    let args = params.args.as_deref();
    let skills = discovered_skills(working_dir, filesystem_skills);

    if let Some(skill) = skills.iter().find(|skill| skill.name == skill_name) {
        return match crate::skills::loaded_skill_context_with_args(skill, args) {
            Ok(rendered) => CallToolResult::success(vec![ContentBlock::text(rendered)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to parse skill arguments: {error}"
            ))]),
        };
    }

    if let Some((parent_skill_name, raw_relative_path)) = skill_name.split_once('/') {
        let relative_path = raw_relative_path.replace('\\', "/");
        if let Some(skill) = skills.iter().find(|skill| {
            skill.name == parent_skill_name
                && matches!(
                    skill.source_type,
                    SourceType::Skill | SourceType::BuiltinSkill
                )
        }) {
            return load_supporting_file(skill, skill_name, &relative_path);
        }
    }

    let suggestions: Vec<&str> = skills
        .iter()
        .filter(|skill| {
            skill
                .name
                .to_lowercase()
                .contains(&skill_name.to_lowercase())
                || skill_name
                    .to_lowercase()
                    .contains(&skill.name.to_lowercase())
        })
        .take(3)
        .map(|skill| skill.name.as_str())
        .collect();
    if suggestions.is_empty() {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Skill '{skill_name}' not found."
        ))])
    } else {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Skill '{skill_name}' not found. Did you mean: {}?",
            suggestions.join(", ")
        ))])
    }
}

fn load_supporting_file(
    skill: &SourceEntry,
    skill_name: &str,
    relative_path: &str,
) -> CallToolResult {
    let skill_dir = PathBuf::from(&skill.path);
    for file_path in &skill.supporting_files {
        let file_path = Path::new(file_path);
        let Ok(relative) = file_path.strip_prefix(&skill_dir) else {
            continue;
        };
        if relative.to_string_lossy().replace('\\', "/") != relative_path {
            continue;
        }
        return match crate::skills::load_supporting_file(&skill_dir, relative, skill_name) {
            Ok(content) => CallToolResult::success(vec![ContentBlock::text(content)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to read '{skill_name}': {error}"
            ))]),
        };
    }

    let available: Vec<String> = skill
        .supporting_files
        .iter()
        .filter_map(|file| {
            Path::new(file)
                .strip_prefix(&skill_dir)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .take(10)
        .collect();
    if available.is_empty() {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Skill '{}' has no supporting files.",
            skill.name
        ))])
    } else {
        CallToolResult::error(vec![ContentBlock::text(format!(
            "File '{skill_name}' not found. Available: {}",
            available.join(", ")
        ))])
    }
}

impl SkillOperation {
    pub fn new(
        hook_manager: HookManager,
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
            filesystem_skills: FilesystemSkills::default(),
        }
    }

    /// 设定文件系统技能发现档位（唤星 PATCHES.md #9），语义见
    /// [`crate::agents::FilesystemSkills`]。`new` 的缺省是 `Disabled`。
    pub fn with_filesystem_skills(mut self, filesystem_skills: FilesystemSkills) -> Self {
        self.filesystem_skills = filesystem_skills;
        self
    }

    fn leased_session(&self, session: &Session) -> Option<Session> {
        let lease = self
            .extension_lease
            .lock()
            .expect("extension lease unavailable")
            .clone()?;
        if lease.scope_id() != session.id.as_str() {
            return None;
        }
        let mut session = session.clone();
        session.working_dir = lease.working_dir()?.to_path_buf();
        Some(session)
    }

    fn command_response(
        conversation: &Conversation,
        message: String,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("skill command conversation has no kickoff message"))?;
        let message_id = command
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let command = command.with_visibility(true, false);
        let response = Message::assistant()
            .with_text(message)
            .with_visibility(true, false);
        emit.message(command);
        let response = emit.message(response);
        yielded_with([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ])
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for SkillOperation {
    fn name(&self) -> &'static str {
        "skills"
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if command.command == "skills" {
            return Self::command_response(
                conversation,
                crate::slash_commands::skill_slash_command::format_installed_skills(Some(
                    &session.working_dir,
                )),
                emit,
            );
        }

        let prompt = match crate::slash_commands::skill_slash_command::resolve_command(
            command.command,
            command.params_str,
            Some(&session.working_dir),
        ) {
            Ok(Some(prompt)) => prompt,
            Ok(None) => return not_applicable(),
            Err(error) => return Self::command_response(conversation, error, emit),
        };
        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("skill command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        applied([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            Message::user()
                .with_text(prompt)
                .with_visibility(false, true)
                .into(),
        ])
    }

    async fn inference_tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        if self.filesystem_skills == FilesystemSkills::Disabled {
            return Ok(Vec::new());
        }
        Ok(vec![skill_tool()?])
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(
            skill_instructions(&session.working_dir, self.filesystem_skills)
                .map(|instructions| ("skills".to_string(), instructions))
                .into_iter()
                .collect(),
        )
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending: Vec<_> = pending_advertised_tool_requests(messages)
            .into_iter()
            .filter(|(request, _)| {
                request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|tool_call| tool_call.name.as_ref() == LOAD_SKILL_TOOL_NAME)
            })
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }

        let leased_session = self.leased_session(session);
        let mut response = Message::user();
        for (request, disposition) in pending {
            let result: std::result::Result<CallToolResult, ErrorData> = match disposition {
                ToolDisposition::Execute if session.goose_mode == GooseMode::Chat => {
                    // Nothing executes in chat mode, so no tool lifecycle runs.
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )]))
                }
                ToolDisposition::Execute => {
                    let Some(session) = leased_session.as_ref() else {
                        response.add_tool_response_with_metadata(
                            request.id,
                            Ok(CallToolResult::error(vec![ContentBlock::text(
                                EXPIRED_APPROVAL_RESPONSE,
                            )])),
                            request.metadata.as_ref(),
                        );
                        continue;
                    };
                    let tool_call = request.tool_call.as_ref().map_err(|error| {
                        anyhow!("load_skill tool call could not be parsed: {error}")
                    })?;
                    let span = tool_span(&tool_call.name, &request.id, &session.id);
                    // `load_skill` is executed here rather than by
                    // ToolExecutionOperation, which is registered after this one.
                    // Run the same hook lifecycle it would have run there.
                    let tool_input = tool_call
                        .arguments
                        .as_ref()
                        .map(|arguments| Value::Object(arguments.clone()));
                    match run_pre_tool_hooks(
                        &self.hook_manager,
                        session,
                        &request.id,
                        &tool_call.name,
                        tool_input.as_ref(),
                    )
                    .instrument(span.clone())
                    .await
                    {
                        // A denial returns before execution and emits no post
                        // event, the same shape ToolExecutionOperation has: its
                        // dispatch returns the denial before the post-hook wrapper
                        // is ever applied.
                        Err(denial) => Err(denial),
                        Ok(()) => {
                            let result = {
                                let _entered = span.enter();
                                execute_skill(
                                    &session.working_dir,
                                    self.filesystem_skills,
                                    tool_call.arguments.clone(),
                                )
                            };
                            if result.is_error == Some(true) {
                                span.record("error.type", "tool_error");
                            }
                            let output = Ok(result);
                            // Post event carries the same tool_call_id as the pre
                            // events. The large-response rewrite
                            // ToolExecutionOperation applies is deliberately not
                            // reused: a skill body is content the model is meant to
                            // read, not a payload to offload to a temp file.
                            emit_post_tool_use(
                                &self.hook_manager,
                                &session.id,
                                &session.working_dir.to_string_lossy(),
                                &tool_call.name,
                                &request.id,
                                tool_input.as_ref(),
                                &output,
                            )
                            .instrument(span.clone())
                            .await;
                            output
                        }
                    }
                }
                ToolDisposition::Decline => Ok(CallToolResult::error(vec![ContentBlock::text(
                    DECLINED_RESPONSE,
                )])),
                ToolDisposition::ParseError(error) => {
                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))]))
                }
            };
            response.add_tool_response_with_metadata(request.id, result, request.metadata.as_ref());
        }
        let response = emit.message(response);
        applied([response.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// 唤星 PATCHES.md #9：`Disabled` 档位下 `load_skill` 落到执行面也具名失败，不读任何
    /// 技能文件。工作目录里故意放一个同名技能——若把守失效，结果会是成功而非「没找到」。
    #[test]
    fn disabled_execute_fails_by_name_without_reading_files() {
        let working_dir = tempfile::tempdir().unwrap();
        let skill_dir = working_dir.path().join(".agents/skills/demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: demo\ndescription: DEMO_SKILL_DESC\n---\nDEMO_SKILL_BODY",
        )
        .unwrap();
        let arguments = serde_json::json!({ "name": "demo" }).as_object().cloned();

        let result = execute_skill(working_dir.path(), FilesystemSkills::Disabled, arguments);

        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().expect("expected text");
        assert!(
            text.text.contains("load_skill") && text.text.contains("disabled"),
            "Disabled 档位下执行面必须具名失败：{}",
            text.text
        );
        assert!(!text.text.contains("DEMO_SKILL_BODY"));
    }

    /// 唤星 PATCHES.md #9：`WorkingDirectory` 档位下 `load_skill` 只认非全局来源——全局配置
    /// 目录（`$GOOSE_PATH_ROOT/config/skills`）里的技能按名查找也落空（具名 not found）。
    /// 用 `GOOSE_PATH_ROOT` 充当局全局探针，不碰真实 home。
    #[test]
    fn working_directory_execute_does_not_load_global_skills() {
        let path_root = tempfile::tempdir().unwrap();
        let global_skill_dir = path_root.path().join("config/skills/global-only");
        std::fs::create_dir_all(&global_skill_dir).unwrap();
        std::fs::write(
            global_skill_dir.join("SKILL.md"),
            "---\nname: global-only\ndescription: GLOBAL_ONLY_DESC\n---\nGLOBAL_ONLY_BODY",
        )
        .unwrap();
        let working_dir = tempfile::tempdir().unwrap();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", path_root.path().to_str()),
            ("PLUGINS", None),
        ]);
        let arguments = serde_json::json!({ "name": "global-only" })
            .as_object()
            .cloned();

        let result = execute_skill(
            working_dir.path(),
            FilesystemSkills::WorkingDirectory,
            arguments,
        );

        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().expect("expected text");
        assert!(
            text.text.contains("Skill 'global-only' not found"),
            "WorkingDirectory 档位下全局技能必须按名找不到：{}",
            text.text
        );
        assert!(!text.text.contains("GLOBAL_ONLY_BODY"));
    }

    /// 唤星 PATCHES.md #9：`WorkingDirectory` 档位下工作目录技能正文确实能读出来
    /// （非真空对照：上面的把守测试若恒真，这条会抓住「全部判 not found」的过度过滤）。
    #[test]
    fn working_directory_execute_loads_workdir_skill() {
        let working_dir = tempfile::tempdir().unwrap();
        let skill_dir = working_dir.path().join(".agents/skills/demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: demo\ndescription: DEMO_SKILL_DESC\n---\nDEMO_SKILL_BODY",
        )
        .unwrap();
        let arguments = serde_json::json!({ "name": "demo" }).as_object().cloned();

        let result = execute_skill(
            working_dir.path(),
            FilesystemSkills::WorkingDirectory,
            arguments,
        );

        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().expect("expected text");
        assert!(text.text.contains("DEMO_SKILL_BODY"));
    }

    #[test]
    fn supporting_file_loader_reads_nested_regular_file() {
        let root = tempfile::tempdir().unwrap();
        let skill_dir = std::fs::canonicalize(root.path()).unwrap();
        let nested = skill_dir.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("guide.md");
        std::fs::write(&file, "Nested guidance.").unwrap();
        let skill = SourceEntry {
            source_type: SourceType::Skill,
            name: "test-skill".to_string(),
            description: String::new(),
            content: String::new(),
            path: skill_dir.to_string_lossy().into_owned(),
            global: false,
            writable: true,
            supporting_files: vec![file.to_string_lossy().into_owned()],
            properties: HashMap::new(),
        };

        let result = load_supporting_file(&skill, "test-skill/nested/guide.md", "nested/guide.md");

        assert_eq!(result.is_error, Some(false));
        let text = result.content[0].as_text().expect("expected text");
        assert!(text.text.contains("Nested guidance."));
    }
}
