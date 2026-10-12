//! 唤星 PATCHES.md #9：文件系统技能发现档位（`AgentConfig::filesystem_skills`）在状态机
//! 上的行为——默认 `Disabled` 全关；`WorkingDirectory` 只放工作目录来源的技能。

use anyhow::Result;
use serde_json::json;

use super::pipeline::{test_pipeline, MessageKind::Agent, MessageKind::ToolResponse};
use crate::agents::FilesystemSkills;

/// 在工作目录的 `.agents/skills/<name>/` 放一个技能。
fn install_workdir_skill(working_dir: &std::path::Path, name: &str, description: &str, body: &str) {
    let skill_dir = working_dir.join(".agents/skills").join(name);
    std::fs::create_dir_all(&skill_dir).expect("skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n{body}"),
    )
    .expect("skill file");
}

/// 在全局配置目录（`$GOOSE_PATH_ROOT/config/skills`）放一个技能，作为「全局来源」探针：
/// 它不带 home，不需要碰真实 `~`，过滤若失效它必然漏进 `# Skills`。
fn install_global_config_skill(
    path_root: &std::path::Path,
    name: &str,
    description: &str,
    body: &str,
) {
    let skill_dir = path_root.join("config/skills").join(name);
    std::fs::create_dir_all(&skill_dir).expect("global skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n{body}"),
    )
    .expect("global skill file");
}

/// 默认（Disabled）：工作目录里放了技能，状态机推理也不广告 `load_skill`、不注入 `# Skills`。
#[tokio::test]
async fn disabled_by_default_hides_load_skill_and_skills_section() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    install_workdir_skill(
        pipeline.working_dir(),
        "demo",
        "DEMO_SKILL_DESC",
        "DEMO_SKILL_BODY",
    );
    api.on("hello").reply("hi");

    pipeline.run(["hello"]).await?;

    let calls = api.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        !calls[0].advertises_tool("load_skill"),
        "默认档位（Disabled）不得广告 load_skill"
    );
    assert!(
        !calls[0].system_contains("# Skills"),
        "默认档位（Disabled）不得注入 # Skills 段"
    );
    assert!(
        !calls[0].system_contains("DEMO_SKILL_DESC"),
        "默认档位（Disabled）下工作目录技能不得出现在系统提示词"
    );
    Ok(())
}

/// Disabled：模型硬发未广告的 `load_skill`，具名失败（UnknownToolOperation 的
/// 「not available」），且技能正文不得进入会话。
#[tokio::test]
async fn disabled_mode_hard_sent_load_skill_fails_by_name_without_reading_files() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    install_workdir_skill(
        pipeline.working_dir(),
        "demo",
        "DEMO_SKILL_DESC",
        "DEMO_SKILL_BODY",
    );
    api.on("use the demo skill")
        .unadvertised_call("load_skill", json!({ "name": "demo" }));
    api.on("not available").reply("understood");

    let result = pipeline.run(["use the demo skill"]).await?;

    result.assert_message(-2, ToolResponse, "Tool 'load_skill' is not available");
    result.assert_message(-1, Agent, "understood");
    assert!(
        !result
            .conversation()
            .messages()
            .iter()
            .any(|message| message.as_concat_text().contains("DEMO_SKILL_BODY")),
        "Disabled 档位下技能正文不得进入会话"
    );
    Ok(())
}

/// WorkingDirectory：广告 `load_skill`，`# Skills` 段列工作目录技能；全局配置目录
/// （`$GOOSE_PATH_ROOT/config/skills`，`SourceEntry.global == true`）里的技能不出现。
/// 用 `GOOSE_PATH_ROOT` 充当局全局探针（`Paths::config_dir()` 每次现读该环境变量），
/// 不碰真实 home。
#[tokio::test]
async fn working_directory_mode_lists_workdir_skill_and_hides_global_skills() -> Result<()> {
    let path_root = tempfile::tempdir()?;
    install_global_config_skill(
        path_root.path(),
        "global-only",
        "GLOBAL_ONLY_DESC",
        "GLOBAL_ONLY_BODY",
    );
    let _guard = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", path_root.path().to_str()),
        ("PLUGINS", None),
    ]);
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_filesystem_skills(FilesystemSkills::WorkingDirectory);
    install_workdir_skill(
        pipeline.working_dir(),
        "demo",
        "DEMO_SKILL_DESC",
        "DEMO_SKILL_BODY",
    );
    api.on("hello").reply("hi");

    pipeline.run(["hello"]).await?;

    let calls = api.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].advertises_tool("load_skill"),
        "WorkingDirectory 档位必须广告 load_skill"
    );
    assert!(
        calls[0].system_contains("# Skills"),
        "WorkingDirectory 档位必须注入 # Skills 段"
    );
    assert!(
        calls[0].system_contains("DEMO_SKILL_DESC"),
        "工作目录技能必须出现在 # Skills 段"
    );
    assert!(
        !calls[0].system_contains("GLOBAL_ONLY_DESC"),
        "全局配置目录技能不得出现在 # Skills 段"
    );
    Ok(())
}

/// WorkingDirectory：`load_skill` 执行路径真能读到工作目录技能的正文。
#[tokio::test]
async fn working_directory_mode_load_skill_reads_workdir_skill_content() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_filesystem_skills(FilesystemSkills::WorkingDirectory);
    install_workdir_skill(
        pipeline.working_dir(),
        "demo",
        "DEMO_SKILL_DESC",
        "DEMO_SKILL_BODY",
    );
    api.on("use the demo skill")
        .call("load_skill", json!({ "name": "demo" }));
    api.on("DEMO_SKILL_BODY").reply("skill loaded");

    let result = pipeline.run(["use the demo skill"]).await?;

    result.assert_message(-2, ToolResponse, "DEMO_SKILL_BODY");
    result.assert_message(-1, Agent, "skill loaded");
    Ok(())
}
