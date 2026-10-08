#[cfg(test)]
use chrono::DateTime;
use chrono::Utc;
use indexmap::IndexMap;
use serde::Serialize;

use crate::agents::{extension::ExtensionInfo, moim};
use crate::hints::load_hints::build_gitignore;
use crate::hints::{get_context_filenames, load_hint_files, SubdirectoryHintTracker};
use crate::session::Session;
use crate::{
    config::{Config, GooseMode},
    prompt_template,
    utils::sanitize_unicode_tags,
};
use std::path::Path;

pub struct PromptManager {
    subdirectory_hints: IndexMap<String, String>,
    /// 嵌入宿主按 key 注入的提示词，与会话持久化 extras 和子目录 hints 分开保存。
    system_prompt_extras: IndexMap<String, String>,
    current_date_timestamp: String,
    subdirectory_hint_tracker: SubdirectoryHintTracker,
    /// 嵌入方显式指定的上下文文件名。`None` ⇒ 读全局配置的 `CONTEXT_FILE_NAMES`（原行为）。
    context_file_names: Option<Vec<String>>,
    /// 经 `add_system_prompt_extra` / `remove_system_prompt_extra` 写入的 extras **内容版本**
    /// （唤星 `PATCHES.md` `#7`）：内容真的变了才 +1，同 key 同正文重写不动它。
    /// 两条循环现在每次推理前都重建提示词，同轮生效不再依赖版本比对。
    system_prompt_extras_revision: u64,
}

impl Default for PromptManager {
    fn default() -> Self {
        PromptManager::new()
    }
}

#[derive(Serialize)]
struct SystemPromptContext {
    extensions: Vec<ExtensionInfo>,
    current_date_time: String,
    goose_mode: GooseMode,
    is_autonomous: bool,
    enable_subagents: bool,
    code_execution_mode: bool,
    include_extensions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    moim_system_prompt_block: Option<String>,
}

pub struct SystemPromptBuilder<'a, M> {
    manager: &'a M,

    system_prompt_override: Option<String>,
    extensions_info: Vec<ExtensionInfo>,
    prompt_extras: IndexMap<String, String>,
    subagents_enabled: bool,
    hints: Option<String>,
    code_execution_mode: bool,
    include_extensions: bool,
    goose_mode: Option<GooseMode>,
}

impl<'a> SystemPromptBuilder<'a, PromptManager> {
    pub fn with_session(mut self, session: &Session) -> Self {
        self.system_prompt_override = session.system_prompt_override.clone();
        self.prompt_extras
            .extend(session.system_prompt_extras.clone());
        self
    }

    pub fn with_extension(mut self, extension: ExtensionInfo) -> Self {
        self.extensions_info.push(extension);
        self
    }

    pub fn with_extensions(mut self, extensions: impl Iterator<Item = ExtensionInfo>) -> Self {
        for extension in extensions {
            self.extensions_info.push(extension);
        }
        self
    }

    pub fn with_prompt_extras(
        mut self,
        extras: impl IntoIterator<Item = (String, String)>,
    ) -> Self {
        self.prompt_extras.extend(extras);
        self
    }

    pub fn with_code_execution_mode(mut self, enabled: bool) -> Self {
        self.code_execution_mode = enabled;
        self
    }

    pub fn without_extensions(mut self) -> Self {
        self.include_extensions = false;
        self
    }

    pub fn with_hints(mut self, working_dir: &Path) -> Self {
        let hints_filenames = self
            .manager
            .context_file_names
            .clone()
            .unwrap_or_else(get_context_filenames);
        let ignore_patterns = build_gitignore(working_dir);

        let hints = load_hint_files(working_dir, &hints_filenames, &ignore_patterns);

        if !hints.is_empty() {
            self.hints = Some(hints);
        }
        self
    }

    pub fn with_enable_subagents(mut self, subagents_enabled: bool) -> Self {
        self.subagents_enabled = subagents_enabled;
        self
    }

    pub fn with_goose_mode(mut self, mode: GooseMode) -> Self {
        self.goose_mode = Some(mode);
        self
    }

    pub fn build(self) -> String {
        let mut extensions_info = self.extensions_info;

        // Stable tool ordering is important for multi session prompt caching.
        extensions_info.sort_by(|a, b| a.name.cmp(&b.name));

        let sanitized_extensions_info: Vec<ExtensionInfo> = extensions_info
            .into_iter()
            .map(|mut ext_info| {
                ext_info.instructions = sanitize_unicode_tags(&ext_info.instructions);
                ext_info
            })
            .collect();

        let goose_mode = self
            .goose_mode
            .unwrap_or_else(|| Config::global().get_goose_mode().unwrap_or_default());

        let context = SystemPromptContext {
            extensions: sanitized_extensions_info,
            current_date_time: self.manager.current_date_timestamp.clone(),
            goose_mode,
            is_autonomous: goose_mode == GooseMode::Auto,
            enable_subagents: self.subagents_enabled,
            code_execution_mode: self.code_execution_mode,
            include_extensions: self.include_extensions,
            moim_system_prompt_block: moim::system_prompt_block(),
        };

        let base_prompt = if let Some(override_prompt) = &self.system_prompt_override {
            let sanitized_override_prompt = sanitize_unicode_tags(override_prompt);
            prompt_template::render_string(&sanitized_override_prompt, &context)
        } else {
            prompt_template::render_template("system.md", &context)
        }
        .unwrap_or_else(|_| {
            "You are a general-purpose AI agent called goose, created by Block".to_string()
        });

        let mut system_prompt_extras = self.manager.subdirectory_hints.clone();
        system_prompt_extras.extend(self.prompt_extras);
        // 宿主当前注入是权威，不能被会话持久化的旧正文遮盖；同 key 保留原位置。
        system_prompt_extras.extend(self.manager.system_prompt_extras.clone());

        // Add hints if provided
        if let Some(hints) = self.hints {
            system_prompt_extras.insert("hints".to_string(), hints);
        }

        if goose_mode == GooseMode::Chat {
            system_prompt_extras.insert(
                "chat_mode".to_string(),
                "Right now you are in the chat only mode, no access to any tool use and system."
                    .to_string(),
            );
        }

        if system_prompt_extras.is_empty() {
            base_prompt
        } else {
            let sanitized_system_prompt_extras: Vec<String> = system_prompt_extras
                .into_values()
                .map(|extra| sanitize_unicode_tags(&extra))
                .collect();

            format!(
                "{}\n\n# Additional Instructions:\n\n{}",
                base_prompt,
                sanitized_system_prompt_extras.join("\n\n")
            )
        }
    }
}

impl PromptManager {
    pub fn new() -> Self {
        PromptManager {
            subdirectory_hints: IndexMap::new(),
            system_prompt_extras: IndexMap::new(),
            // Use the fixed current date time so that prompt cache can be used.
            // Filtering to an hour to balance user time accuracy and multi session prompt cache hits.
            current_date_timestamp: Utc::now().format("%Y-%m-%d %H:00 %:z").to_string(),
            subdirectory_hint_tracker: SubdirectoryHintTracker::new(),
            context_file_names: None,
            system_prompt_extras_revision: 0,
        }
    }

    /// 让**嵌入方显式指定**哪些文件名算「上下文文件」（hints）。
    ///
    /// - `None` ⇒ 与 [`PromptManager::new`] 完全相同：文件名读全局配置的
    ///   `CONTEXT_FILE_NAMES`，缺省 `[.goosehints, AGENTS.md]`；
    /// - `Some(names)` ⇒ 文件名**只**取 `names`，全局配置的 `CONTEXT_FILE_NAMES` 不再生效；
    ///   工作目录向上到 git 根、全局配置目录、子目录三条读路
    ///   （[`SystemPromptBuilder::with_hints`] 与 [`PromptManager::load_subdirectory_hints`]）都用它；
    /// - `Some(vec![])` ⇒ **一个 hints 文件都不读**。
    ///
    /// 为什么需要它：`Config::global()` 只有配置文件与环境变量两层，**没有进程内覆盖层**。
    /// 把 goose 嵌进自己进程的宿主若自己持有系统提示词的权威，而 agent 又能写它的工作目录，
    /// 那 agent 写下的一个 `AGENTS.md` 就会成为它自己下一轮的系统提示词。
    pub fn with_context_file_names(context_file_names: Option<Vec<String>>) -> Self {
        let mut manager = Self::new();
        if let Some(names) = &context_file_names {
            manager.subdirectory_hint_tracker =
                SubdirectoryHintTracker::with_context_filenames(names.clone());
        }
        manager.context_file_names = context_file_names;
        manager
    }

    #[cfg(test)]
    pub fn with_timestamp(dt: DateTime<Utc>) -> Self {
        PromptManager {
            subdirectory_hints: IndexMap::new(),
            system_prompt_extras: IndexMap::new(),
            current_date_timestamp: dt.format("%Y-%m-%d %H:%M:%S %:z").to_string(),
            subdirectory_hint_tracker: SubdirectoryHintTracker::new(),
            context_file_names: None,
            system_prompt_extras_revision: 0,
        }
    }

    /// 按 key 注入宿主提示词，同 key 替换原正文。
    pub fn add_system_prompt_extra(&mut self, key: String, instruction: String) {
        // 同 key 同正文 ⇒ 什么都不变（`IndexMap::insert` 本就保位），不推进版本。
        if self.system_prompt_extras.get(&key) != Some(&instruction) {
            self.system_prompt_extras.insert(key, instruction);
            self.system_prompt_extras_revision += 1;
        }
    }

    pub fn remove_system_prompt_extra(&mut self, key: &str) {
        if self.system_prompt_extras.shift_remove(key).is_some() {
            self.system_prompt_extras_revision += 1;
        }
    }

    /// extras 内容版本（唤星 `PATCHES.md` `#7`）。只比较相等性，不承诺具体数值。
    pub fn system_prompt_extras_revision(&self) -> u64 {
        self.system_prompt_extras_revision
    }

    pub fn record_tool_arguments(
        &mut self,
        arguments: &Option<serde_json::Map<String, serde_json::Value>>,
        working_dir: &Path,
    ) {
        self.subdirectory_hint_tracker
            .record_tool_arguments(arguments, working_dir);
    }

    pub fn load_subdirectory_hints(&mut self, working_dir: &Path) -> bool {
        let new_hints = self.subdirectory_hint_tracker.load_new_hints(working_dir);
        let has_new = !new_hints.is_empty();
        for (key, content) in new_hints {
            self.subdirectory_hints.insert(key, content);
        }
        has_new
    }

    pub fn build_system_prompt(
        &mut self,
        session: &Session,
        prompt_parts: Vec<(String, String)>,
        goose_mode: GooseMode,
    ) -> String {
        self.load_subdirectory_hints(&session.working_dir);
        self.builder()
            .with_session(session)
            .with_prompt_extras(prompt_parts)
            .with_hints(&session.working_dir)
            .with_goose_mode(goose_mode)
            .without_extensions()
            .build()
    }

    pub fn builder<'a>(&'a self) -> SystemPromptBuilder<'a, Self> {
        SystemPromptBuilder {
            manager: self,

            system_prompt_override: None,
            extensions_info: vec![],
            prompt_extras: self.system_prompt_extras.clone(),
            subagents_enabled: false,
            hints: None,
            code_execution_mode: false,
            include_extensions: true,
            goose_mode: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;

    fn session_with_override(system_prompt_override: &str) -> Session {
        Session {
            system_prompt_override: Some(system_prompt_override.to_string()),
            ..Session::default()
        }
    }

    #[test]
    fn test_build_system_prompt_sanitizes_override() {
        let manager = PromptManager::new();
        let session =
            session_with_override("System prompt\u{E0041}\u{E0042}\u{E0043}with hidden text");

        let result = manager.builder().with_session(&session).build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("System prompt"));
        assert!(result.contains("with hidden text"));
    }

    #[test]
    fn test_current_date_time_includes_timezone() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let session = session_with_override("It is currently {{current_date_time}}");

        let result = manager.builder().with_session(&session).build();

        assert_eq!(result, "It is currently 1970-01-01 00:00:00 +00:00");
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extras() {
        let malicious_extra = "Extra instruction\u{E0041}\u{E0042}\u{E0043}hidden";

        let result = PromptManager::new()
            .builder()
            .with_prompt_extras([("test".to_string(), malicious_extra.to_string())])
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extra instruction"));
        assert!(result.contains("hidden"));
    }

    #[test]
    fn prompt_contributions_are_not_retained() {
        let manager = PromptManager::new();

        let with_contribution = manager
            .builder()
            .with_prompt_extras([("operation".to_string(), "temporary instruction".to_string())])
            .build();
        let without_contribution = manager.builder().build();

        assert!(with_contribution.contains("temporary instruction"));
        assert!(!without_contribution.contains("temporary instruction"));
    }

    #[test]
    fn composed_prompt_uses_contributions_instead_of_the_extension_catalog() {
        let mut manager = PromptManager::new();
        let working_dir = tempfile::tempdir().unwrap();

        let session = Session {
            working_dir: working_dir.path().to_path_buf(),
            ..Session::default()
        };

        let prompt = manager.build_system_prompt(
            &session,
            vec![(
                "extensions".to_string(),
                "# Extensions\n\n## developer".to_string(),
            )],
            GooseMode::Auto,
        );

        assert!(prompt.contains("## developer"));
        assert!(!prompt.contains("No extensions are defined"));
    }

    #[test]
    fn project_git_metadata_does_not_reach_system_prompt() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".git")).unwrap();
        std::fs::create_dir(project.path().join("docs")).unwrap();
        std::fs::write(
            project.path().join(".git/config"),
            "url = https://oauth2:PROMPT_SECRET@example.invalid/repo.git",
        )
        .unwrap();
        std::fs::write(
            project.path().join("docs/config.md"),
            "legitimate project configuration",
        )
        .unwrap();
        std::fs::write(
            project.path().join(crate::hints::AGENTS_MD_FILENAME),
            "project instructions\n@.git/config\n@docs/config.md",
        )
        .unwrap();
        let ignore_patterns = build_gitignore(project.path());
        let hints = load_hint_files(
            project.path(),
            &[crate::hints::AGENTS_MD_FILENAME.to_string()],
            &ignore_patterns,
        );

        let prompt = PromptManager::new()
            .builder()
            .with_prompt_extras([("hints".to_string(), hints)])
            .build();

        assert!(prompt.contains("project instructions"));
        assert!(prompt.contains("legitimate project configuration"));
        assert!(!prompt.contains("PROMPT_SECRET"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_multiple_extras() {
        let result = PromptManager::new()
            .builder()
            .with_prompt_extras([
                ("test1".to_string(), "First\u{E0041}instruction".to_string()),
                (
                    "test2".to_string(),
                    "Second\u{E0042}instruction".to_string(),
                ),
                ("test3".to_string(), "Third\u{E0043}instruction".to_string()),
            ])
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Firstinstruction"));
        assert!(result.contains("Secondinstruction"));
        assert!(result.contains("Thirdinstruction"));
    }

    #[test]
    fn test_remove_system_prompt_extra() {
        let mut manager = PromptManager::new();
        manager.add_system_prompt_extra("agent".to_string(), "Agent instruction".to_string());
        manager.add_system_prompt_extra("project".to_string(), "Project instruction".to_string());

        manager.remove_system_prompt_extra("agent");
        let result = manager.builder().build();

        assert!(!result.contains("Agent instruction"));
        assert!(result.contains("Project instruction"));
    }

    #[test]
    fn host_extras_and_session_extras_remain_independent() {
        let mut manager = PromptManager::new();
        manager.add_system_prompt_extra("host".to_string(), "Host instruction".to_string());
        let mut session = session_with_override("BASE");
        session
            .system_prompt_extras
            .insert("session".to_string(), "Session instruction".to_string());

        assert_eq!(
            manager.builder().with_session(&session).build(),
            "BASE\n\n# Additional Instructions:\n\nHost instruction\n\nSession instruction"
        );
        manager.remove_system_prompt_extra("host");
        assert_eq!(
            manager.builder().with_session(&session).build(),
            "BASE\n\n# Additional Instructions:\n\nSession instruction"
        );
        assert_eq!(
            session.system_prompt_extras["session"],
            "Session instruction"
        );
        assert_eq!(
            manager
                .builder()
                .with_session(&session_with_override("BASE"))
                .build(),
            "BASE"
        );
    }

    #[test]
    fn host_extras_override_stale_session_and_operation_values() {
        let mut manager = PromptManager::new();
        manager.add_system_prompt_extra("expert".to_string(), "HOST_CURRENT".to_string());
        let mut session = session_with_override("BASE");
        session
            .system_prompt_extras
            .insert("expert".to_string(), "SESSION_STALE".to_string());

        assert_eq!(
            manager
                .builder()
                .with_session(&session)
                .with_prompt_extras([("expert".to_string(), "OPERATION_STALE".to_string())])
                .build(),
            "BASE\n\n# Additional Instructions:\n\nHOST_CURRENT"
        );
    }

    /// 唤星 `PATCHES.md` `#7`：extras 内容版本只在内容真的变了时推进。
    #[test]
    fn extras_revision_moves_only_when_extras_content_changes() {
        let mut manager = PromptManager::new();
        let start = manager.system_prompt_extras_revision();

        manager.add_system_prompt_extra("expert".to_string(), "A".to_string());
        let after_add = manager.system_prompt_extras_revision();
        assert_ne!(after_add, start, "新增一格必须推进版本");

        manager.add_system_prompt_extra("expert".to_string(), "A".to_string());
        assert_eq!(
            manager.system_prompt_extras_revision(),
            after_add,
            "同 key 同正文重写不得推进版本"
        );

        manager.add_system_prompt_extra("expert".to_string(), "B".to_string());
        let after_replace = manager.system_prompt_extras_revision();
        assert_ne!(after_replace, after_add, "同 key 换正文必须推进版本");

        manager.remove_system_prompt_extra("absent");
        assert_eq!(
            manager.system_prompt_extras_revision(),
            after_replace,
            "撤一个不存在的 key 不得推进版本"
        );

        manager.remove_system_prompt_extra("expert");
        assert_ne!(
            manager.system_prompt_extras_revision(),
            after_replace,
            "真的撤掉一格必须推进版本"
        );
    }

    #[test]
    fn test_clear_system_prompt_override() {
        let manager = PromptManager::new();
        let mut session = session_with_override("Replacement prompt");
        assert!(manager
            .builder()
            .with_session(&session)
            .build()
            .contains("Replacement prompt"));

        session.system_prompt_override = None;
        assert!(!manager
            .builder()
            .with_session(&session)
            .build()
            .contains("Replacement prompt"));
    }

    #[test]
    fn test_build_system_prompt_preserves_legitimate_unicode_in_extras() {
        let legitimate_unicode = "Instruction with 世界 and 🌍 emojis";

        let result = PromptManager::new()
            .builder()
            .with_prompt_extras([("test".to_string(), legitimate_unicode.to_string())])
            .build();

        assert!(result.contains("世界"));
        assert!(result.contains("🌍"));
        assert!(result.contains("Instruction with"));
        assert!(result.contains("emojis"));
    }

    #[test]
    fn test_build_system_prompt_sanitizes_extension_instructions() {
        let manager = PromptManager::new();
        let malicious_extension_info = ExtensionInfo::new(
            "test_extension",
            "Extension help\u{E0041}\u{E0042}\u{E0043}hidden instructions",
            false,
        );

        let result = manager
            .builder()
            .with_extension(malicious_extension_info)
            .build();

        assert!(!result.contains('\u{E0041}'));
        assert!(!result.contains('\u{E0042}'));
        assert!(!result.contains('\u{E0043}'));
        assert!(result.contains("Extension help"));
        assert!(result.contains("hidden instructions"));
    }

    #[test]
    fn test_basic() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager.builder().build();

        assert_snapshot!(system_prompt)
    }

    #[test]
    fn test_one_extension() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager
            .builder()
            .with_extension(ExtensionInfo::new(
                "test",
                "how to use this extension",
                true,
            ))
            .build();

        assert_snapshot!(system_prompt)
    }

    #[test]
    fn test_typical_setup() {
        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let system_prompt = manager
            .builder()
            .with_extension(ExtensionInfo::new(
                "extension_A",
                "<instructions on how to use extension A>",
                true,
            ))
            .with_extension(ExtensionInfo::new(
                "extension_B",
                "<instructions on how to use extension B (no resources)>",
                false,
            ))
            .build();

        assert_snapshot!(system_prompt)
    }

    #[tokio::test]
    async fn test_all_platform_extensions() {
        use crate::agents::platform_extensions::{PlatformExtensionContext, PLATFORM_EXTENSIONS};
        use crate::config::GooseMode;
        use crate::session::SessionManager;
        use std::sync::Arc;

        let tmp_dir = tempfile::tempdir().unwrap();
        let temp_root = tmp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([
            ("HOME", Some(temp_root.as_str())),
            ("GOOSE_PATH_ROOT", Some(temp_root.as_str())),
        ]);
        let session_manager = Arc::new(SessionManager::new(tmp_dir.path().to_path_buf()));
        let session = session_manager
            .create_session(
                tmp_dir.path().to_path_buf(),
                "test session".to_owned(),
                crate::session::SessionType::Hidden,
                GooseMode::default(),
            )
            .await
            .unwrap();
        let scheduler = crate::scheduler::Scheduler::new(
            tmp_dir.path().join("schedules"),
            session_manager.clone(),
        )
        .await
        .unwrap();
        let context = PlatformExtensionContext {
            extension_manager: None,
            providers: Default::default(),
            session_manager,
            scheduler: Some(scheduler),
            use_login_shell_path: false,
        };

        let mut extensions = Vec::new();
        for def in PLATFORM_EXTENSIONS.values() {
            let Some(client) = (def.client_factory)(context.clone()) else {
                continue;
            };
            let instructions = client
                .get_instructions(&session.id, &session.working_dir)
                .await
                .unwrap_or_default();
            let has_resources = client
                .get_info()
                .and_then(|i| i.capabilities.resources.as_ref())
                .is_some();
            extensions.push(ExtensionInfo::new(def.name, &instructions, has_resources));
        }

        extensions.sort_by(|a, b| a.name.cmp(&b.name));

        let manager = PromptManager::with_timestamp(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let system_prompt = manager
            .builder()
            .with_extensions(extensions.into_iter())
            .build();

        assert_snapshot!(system_prompt);
    }

    /// 唤星 `PATCHES.md` `#2` 的夹具：git 根里一份、工作目录里两份（其一带 `@引用`）、
    /// 子目录一份。
    fn context_file_layout() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let work = project.join("work");
        std::fs::create_dir_all(project.join(".git")).unwrap();
        std::fs::create_dir_all(work.join("nested")).unwrap();
        std::fs::write(
            project.join(crate::hints::AGENTS_MD_FILENAME),
            "PARENT_HINT",
        )
        .unwrap();
        std::fs::write(
            work.join(crate::hints::AGENTS_MD_FILENAME),
            "ROOT_HINT\n@referenced.md",
        )
        .unwrap();
        std::fs::write(work.join("referenced.md"), "REFERENCED_HINT").unwrap();
        std::fs::write(
            work.join(crate::hints::GOOSE_HINTS_FILENAME),
            "GOOSEHINTS_HINT",
        )
        .unwrap();
        std::fs::write(
            work.join("nested").join(crate::hints::AGENTS_MD_FILENAME),
            "NESTED_HINT",
        )
        .unwrap();
        (temp, work)
    }

    fn nested_tool_arguments() -> Option<serde_json::Map<String, serde_json::Value>> {
        serde_json::json!({ "path": "nested/file.rs" })
            .as_object()
            .cloned()
    }

    fn base_prompt_with_hints(manager: &PromptManager, working_dir: &Path) -> String {
        manager
            .builder()
            .with_session(&session_with_override("BASE"))
            .with_goose_mode(GooseMode::Auto)
            .with_hints(working_dir)
            .build()
    }

    /// 显式空表 ⇒ 工作目录（向上到 git 根、含 `@引用`）与子目录两条读路**一个 hints 都不读**：
    /// 产出的 system prompt 与「根本没有 hints」逐字节相同。
    #[test]
    fn explicit_empty_context_file_names_read_no_hint_files() {
        let (_temp, work) = context_file_layout();
        let mut manager = PromptManager::with_context_file_names(Some(Vec::new()));

        assert_eq!(base_prompt_with_hints(&manager, &work), "BASE");

        manager.record_tool_arguments(&nested_tool_arguments(), &work);
        assert!(
            !manager.load_subdirectory_hints(&work),
            "空表之下子目录跟踪器仍然读出了 hints"
        );
        assert_eq!(base_prompt_with_hints(&manager, &work), "BASE");
    }

    /// 显式非空表 ⇒ **只**认这张表，全局配置的缺省表（含 `AGENTS.md`）不再生效。
    #[test]
    fn explicit_context_file_names_replace_the_global_list() {
        let (_temp, work) = context_file_layout();
        let mut manager = PromptManager::with_context_file_names(Some(vec![
            crate::hints::GOOSE_HINTS_FILENAME.to_string(),
        ]));

        let prompt = base_prompt_with_hints(&manager, &work);
        assert!(prompt.contains("GOOSEHINTS_HINT"), "给定的文件名没被读到");
        for absent in ["ROOT_HINT", "PARENT_HINT", "REFERENCED_HINT"] {
            assert!(!prompt.contains(absent), "表外的文件 `{absent}` 进了提示词");
        }

        manager.record_tool_arguments(&nested_tool_arguments(), &work);
        assert!(
            !manager.load_subdirectory_hints(&work),
            "子目录里只有表外的 AGENTS.md，却读出了 hints"
        );
    }

    /// 剥掉全局 hints 块（`### Global Hints` 起、`### Project Hints` 止）。
    ///
    /// 全局块读的是**进程级**的 `GOOSE_PATH_ROOT` / home，而同一测试进程里的
    /// `hints::load_hints::tests::test_global_agents_md_skipped_when_not_in_context_file_names`
    /// 裸 `set_var` 改它、不持锁 ⇒ 并行时同一条用例前后两次读会读到不同的全局块（实测撞到过）。
    /// 被剥掉的块与项目块出自同一次 `load_hint_files`、用的是同一张文件名表，
    /// 文件名表一旦不同，项目块照样对不上。
    fn without_global_hints(prompt: &str) -> String {
        let Some((head, rest)) = prompt.split_once("\n### Global Hints\n") else {
            return prompt.to_string();
        };
        match rest.split_once("### Project Hints") {
            Some((_, project)) => format!("{head}### Project Hints{project}"),
            None => prompt.to_string(),
        }
    }

    /// `None` ⇒ 与改前**逐字节相同**（非真空对照）。
    ///
    /// 参照物照改前的代码逐步复原：文件名取 `get_context_filenames()`、经 `load_hint_files`
    /// 读出并以 `hints` 为 key 追加；子目录那一支由 `SubdirectoryHintTracker::new()` 读出、
    /// 以它自己的 key 进 extras。⛔ 参照物一步都不经过本 patch 改过的 `with_hints`。
    /// 唯一的让步是比较前剥掉全局 hints 块，理由见 `without_global_hints`。
    #[test]
    fn unset_context_file_names_match_the_global_config_behaviour_byte_for_byte() {
        let (_temp, work) = context_file_layout();
        let root_hints = load_hint_files(&work, &get_context_filenames(), &build_gitignore(&work));
        for present in [
            "ROOT_HINT",
            "PARENT_HINT",
            "REFERENCED_HINT",
            "GOOSEHINTS_HINT",
        ] {
            assert!(
                root_hints.contains(present),
                "非真空对照不成立：缺省行为本该读到 `{present}`"
            );
        }
        let mut tracker = SubdirectoryHintTracker::new();
        tracker.record_tool_arguments(&nested_tool_arguments(), &work);
        let nested = tracker.load_new_hints(&work);
        assert!(
            nested
                .iter()
                .any(|(_, content)| content.contains("NESTED_HINT")),
            "非真空对照不成立：缺省行为本该在工具调用后读到子目录 hints"
        );

        let mut reference = PromptManager::new();
        let reference_prompt = |reference: &PromptManager| {
            reference
                .builder()
                .with_session(&session_with_override("BASE"))
                .with_goose_mode(GooseMode::Auto)
                .with_prompt_extras([("hints".to_string(), root_hints.clone())])
                .build()
        };
        let expected_before_tool = reference_prompt(&reference);
        for (key, content) in nested {
            reference.add_system_prompt_extra(key, content);
        }
        let expected_after_tool = reference_prompt(&reference);

        for mut manager in [
            PromptManager::new(),
            PromptManager::with_context_file_names(None),
        ] {
            assert_eq!(
                without_global_hints(&base_prompt_with_hints(&manager, &work)),
                without_global_hints(&expected_before_tool)
            );
            manager.record_tool_arguments(&nested_tool_arguments(), &work);
            assert!(manager.load_subdirectory_hints(&work));
            assert_eq!(
                without_global_hints(&base_prompt_with_hints(&manager, &work)),
                without_global_hints(&expected_after_tool)
            );
        }
    }
}
