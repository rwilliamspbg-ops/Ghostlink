use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::cluster_bridge;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Code,
    Work,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub kind: ProjectKind,
    pub root_path: String,
    pub allowed_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub created_at: String,
}

impl Project {
    pub fn effective_allowed_tools(&self) -> Vec<String> {
        if self.allowed_tools.is_empty() && self.kind == ProjectKind::Code {
            vec![
                "write_file".to_string(),
                "read_file".to_string(),
                "run_command".to_string(),
            ]
        } else {
            self.allowed_tools.clone()
        }
    }
}

/// How a project proves a change is good.
///
/// Without this, `ReviewPacket.checks` only ever contained commands the *model
/// chose* to run — so "it passed checks" meant "the model ran something", not
/// "the project's tests pass". A packet with zero checks was equally
/// acceptable, which is how unverified work reached `main`.
///
/// A plan is a list of commands the project itself declares as its definition
/// of done. They run through the same `Judge` policy as agent-issued commands,
/// so a plan cannot be used to smuggle in a denied command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationPlan {
    /// Each entry is an argv vector, e.g. `["cargo", "test", "--workspace"]`.
    #[serde(default)]
    pub commands: Vec<Vec<String>>,
    /// When true, a failing (or absent) verification blocks acceptance.
    #[serde(default)]
    pub required: bool,
}

impl VerificationPlan {
    /// Detect a plan from the project's own build files.
    ///
    /// The point is that this works with no configuration: point Ghostlink at
    /// any repository and it already knows how to check itself. Detection is
    /// deliberately conservative — it only emits commands the `Judge` allows.
    pub fn detect(root_path: &str) -> Option<VerificationPlan> {
        let root = std::path::Path::new(root_path);
        let mut commands: Vec<Vec<String>> = Vec::new();

        if root.join("Cargo.toml").exists() {
            commands.push(vec!["cargo".into(), "test".into(), "--workspace".into()]);
        }
        if root.join("package.json").exists() {
            // `npm test` is the conventional entry point; only include it when
            // the manifest actually defines a test script.
            let has_test_script = fs::read_to_string(root.join("package.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| v.get("scripts").and_then(|s| s.get("test")).cloned())
                .is_some();
            if has_test_script {
                commands.push(vec!["npx".into(), "vitest".into(), "run".into()]);
            }
        }
        if root.join("pyproject.toml").exists() || root.join("pytest.ini").exists() {
            commands.push(vec!["python3".into(), "-m".into(), "pytest".into()]);
        }

        if commands.is_empty() {
            None
        } else {
            Some(VerificationPlan {
                commands,
                required: true,
            })
        }
    }

    /// Commands in this plan that the `Judge` would refuse to run. A plan
    /// containing one is misconfigured, and we say so rather than silently
    /// executing it (the Judge is the single authority on what may run).
    pub fn disallowed_commands(&self) -> Vec<Vec<String>> {
        self.commands
            .iter()
            .filter(|argv| Judge::evaluate(argv) != JudgeResult::Allow)
            .cloned()
            .collect()
    }
}

/// Outcome of running one verification command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub argv: Vec<String>,
    pub exit: i32,
    pub passed: bool,
    /// Captured output, truncated — enough to see *why* it failed.
    pub excerpt: String,
    #[serde(default)]
    pub timed_out: bool,
}

impl VerificationResult {
    /// One-line summary suitable for a reviewer.
    pub fn summary(&self) -> String {
        let cmd = self.argv.join(" ");
        if self.timed_out {
            format!("TIMED OUT: {cmd}")
        } else if self.passed {
            format!("PASS: {cmd}")
        } else {
            format!("FAIL (exit {}): {cmd}", self.exit)
        }
    }
}

/// Run a verification plan against a project root.
///
/// Every command goes through `Judge` first — a plan cannot bypass the policy
/// that governs agent-issued commands. Each command gets its own timeout so a
/// hanging test suite cannot stall a task forever.
pub async fn run_verification_plan(
    plan: &VerificationPlan,
    root_path: &str,
    per_command_timeout: std::time::Duration,
) -> Vec<VerificationResult> {
    let mut results = Vec::new();

    for argv in &plan.commands {
        if argv.is_empty() {
            continue;
        }

        // The Judge is the single authority on what may run.
        if Judge::evaluate(argv) != JudgeResult::Allow {
            results.push(VerificationResult {
                argv: argv.clone(),
                exit: -1,
                passed: false,
                excerpt: "Refused: command is not permitted by the Judge policy".into(),
                timed_out: false,
            });
            continue;
        }

        let mut cmd = tokio::process::Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .current_dir(root_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let outcome = tokio::time::timeout(per_command_timeout, cmd.output()).await;

        match outcome {
            Err(_elapsed) => results.push(VerificationResult {
                argv: argv.clone(),
                exit: -1,
                passed: false,
                excerpt: format!("Timed out after {}s", per_command_timeout.as_secs()),
                timed_out: true,
            }),
            Ok(Err(e)) => results.push(VerificationResult {
                argv: argv.clone(),
                exit: -1,
                passed: false,
                excerpt: format!("Failed to spawn: {e}"),
                timed_out: false,
            }),
            Ok(Ok(output)) => {
                let exit = output.status.code().unwrap_or(-1);
                let mut excerpt = format!(
                    "STDOUT:\n{}\nSTDERR:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                // Keep the tail as well as the head: test failures print their
                // summary at the end, which is the part a reader needs.
                if excerpt.len() > 4000 {
                    let head = &excerpt[..2000];
                    let tail = &excerpt[excerpt.len() - 2000..];
                    excerpt = format!("{head}\n...\n(tail)\n{tail}");
                }
                results.push(VerificationResult {
                    argv: argv.clone(),
                    exit,
                    passed: exit == 0,
                    excerpt,
                    timed_out: false,
                });
            }
        }
    }

    results
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBudget {
    pub max_steps: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    pub max_minutes: u32,
}

impl Default for TaskBudget {
    fn default() -> Self {
        Self {
            max_steps: 30,
            max_tokens: Some(100_000),
            max_minutes: 15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    NeedsReview,
    Accepted,
    Rejected,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub goal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<String>,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub budget: TaskBudget,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub task_id: String,
    pub role: String,
    pub model: String,
    pub status: String,
    pub step_count: u32,
    pub token_count: u32,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDiff {
    pub path: String,
    pub unified_diff: String,
    pub original: String,
    pub proposed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JudgeDecision {
    Allow,
    Deny,
    Pause,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCommand {
    pub argv: Vec<String>,
    pub judge: JudgeDecision,
    pub exit: i32,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPacket {
    pub id: String,
    pub task_id: String,
    pub run_id: String,
    pub summary: String,
    pub diffs: Vec<ReviewDiff>,
    pub commands: Vec<ReviewCommand>,
    pub checks: Vec<String>,
    pub risks: Vec<String>,
    /// Results of running the project's own verification plan against the
    /// change. Empty means no plan was detected/configured — which is itself
    /// reported in `risks`, not silently treated as success.
    #[serde(default)]
    pub verification: Vec<VerificationResult>,
    /// Which cluster node this run was placed on, and why. `None` in
    /// single-node mode (no cluster attached) - the field is absent from
    /// the JSON in that case rather than carrying a placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementDecision>,
    /// Accelerator usage for this run. `None` in single-node mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accelerator_usage: Option<AcceleratorUsage>,
    /// Summary of the context compaction that happened during the run,
    /// if any. Present only when messages were actually dropped - a run
    /// that fit its budget carries no summary and says so by absence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_summary: Option<ContextSummary>,
    pub created_at: String,
}

/// Why a task landed on the node it did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementDecision {
    pub task_id: String,
    pub assigned_node: String,
    pub node_capability: cluster_bridge::NodeCapability,
    /// Why this node was chosen - the hint class that matched.
    pub rationale: String,
    /// Other candidates considered, by node id.
    pub alternative_nodes: Vec<String>,
}

/// What accelerator work this run actually did.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceleratorUsage {
    pub node_id: String,
    pub compute_capability: String,
    pub gpu_name: Option<String>,
    /// VRAM the node reported as free at placement time.
    pub vram_free_gb: f32,
    pub commands_executed: u32,
    pub tokens_generated: u32,
}

/// What was dropped when the transcript no longer fit the context budget.
///
/// Compaction used to be invisible: turns were dropped and only a count
/// appeared in `risks`. This carries the substance of what was dropped so
/// a reviewer can tell whether the agent lost something that mattered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub original_turn_count: usize,
    pub summary_text: String,
    pub key_decisions: Vec<String>,
    pub files_modified: Vec<String>,
}

/// Human-readable reason a placement was made, derived from the same hint
/// keywords `choose_node_for_task` matches on.
///
/// Kept in sync with that function's branches deliberately: a rationale
/// that names a class the chooser does not actually test for would be a lie
/// in the ReviewPacket, which is the one artifact a human reads before
/// accepting.
fn placement_rationale(hint: &str) -> String {
    let h = hint.to_lowercase();
    if h.contains("vision") || h.contains("image") {
        "vision/image task routed to an NPU node".to_string()
    } else if h.contains("heavy")
        || h.contains("coding")
        || h.contains("model")
        || h.contains("training")
    {
        "compute-heavy task routed to the accelerator with the most free VRAM".to_string()
    } else if h.contains("light")
        || h.contains("verification")
        || h.contains("test")
        || h.contains("lint")
    {
        "light verification routed to a CPU node to avoid occupying an accelerator".to_string()
    } else {
        "no placement hint matched; assigned the first active node".to_string()
    }
}

impl ReviewPacket {
    /// True when every verification command passed.
    ///
    /// `None` (rather than `false`) when nothing was run, so "unverified" and
    /// "verified and failed" stay distinguishable — collapsing them is how a
    /// task with no checks ends up looking as good as one that passed a suite.
    pub fn verification_passed(&self) -> Option<bool> {
        if self.verification.is_empty() {
            None
        } else {
            Some(self.verification.iter().all(|r| r.passed))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub ts: u64,
    pub kind: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeResult {
    Allow,
    Deny,
    Pause,
}

pub struct Judge;

impl Judge {
    pub fn evaluate(argv: &[String]) -> JudgeResult {
        if argv.is_empty() {
            return JudgeResult::Deny;
        }

        let cmd_str = argv.join(" ");

        // Check explicit DENY rules first
        if cmd_str.contains("rm -rf")
            || cmd_str.contains("mkfs")
            || cmd_str.contains("dd ")
            || cmd_str.contains("format ")
            || cmd_str.contains("git reset --hard")
            || cmd_str.contains("git push --force")
            || cmd_str.contains("git push -f")
            || cmd_str.contains(".git/objects")
            || cmd_str.contains(".env")
            || cmd_str.contains("id_rsa")
        {
            return JudgeResult::Deny;
        }

        let first = argv[0].as_str();
        if first == "curl"
            || first == "wget"
            || first == "scp"
            || first == "ssh"
            || first == "sudo"
            || first == "su"
            || first == "docker"
            || first == "kubectl"
        {
            return JudgeResult::Deny;
        }

        // ALLOW table check
        if Self::is_allowed(argv) {
            return JudgeResult::Allow;
        }

        // Default to PAUSE for unrecognized commands
        JudgeResult::Pause
    }

    fn is_allowed(argv: &[String]) -> bool {
        if argv.is_empty() {
            return false;
        }

        let prog = argv[0].as_str();
        match prog {
            "ls" | "rg" | "grep" => true,
            "git" => {
                if argv.len() >= 2 {
                    matches!(
                        argv[1].as_str(),
                        "status" | "diff" | "log" | "show" | "branch" | "stash"
                    )
                } else {
                    false
                }
            }
            "cargo" => {
                if argv.len() >= 2 {
                    matches!(argv[1].as_str(), "test" | "check" | "clippy")
                } else {
                    false
                }
            }
            "npx" => {
                argv.len() >= 3
                    && ((argv[1] == "vitest" && argv[2] == "run")
                        || (argv[1] == "tsc" && argv[2] == "--noEmit"))
            }
            "python" | "python3" => argv.len() >= 3 && argv[1] == "-m" && argv[2] == "pytest",
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct TaskRuntimeStore {
    data_dir: PathBuf,
    event_bus: broadcast::Sender<TaskEvent>,
    active_runs: Mutex<HashMap<String, tokio_util::sync::CancellationToken>>,
    /// Live cluster state, when this process is part of one. `None` is
    /// single-node mode and is the common case: every cluster feature then
    /// behaves exactly as it did before any of this existed.
    cluster: Option<Arc<ghostlink_core::cluster::ClusterState>>,
    /// MCP registry, when one is configured. `None` means no MCP servers
    /// are reachable and the agent toolset is the built-in one only.
    mcp_registry: Option<Arc<crate::mcp::McpRegistry>>,
}

impl TaskRuntimeStore {
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        let path = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        fs::create_dir_all(path.join("projects"))?;
        fs::create_dir_all(path.join("tasks"))?;
        fs::create_dir_all(path.join("runs"))?;
        fs::create_dir_all(path.join("reviews"))?;

        fs::create_dir_all(path.join("events"))?;
        let (event_bus, _) = broadcast::channel(64);

        Ok(Self {
            data_dir: path,
            event_bus,
            active_runs: Mutex::new(HashMap::new()),
            cluster: None,
            mcp_registry: None,
        })
    }

    /// Attach the live cluster state. Single-node deployments never call this.
    pub fn with_cluster(mut self, cluster: Arc<ghostlink_core::cluster::ClusterState>) -> Self {
        self.cluster = Some(cluster);
        self
    }

    /// Attach the MCP registry. Deployments with no MCP servers never call this.
    pub fn with_mcp_registry(mut self, registry: Arc<crate::mcp::McpRegistry>) -> Self {
        self.mcp_registry = Some(registry);
        self
    }

    /// The attached cluster, if any.
    pub fn cluster(&self) -> Option<&Arc<ghostlink_core::cluster::ClusterState>> {
        self.cluster.as_ref()
    }

    /// The attached MCP registry, if any.
    pub fn mcp_registry(&self) -> Option<&Arc<crate::mcp::McpRegistry>> {
        self.mcp_registry.as_ref()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_bus.subscribe()
    }

    pub fn emit_event(&self, event: TaskEvent) {
        let events_dir = self.data_dir.join("events");
        let file_path = events_dir.join(format!("{}.jsonl", event.task_id));
        if let Ok(json) = serde_json::to_string(&event) {
            use std::io::Write;
            if let Ok(mut file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
            {
                let _ = writeln!(file, "{}", json);
            }
        }
        let _ = self.event_bus.send(event);
    }

    pub fn get_task_events(&self, task_id: &str) -> Vec<TaskEvent> {
        let file_path = self
            .data_dir
            .join("events")
            .join(format!("{}.jsonl", task_id));
        let mut events = Vec::new();
        if file_path.exists() {
            if let Ok(content) = fs::read_to_string(file_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        if let Ok(ev) = serde_json::from_str::<TaskEvent>(trimmed) {
                            events.push(ev);
                        }
                    }
                }
            }
        }
        events
    }

    pub fn register_cancel_token(
        &self,
        task_id: String,
        token: tokio_util::sync::CancellationToken,
    ) {
        if let Ok(mut guard) = self.active_runs.try_lock() {
            guard.insert(task_id, token);
        }
    }

    pub fn cancel_task_run(&self, task_id: &str) -> bool {
        let mut cancelled = false;
        if let Ok(mut guard) = self.active_runs.try_lock() {
            if let Some(token) = guard.remove(task_id) {
                token.cancel();
                cancelled = true;
            }
        }

        // Cascade cancel to children if task_id has children
        if let Ok(task) = self.get_task(task_id) {
            if let Ok(tasks) = self.list_project_tasks(&task.project_id) {
                for child in tasks
                    .iter()
                    .filter(|t| t.parent_id.as_deref() == Some(task_id))
                {
                    let _ = self.cancel_task_run(&child.id);
                    let _ = self.update_task_status(&child.id, TaskStatus::Cancelled);
                }
            }
        }

        cancelled
    }

    fn atomic_write_json<T: Serialize>(path: &Path, val: &T) -> Result<()> {
        let temp_path = path.with_extension(format!("tmp.{}", Uuid::new_v4()));
        let json = serde_json::to_string_pretty(val)?;
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, path)?;
        Ok(())
    }

    // Projects
    pub fn create_project(
        &self,
        name: String,
        kind: ProjectKind,
        root_path: String,
        allowed_tools: Option<Vec<String>>,
        default_model: Option<String>,
    ) -> Result<Project> {
        let canonical_root = fs::canonicalize(&root_path).with_context(|| {
            format!(
                "Project root_path '{}' does not exist or is invalid",
                root_path
            )
        })?;
        if !canonical_root.is_dir() {
            return Err(anyhow!(
                "Project root_path '{}' is not a directory",
                root_path
            ));
        }

        let proj = Project {
            id: format!("proj_{}", Uuid::new_v4().simple()),
            name,
            kind,
            root_path: canonical_root.to_string_lossy().to_string(),
            allowed_tools: allowed_tools.unwrap_or_else(|| {
                vec![
                    "read_file".into(),
                    "write_file".into(),
                    "run_command".into(),
                ]
            }),
            default_model,
            created_at: Utc::now().to_rfc3339(),
        };

        let file_path = self
            .data_dir
            .join("projects")
            .join(format!("{}.json", proj.id));
        Self::atomic_write_json(&file_path, &proj)?;
        Ok(proj)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let dir = self.data_dir.join("projects");
        let mut projects = Vec::new();
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let content = fs::read_to_string(&path)?;
                    if let Ok(proj) = serde_json::from_str::<Project>(&content) {
                        projects.push(proj);
                    }
                }
            }
        }
        // OPTIMIZATION: Unstable sorting avoids scratch buffer allocations and stability tracking overhead.
        projects.sort_unstable_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(projects)
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        let file_path = self.data_dir.join("projects").join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow!("Project '{}' not found", id));
        }
        let content = fs::read_to_string(file_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn update_project(
        &self,
        id: &str,
        name: Option<String>,
        default_model: Option<String>,
        allowed_tools: Option<Vec<String>>,
    ) -> Result<Project> {
        let mut proj = self.get_project(id)?;
        if let Some(n) = name {
            proj.name = n;
        }
        if let Some(m) = default_model {
            proj.default_model = Some(m);
        }
        if let Some(t) = allowed_tools {
            proj.allowed_tools = t;
        }
        let file_path = self.data_dir.join("projects").join(format!("{}.json", id));
        Self::atomic_write_json(&file_path, &proj)?;
        Ok(proj)
    }

    // Tasks
    pub fn create_task(
        &self,
        project_id: &str,
        goal: String,
        acceptance_criteria: Option<String>,
        budget: Option<TaskBudget>,
        parent_id: Option<String>,
    ) -> Result<Task> {
        let _proj = self.get_project(project_id)?;

        // Phase B Fan-Out caps: max_depth = 1, max_children = 4
        if let Some(ref p_id) = parent_id {
            let parent_task = self.get_task(p_id)?;
            if parent_task.parent_id.is_some() {
                return Err(anyhow!("Child task depth limit exceeded (max_depth=1)"));
            }

            let existing_tasks = self.list_project_tasks(project_id)?;
            let child_count = existing_tasks
                .iter()
                .filter(|t| t.parent_id.as_deref() == Some(p_id.as_str()))
                .count();
            if child_count >= 4 {
                return Err(anyhow!(
                    "Child task count limit exceeded for parent (max_children=4)"
                ));
            }
        }

        let now = Utc::now().to_rfc3339();
        let task = Task {
            id: format!("task_{}", Uuid::new_v4().simple()),
            project_id: project_id.to_string(),
            goal,
            acceptance_criteria,
            status: TaskStatus::Queued,
            parent_id,
            budget: budget.unwrap_or_default(),
            created_at: now.clone(),
            updated_at: now,
        };

        let file_path = self
            .data_dir
            .join("tasks")
            .join(format!("{}.json", task.id));
        Self::atomic_write_json(&file_path, &task)?;
        Ok(task)
    }

    pub fn list_project_tasks(&self, project_id: &str) -> Result<Vec<Task>> {
        let dir = self.data_dir.join("tasks");
        let mut tasks = Vec::new();
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let content = fs::read_to_string(&path)?;
                    if let Ok(task) = serde_json::from_str::<Task>(&content) {
                        if task.project_id == project_id {
                            tasks.push(task);
                        }
                    }
                }
            }
        }
        // OPTIMIZATION: Unstable sorting avoids scratch buffer allocations and stability tracking overhead.
        tasks.sort_unstable_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(tasks)
    }

    pub fn list_child_tasks(&self, parent_id: &str) -> Result<Vec<Task>> {
        let parent = self.get_task(parent_id)?;
        let all_tasks = self.list_project_tasks(&parent.project_id)?;
        Ok(all_tasks
            .into_iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent_id))
            .collect())
    }

    pub fn get_task(&self, id: &str) -> Result<Task> {
        let file_path = self.data_dir.join("tasks").join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow!("Task '{}' not found", id));
        }
        let content = fs::read_to_string(file_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn update_task_status(&self, id: &str, status: TaskStatus) -> Result<Task> {
        let mut task = self.get_task(id)?;
        task.status = status;
        task.updated_at = Utc::now().to_rfc3339();
        let file_path = self.data_dir.join("tasks").join(format!("{}.json", id));
        Self::atomic_write_json(&file_path, &task)?;
        Ok(task)
    }

    pub fn requeue_task(&self, id: &str) -> Result<Task> {
        let mut task = self.get_task(id)?;
        task.status = TaskStatus::Queued;
        task.updated_at = Utc::now().to_rfc3339();
        let file_path = self.data_dir.join("tasks").join(format!("{}.json", id));
        Self::atomic_write_json(&file_path, &task)?;
        Ok(task)
    }

    // Runs
    pub fn save_run(&self, run: &AgentRun) -> Result<()> {
        let file_path = self.data_dir.join("runs").join(format!("{}.json", run.id));
        Self::atomic_write_json(&file_path, run)
    }

    #[allow(dead_code)]
    pub fn get_run(&self, id: &str) -> Result<AgentRun> {
        let file_path = self.data_dir.join("runs").join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow!("Run '{}' not found", id));
        }
        let content = fs::read_to_string(file_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    // Reviews
    pub fn save_review(&self, review: &ReviewPacket) -> Result<()> {
        let file_path = self
            .data_dir
            .join("reviews")
            .join(format!("{}.json", review.id));
        Self::atomic_write_json(&file_path, review)?;
        // Also save by task_id for quick lookup
        let task_review_path = self
            .data_dir
            .join("reviews")
            .join(format!("task_{}.json", review.task_id));
        Self::atomic_write_json(&task_review_path, review)
    }

    pub fn get_task_review(&self, task_id: &str) -> Result<ReviewPacket> {
        let task_review_path = self
            .data_dir
            .join("reviews")
            .join(format!("task_{}.json", task_id));
        if !task_review_path.exists() {
            return Err(anyhow!("No review packet found for task '{}'", task_id));
        }
        let content = fs::read_to_string(task_review_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn get_review(&self, id: &str) -> Result<ReviewPacket> {
        let file_path = self.data_dir.join("reviews").join(format!("{}.json", id));
        if !file_path.exists() {
            // Check if id is actually a task_id
            return self.get_task_review(id);
        }
        let content = fs::read_to_string(file_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    // Path Isolation & Staging Helpers
    pub fn validate_and_resolve_path(root_path: &str, relative_path: &str) -> Result<PathBuf> {
        let root = PathBuf::from(root_path);
        let target = root.join(relative_path);

        // Normalize path without requiring target file to exist yet
        let mut normalized = PathBuf::new();
        for component in target.components() {
            match component {
                std::path::Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(anyhow!("Path traversal detected in '{}'", relative_path));
                    }
                }
                std::path::Component::CurDir => {}
                c => normalized.push(c),
            }
        }

        let canonical_root =
            fs::canonicalize(&root).with_context(|| "Failed to canonicalize root_path")?;

        // Ensure normalized target stays under root
        if !normalized.starts_with(&canonical_root) && !normalized.starts_with(&root) {
            return Err(anyhow!("Path '{}' escapes project root", relative_path));
        }

        Ok(normalized)
    }

    pub fn get_task_proposed_dir(&self, project_root: &str, task_id: &str) -> PathBuf {
        PathBuf::from(project_root)
            .join(".ghostlink")
            .join("tasks")
            .join(task_id)
            .join("proposed")
    }

    /// Directory holding the pre-apply snapshot of every live file an accept
    /// is about to overwrite, so `rollback_applied_changes` can restore them.
    pub fn get_task_backup_dir(&self, project_root: &str, task_id: &str) -> PathBuf {
        PathBuf::from(project_root)
            .join(".ghostlink")
            .join("tasks")
            .join(task_id)
            .join("backup")
    }

    pub fn write_proposed_file(
        &self,
        project_root: &str,
        task_id: &str,
        relative_path: &str,
        content: &str,
    ) -> Result<PathBuf> {
        // Prevent path traversal
        let target_live_path = Self::validate_and_resolve_path(project_root, relative_path)?;
        let root = PathBuf::from(project_root);
        let rel = target_live_path
            .strip_prefix(&root)
            .map_err(|_| anyhow!("Target path outside root"))?;

        let proposed_dir = self.get_task_proposed_dir(project_root, task_id);
        let proposed_file_path = proposed_dir.join(rel);

        if let Some(parent) = proposed_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&proposed_file_path, content)?;
        Ok(proposed_file_path)
    }

    pub fn check_parent_accept_allowed(
        &self,
        project_id: &str,
        parent_task_id: &str,
    ) -> Result<()> {
        let tasks = self.list_project_tasks(project_id)?;
        let unaccepted_children: Vec<_> = tasks
            .into_iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent_task_id))
            .filter(|t| !matches!(t.status, TaskStatus::Accepted | TaskStatus::Cancelled))
            .map(|t| t.id)
            .collect();

        if !unaccepted_children.is_empty() {
            return Err(anyhow!("Parent task accept is blocked until all child tasks are accepted or cancelled (open children: {})", unaccepted_children.join(", ")));
        }
        Ok(())
    }

    /// Apply a task's staged files onto the live project root.
    ///
    /// Every live file that is about to be overwritten is copied into the
    /// task's `backup/` directory first, and a manifest of which paths
    /// existed before is written alongside it. Without this, an accept was
    /// an unrecoverable blind `fs::copy` over the user's working tree — a
    /// dirty tree had no way back. `rollback_applied_changes` reverses it.
    pub fn apply_proposed_changes(&self, project_root: &str, task_id: &str) -> Result<Vec<String>> {
        let proposed_dir = self.get_task_proposed_dir(project_root, task_id);
        let mut applied_files = Vec::new();

        if !proposed_dir.exists() {
            return Ok(applied_files);
        }

        let backup_dir = self.get_task_backup_dir(project_root, task_id);
        let _ = fs::remove_dir_all(&backup_dir);
        fs::create_dir_all(&backup_dir)
            .with_context(|| format!("Failed to create backup dir {}", backup_dir.display()))?;

        let root = PathBuf::from(project_root);
        // Relative paths that did not exist before this apply, so a rollback
        // deletes them rather than restoring an empty file.
        let mut created_paths: Vec<String> = Vec::new();

        fn visit_dirs(
            dir: &Path,
            proposed_root: &Path,
            live_root: &Path,
            backup_root: &Path,
            applied: &mut Vec<String>,
            created: &mut Vec<String>,
        ) -> Result<()> {
            if dir.is_dir() {
                for entry in fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_dir() {
                        visit_dirs(
                            &path,
                            proposed_root,
                            live_root,
                            backup_root,
                            applied,
                            created,
                        )?;
                    } else {
                        let rel = path.strip_prefix(proposed_root)?;
                        let live_target = live_root.join(rel);
                        let rel_str = rel.to_string_lossy().to_string();

                        if live_target.exists() {
                            // Snapshot the current contents before clobbering.
                            let backup_target = backup_root.join(rel);
                            if let Some(parent) = backup_target.parent() {
                                fs::create_dir_all(parent)?;
                            }
                            fs::copy(&live_target, &backup_target).with_context(|| {
                                format!("Failed to back up {}", live_target.display())
                            })?;
                        } else {
                            created.push(rel_str.clone());
                        }

                        if let Some(parent) = live_target.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::copy(&path, &live_target)?;
                        applied.push(rel_str);
                    }
                }
            }
            Ok(())
        }

        visit_dirs(
            &proposed_dir,
            &proposed_dir,
            &root,
            &backup_dir,
            &mut applied_files,
            &mut created_paths,
        )?;

        // Record which applied paths were newly created, so rollback knows to
        // delete them instead of restoring from an (absent) snapshot.
        let manifest = serde_json::json!({
            "task_id": task_id,
            "applied": applied_files,
            "created": created_paths,
            "created_at": Utc::now().to_rfc3339(),
        });
        let manifest_path = backup_dir.join("manifest.json");
        let _ = Self::atomic_write_json(&manifest_path, &manifest);

        // Clean up proposed directory after successful apply
        let _ = fs::remove_dir_all(&proposed_dir);

        Ok(applied_files)
    }

    /// Undo a previous `apply_proposed_changes` for this task.
    ///
    /// Restores every file captured in the task's `backup/` snapshot and
    /// deletes the paths the apply created. Returns the list of paths that
    /// were restored or removed. Errors if there is no snapshot to roll back
    /// to, rather than silently reporting success.
    pub fn rollback_applied_changes(
        &self,
        project_root: &str,
        task_id: &str,
    ) -> Result<Vec<String>> {
        let backup_dir = self.get_task_backup_dir(project_root, task_id);
        let manifest_path = backup_dir.join("manifest.json");

        if !manifest_path.exists() {
            return Err(anyhow!(
                "No apply snapshot for task '{}' — nothing to roll back",
                task_id
            ));
        }

        let raw = fs::read_to_string(&manifest_path)?;
        let manifest: serde_json::Value = serde_json::from_str(&raw)?;

        let root = PathBuf::from(project_root);
        let mut touched = Vec::new();

        // Delete paths that this apply created.
        if let Some(created) = manifest.get("created").and_then(|c| c.as_array()) {
            for rel in created.iter().filter_map(|v| v.as_str()) {
                if let Ok(target) = Self::validate_and_resolve_path(project_root, rel) {
                    if target.exists() {
                        let _ = fs::remove_file(&target);
                    }
                    touched.push(rel.to_string());
                }
            }
        }

        // Restore paths that existed before the apply.
        fn restore_dirs(
            dir: &Path,
            backup_root: &Path,
            live_root: &Path,
            touched: &mut Vec<String>,
        ) -> Result<()> {
            if !dir.is_dir() {
                return Ok(());
            }
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    restore_dirs(&path, backup_root, live_root, touched)?;
                } else if path
                    .file_name()
                    .map(|n| n == "manifest.json")
                    .unwrap_or(false)
                {
                    continue;
                } else {
                    let rel = path.strip_prefix(backup_root)?;
                    let live_target = live_root.join(rel);
                    if let Some(parent) = live_target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(&path, &live_target)?;
                    touched.push(rel.to_string_lossy().to_string());
                }
            }
            Ok(())
        }

        restore_dirs(&backup_dir, &backup_dir, &root, &mut touched)?;

        // Snapshot consumed — remove it so a second rollback errors instead of
        // silently re-restoring stale content.
        let _ = fs::remove_dir_all(&backup_dir);

        Ok(touched)
    }

    pub fn discard_proposed_changes(&self, project_root: &str, task_id: &str) -> Result<()> {
        let proposed_dir = self.get_task_proposed_dir(project_root, task_id);
        if proposed_dir.exists() {
            fs::remove_dir_all(&proposed_dir)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// Real token usage for this turn when the backend reports it
    /// (prompt + completion). `None` means "unknown" — the runtime then falls
    /// back to a character-based estimate rather than pretending it knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_tokens: Option<u32>,
}

#[allow(clippy::double_must_use)]
#[async_trait::async_trait]
pub trait AgentBackend: Send + Sync {
    async fn chat(
        &self,
        messages: &[serde_json::Value],
        allowed_tools: &[String],
    ) -> Result<AgentResponse>;
}

/// Produce a real unified diff between `original` and `proposed`.
///
/// `ReviewPacket.diffs[].unified_diff` previously carried a hand-built string
/// with a hardcoded `@@ -0,0 +1,3 @@` hunk header regardless of the actual
/// content, so a reviewer saw fabricated line numbers. This computes an
/// LCS-based diff with correct hunk headers instead of pulling in a new
/// dependency for one function.
///
/// Returns an empty string when the two sides are identical.
pub fn unified_diff(rel_path: &str, original: &str, proposed: &str) -> String {
    if original == proposed {
        return String::new();
    }

    let a: Vec<&str> = original.lines().collect();
    let b: Vec<&str> = proposed.lines().collect();

    // LCS table over lines.
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    // Walk the table emitting context / deletions / additions.
    #[derive(PartialEq)]
    enum Op {
        Ctx,
        Del,
        Add,
    }
    let mut ops: Vec<(Op, &str)> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push((Op::Ctx, a[i]));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            ops.push((Op::Del, a[i]));
            i += 1;
        } else {
            ops.push((Op::Add, b[j]));
            j += 1;
        }
    }
    while i < n {
        ops.push((Op::Del, a[i]));
        i += 1;
    }
    while j < m {
        ops.push((Op::Add, b[j]));
        j += 1;
    }

    // Group into hunks with 3 lines of context, tracking real line numbers.
    const CTX: usize = 3;
    let mut out = String::new();
    out.push_str(&format!("--- a/{rel_path}\n+++ b/{rel_path}\n"));

    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (op, _))| *op != Op::Ctx)
        .map(|(idx, _)| idx)
        .collect();

    if changed.is_empty() {
        return out;
    }

    // Build hunk ranges [start, end) over `ops`.
    let mut hunks: Vec<(usize, usize)> = Vec::new();
    let mut start = changed[0].saturating_sub(CTX);
    let mut end = (changed[0] + CTX + 1).min(ops.len());
    for &idx in &changed[1..] {
        if idx <= end + CTX {
            end = (idx + CTX + 1).min(ops.len());
        } else {
            hunks.push((start, end));
            start = idx.saturating_sub(CTX);
            end = (idx + CTX + 1).min(ops.len());
        }
    }
    hunks.push((start, end));

    for (hs, he) in hunks {
        // Line numbers at the start of this hunk (1-based, as of the first
        // op in the range).
        let old_start = 1 + ops[..hs].iter().filter(|(o, _)| *o != Op::Add).count();
        let new_start = 1 + ops[..hs].iter().filter(|(o, _)| *o != Op::Del).count();
        let old_len = ops[hs..he].iter().filter(|(o, _)| *o != Op::Add).count();
        let new_len = ops[hs..he].iter().filter(|(o, _)| *o != Op::Del).count();

        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start, old_len, new_start, new_len
        ));
        for (op, line) in &ops[hs..he] {
            let prefix = match op {
                Op::Ctx => ' ',
                Op::Del => '-',
                Op::Add => '+',
            };
            out.push(prefix);
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

/// Context-window policy for the agent loop.
///
/// The loop's `messages` vector grows by one assistant turn plus one tool-result
/// turn per tool call, with no upper bound — only individual tool *outputs* were
/// truncated. On a local model with a small `n_ctx` a long task therefore hit a
/// context overflow instead of compacting, and the backend call failed.
///
/// This is a real budget: the system prompt and the original task prompt are
/// pinned (dropping them loses the task), and the most recent turns are kept up
/// to whatever budget remains. Older turns are dropped oldest-first.
#[derive(Debug, Clone)]
pub struct ContextGovernor {
    /// Total token budget for the assembled message list.
    pub token_budget: usize,
    /// Tokens reserved for the model's own completion, subtracted from the
    /// budget before history is packed.
    pub reserve_completion_tokens: usize,
    /// Always-kept recent turns, even if they exceed the remaining budget.
    pub keep_last_turns: usize,
}

impl Default for ContextGovernor {
    fn default() -> Self {
        Self {
            token_budget: 4096,
            reserve_completion_tokens: 512,
            keep_last_turns: 6,
        }
    }
}

impl ContextGovernor {
    /// Approximate token count for a message. Uses the same
    /// chars/4 heuristic as the rest of the runtime; `usage_tokens` from the
    /// backend is preferred for *accounting*, but a cheap estimate is what's
    /// needed for a pre-flight packing decision.
    fn message_tokens(msg: &serde_json::Value) -> usize {
        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
        let tool_calls = msg
            .get("tool_calls")
            .map(|t| t.to_string().len())
            .unwrap_or(0);
        ((content.len() + tool_calls) / 4).max(1)
    }

    /// Total estimated tokens across a message list.
    pub fn estimate_tokens(messages: &[serde_json::Value]) -> usize {
        messages.iter().map(Self::message_tokens).sum()
    }

    /// Pack `messages` into the budget, returning the kept list and how many
    /// messages were dropped.
    ///
    /// The first message (system prompt) and the second (the original task
    /// prompt) are pinned — they carry the goal and are never dropped, even if
    /// they alone exceed the budget. The most recent `keep_last_turns` turns
    /// are also always kept. Everything in between is dropped oldest-first
    /// until the remainder fits.
    pub fn fit(&self, messages: &[serde_json::Value]) -> (Vec<serde_json::Value>, usize) {
        // System + original task prompt are the pinned prefix.
        let pinned_prefix = messages.len().min(2);
        let pinned: Vec<serde_json::Value> = messages[..pinned_prefix].to_vec();
        let rest = &messages[pinned_prefix..];

        if rest.is_empty() {
            return (pinned, 0);
        }

        let budget = self
            .token_budget
            .saturating_sub(self.reserve_completion_tokens);
        let pinned_cost = Self::estimate_tokens(&pinned);
        let mut remaining = budget.saturating_sub(pinned_cost);

        // Walk backwards, always keeping the last `keep_last_turns` messages,
        // then as many older ones as fit.
        let keep_always = self.keep_last_turns.min(rest.len());
        let mut kept_from_rest: Vec<serde_json::Value> = Vec::new();

        for (idx, msg) in rest.iter().enumerate().rev() {
            let cost = Self::message_tokens(msg);
            let is_recent = idx >= rest.len() - keep_always;
            if is_recent {
                remaining = remaining.saturating_sub(cost);
                kept_from_rest.push(msg.clone());
            } else if cost <= remaining {
                remaining -= cost;
                kept_from_rest.push(msg.clone());
            } else {
                // Older message doesn't fit — stop scanning; everything before
                // it is older still.
                break;
            }
        }
        kept_from_rest.reverse();

        let dropped = rest.len() - kept_from_rest.len();
        let mut out = pinned;
        out.extend(kept_from_rest);
        (out, dropped)
    }
}

// Helper to recursively list relative paths in a directory
fn list_dir_relative(
    base_dir: &Path,
    current_dir: &Path,
    rel_paths: &mut Vec<String>,
) -> Result<()> {
    if !current_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            list_dir_relative(base_dir, &path, rel_paths)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(base_dir) {
                rel_paths.push(rel.to_string_lossy().to_string());
            }
        }
    }
    Ok(())
}

// Implementer Loop Runner
pub struct TaskRunner;

impl TaskRunner {
    #[allow(dead_code)]
    pub async fn spawn_implementer_joined(
        store: Arc<TaskRuntimeStore>,
        backend: Arc<dyn AgentBackend>,
        task: Task,
        role: String,
        model: String,
        brief: Option<String>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<AgentRun> {
        let now = Utc::now().to_rfc3339();
        let run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role,
            model,
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: now,
            finished_at: None,
            error: None,
        };

        store.save_run(&run)?;
        store.update_task_status(&task.id, TaskStatus::Running)?;
        store.register_cancel_token(task.id.clone(), cancel_token.clone());

        let mut run_clone = run.clone();
        let res = Self::run_loop(
            store.clone(),
            backend,
            task.clone(),
            &mut run_clone,
            brief,
            cancel_token,
        )
        .await;

        let finished_at = Utc::now().to_rfc3339();
        run_clone.finished_at = Some(finished_at.clone());

        match res {
            Ok((review, final_status)) => {
                run_clone.status = "finished".into();
                let _ = store.save_run(&run_clone);
                let _ = store.save_review(&review);
                let _ = store.update_task_status(&task.id, final_status);

                store.emit_event(TaskEvent {
                    task_id: task.id.clone(),
                    ts: Utc::now().timestamp_millis() as u64,
                    kind: "review_ready".into(),
                    payload: serde_json::to_value(&review).unwrap_or_default(),
                });
                Ok(run_clone)
            }
            Err(err) => {
                let err_msg = err.to_string();
                let is_cancelled = err_msg.contains("cancelled");
                run_clone.status = if is_cancelled {
                    "cancelled".into()
                } else {
                    "failed".into()
                };
                run_clone.error = Some(err_msg.clone());
                let _ = store.save_run(&run_clone);

                let status = if is_cancelled {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Blocked
                };
                let _ = store.update_task_status(&task.id, status);

                store.emit_event(TaskEvent {
                    task_id: task.id.clone(),
                    ts: Utc::now().timestamp_millis() as u64,
                    kind: if is_cancelled {
                        "cancelled".into()
                    } else {
                        "error".into()
                    },
                    payload: serde_json::json!({ "error": err_msg }),
                });

                Err(anyhow!(err_msg))
            }
        }
    }
    pub async fn spawn_implementer(
        store: Arc<TaskRuntimeStore>,
        backend: Arc<dyn AgentBackend>,
        task: Task,
        role: String,
        model: String,
        brief: Option<String>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<AgentRun> {
        let now = Utc::now().to_rfc3339();
        let run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role,
            model,
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: now,
            finished_at: None,
            error: None,
        };

        store.save_run(&run)?;
        store.update_task_status(&task.id, TaskStatus::Running)?;
        store.register_cancel_token(task.id.clone(), cancel_token.clone());

        let store_clone = Arc::clone(&store);
        let task_clone = task.clone();
        let mut run_clone = run.clone();

        tokio::spawn(async move {
            let res = Self::run_loop(
                store_clone.clone(),
                backend,
                task_clone.clone(),
                &mut run_clone,
                brief,
                cancel_token,
            )
            .await;

            let finished_at = Utc::now().to_rfc3339();
            run_clone.finished_at = Some(finished_at.clone());

            match res {
                Ok((review, final_status)) => {
                    run_clone.status = "finished".into();
                    let _ = store_clone.save_run(&run_clone);
                    let _ = store_clone.save_review(&review);
                    let _ = store_clone.update_task_status(&task_clone.id, final_status);

                    store_clone.emit_event(TaskEvent {
                        task_id: task_clone.id.clone(),
                        ts: Utc::now().timestamp_millis() as u64,
                        kind: "review_ready".into(),
                        payload: serde_json::to_value(&review).unwrap_or_default(),
                    });
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    let is_cancelled = err_msg.contains("cancelled");
                    run_clone.status = if is_cancelled {
                        "cancelled".into()
                    } else {
                        "failed".into()
                    };
                    run_clone.error = Some(err_msg.clone());
                    let _ = store_clone.save_run(&run_clone);

                    let status = if is_cancelled {
                        TaskStatus::Cancelled
                    } else {
                        TaskStatus::Blocked
                    };
                    let _ = store_clone.update_task_status(&task_clone.id, status);

                    store_clone.emit_event(TaskEvent {
                        task_id: task_clone.id.clone(),
                        ts: Utc::now().timestamp_millis() as u64,
                        kind: if is_cancelled {
                            "cancelled".into()
                        } else {
                            "error".into()
                        },
                        payload: serde_json::json!({ "error": err_msg }),
                    });
                }
            }
        });

        Ok(run)
    }

    #[allow(clippy::type_complexity)]
    fn run_loop<'a>(
        store: Arc<TaskRuntimeStore>,
        backend: Arc<dyn AgentBackend>,
        task: Task,
        run: &'a mut AgentRun,
        brief: Option<String>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(ReviewPacket, TaskStatus)>> + Send + 'a>,
    > {
        Box::pin(async move {
            let project = store.get_project(&task.project_id)?;

            store.emit_event(TaskEvent {
                task_id: task.id.clone(),
                ts: Utc::now().timestamp_millis() as u64,
                kind: "run_started".into(),
                payload: serde_json::json!({ "run_id": run.id, "brief": brief }),
            });

            let mut commands_executed = Vec::new();
            let mut checks = Vec::new();
            let mut risks = Vec::new();
            let mut final_status = TaskStatus::NeedsReview;

            let start_time = std::time::Instant::now();

            // Build initial messages
            //
            // Cluster topology goes into the system prompt when a cluster
            // is attached, so the model can reason about where work should
            // run. In single-node mode `query_cluster_nodes` returns
            // nothing and the prompt is byte-identical to before.
            let cluster_nodes = match store.cluster() {
                Some(cluster) => cluster_bridge::query_cluster_nodes(cluster).await,
                None => Vec::new(),
            };
            let cluster_prompt = cluster_bridge::format_cluster_prompt(&cluster_nodes);

            let system_prompt = format!(
            "You are an AI task agent implementing project '{}' at root path '{}'. Use available tools to write proposed files and execute checks.{}",
            project.name, project.root_path, cluster_prompt
        );

            let mut user_prompt = format!("Task Goal: {}\n", task.goal);
            if let Some(b) = &brief {
                user_prompt.push_str(&format!("Execution Brief: {}\n", b));
            }
            if let Some(ac) = &task.acceptance_criteria {
                user_prompt.push_str(&format!("Acceptance Criteria: {}\n", ac));
            }

            let mut messages = vec![
                serde_json::json!({ "role": "system", "content": system_prompt }),
                serde_json::json!({ "role": "user", "content": user_prompt }),
            ];

            let max_steps = task.budget.max_steps;
            let max_minutes = task.budget.max_minutes as u64;

            // ---- Placement -----------------------------------------
            //
            // Decide where this run executes before the first model call,
            // and say so over SSE. `None` in single-node mode: no cluster
            // attached means no placement decision and no event, which is
            // exactly the previous behaviour.
            let placement = if cluster_nodes.is_empty() {
                None
            } else {
                let hint = format!(
                    "{} {}",
                    task.goal,
                    task.acceptance_criteria.as_deref().unwrap_or("")
                );
                match cluster_bridge::choose_node_for_task(&cluster_nodes, &hint) {
                    Some(node) => {
                        let alternatives: Vec<String> = cluster_nodes
                            .iter()
                            .filter(|n| n.node_id != node.node_id)
                            .map(|n| n.node_id.clone())
                            .collect();
                        let decision = PlacementDecision {
                            task_id: task.id.clone(),
                            assigned_node: node.node_id.clone(),
                            node_capability: node.clone(),
                            rationale: placement_rationale(&hint),
                            alternative_nodes: alternatives,
                        };
                        store.emit_event(TaskEvent {
                            task_id: task.id.clone(),
                            ts: Utc::now().timestamp_millis() as u64,
                            kind: "placement_decided".into(),
                            payload: serde_json::to_value(&decision).unwrap_or_default(),
                        });
                        Some(decision)
                    }
                    None => None,
                }
            };

            // Real context budget for this run. Previously `messages` grew
            // unbounded and the backend call eventually failed on a context
            // overflow; now the list is packed to fit before every call.
            let governor = ContextGovernor::default();
            let mut context_dropped_total = 0usize;

            // ---- Auto-fix budget ------------------------------------
            //
            // A failing verification gets the model exactly one more
            // chance, and only while the step budget has room.
            let mut auto_fix_attempts = 0u32;
            const MAX_AUTO_FIX_ATTEMPTS: u32 = 1;

            // Outer loop enclosing the step loop AND the verification that
            // follows it. The auto-fix path `continue`s this label to give
            // the model one more chance, so the label must cover the
            // verification - labelling only the step loop would leave the
            // `continue` unreachable.
            let (review, final_status) = 'agent_loop: loop {
                while run.step_count < max_steps {
                    if cancel_token.is_cancelled() {
                        return Err(anyhow!("Task execution cancelled by user"));
                    }

                    if start_time.elapsed().as_secs() > max_minutes * 60 {
                        risks.push("Exceeded time budget (max_minutes)".into());
                        break;
                    }

                    if let Some(max_tok) = task.budget.max_tokens {
                        if run.token_count >= max_tok {
                            risks.push("Exceeded token budget (max_tokens)".into());
                            break;
                        }
                    }

                    run.step_count += 1;
                    store.emit_event(TaskEvent {
                    task_id: task.id.clone(),
                    ts: Utc::now().timestamp_millis() as u64,
                    kind: "step".into(),
                    payload: serde_json::json!({ "step": run.step_count, "description": format!("Agent step {}", run.step_count) }),
                });

                    // Call model via AgentBackend.
                    //
                    // Retried with exponential backoff: a local inference backend
                    // (llama-server mid model-swap, a transient RPC hiccup) used to
                    // fail the whole task on a single error, pushing it to
                    // `Blocked` — the same transient-failure class the RPC fabric
                    // already handles with its own backoff policy. Cancellation is
                    // honoured between attempts.
                    const MAX_CHAT_ATTEMPTS: u32 = 3;
                    let mut last_err: Option<String> = None;
                    let mut attempt_resp = None;

                    // Pack the transcript to fit the context budget before the
                    // call. The system prompt and original task prompt are pinned.
                    let (fitted, dropped) = governor.fit(&messages);
                    if dropped > 0 {
                        context_dropped_total += dropped;
                        store.emit_event(TaskEvent {
                            task_id: task.id.clone(),
                            ts: Utc::now().timestamp_millis() as u64,
                            kind: "context_compacted".into(),
                            payload: serde_json::json!({
                                "dropped_messages": dropped,
                                "kept_messages": fitted.len(),
                                "estimated_tokens": ContextGovernor::estimate_tokens(&fitted),
                                "token_budget": governor.token_budget,
                            }),
                        });
                    }

                    for attempt in 0..MAX_CHAT_ATTEMPTS {
                        if cancel_token.is_cancelled() {
                            return Err(anyhow!("Task execution cancelled by user"));
                        }
                        match backend
                            .chat(&fitted, &project.effective_allowed_tools())
                            .await
                        {
                            Ok(resp) => {
                                attempt_resp = Some(resp);
                                break;
                            }
                            Err(err) => {
                                let msg = err.to_string();
                                last_err = Some(msg.clone());
                                if attempt + 1 < MAX_CHAT_ATTEMPTS {
                                    // 1s, 2s — bounded, and short enough not to
                                    // eat a meaningful slice of max_minutes.
                                    let backoff = std::time::Duration::from_secs(1 << attempt);
                                    store.emit_event(TaskEvent {
                                        task_id: task.id.clone(),
                                        ts: Utc::now().timestamp_millis() as u64,
                                        kind: "retry".into(),
                                        payload: serde_json::json!({
                                            "attempt": attempt + 1,
                                            "max_attempts": MAX_CHAT_ATTEMPTS,
                                            "backoff_ms": backoff.as_millis() as u64,
                                            "error": msg,
                                        }),
                                    });
                                    tokio::time::sleep(backoff).await;
                                }
                            }
                        }
                    }

                    let agent_resp = match attempt_resp {
                        Some(resp) => resp,
                        None => {
                            let err = last_err.unwrap_or_else(|| "unknown backend error".into());
                            risks.push(format!(
                                "Backend inference error after {} attempts: {}",
                                MAX_CHAT_ATTEMPTS, err
                            ));
                            break;
                        }
                    };

                    // Count tokens. Prefer the backend's reported usage; fall back
                    // to a character estimate only when the backend doesn't report
                    // it. The old code *always* estimated from assistant text
                    // alone, ignoring prompt and tool-call tokens, so a
                    // tool-call-only turn charged a flat 10 tokens and
                    // `max_tokens` was effectively unreachable.
                    let content_str = agent_resp.content.clone().unwrap_or_default();
                    run.token_count += match agent_resp.usage_tokens {
                        Some(usage) => usage,
                        None => (content_str.len() / 4).max(10) as u32,
                    };

                    if !content_str.is_empty() {
                        messages.push(
                            serde_json::json!({ "role": "assistant", "content": content_str }),
                        );
                    }

                    if agent_resp.tool_calls.is_empty() {
                        break;
                    }

                    let mut should_stop = false;

                    for tool_call in agent_resp.tool_calls {
                        store.emit_event(TaskEvent {
                        task_id: task.id.clone(),
                        ts: Utc::now().timestamp_millis() as u64,
                        kind: "tool".into(),
                        payload: serde_json::json!({ "name": tool_call.name, "args": tool_call.args }),
                    });

                        match tool_call.name.as_str() {
                            "spawn_subagent" | "create_child_task" => {
                                if run.role != "planner" {
                                    risks.push(
                                        "Implementer agent tried to spawn subagent (denied)".into(),
                                    );
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: implementer agents cannot spawn subagents" }));
                                    continue;
                                }

                                let child_goal = tool_call
                                    .args
                                    .get("goal")
                                    .and_then(|g| g.as_str())
                                    .unwrap_or("");
                                let child_criteria = tool_call
                                    .args
                                    .get("acceptance_criteria")
                                    .and_then(|c| c.as_str());

                                if child_goal.is_empty() {
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: goal cannot be empty for child task" }));
                                    continue;
                                }

                                let elapsed_mins = (start_time.elapsed().as_secs() / 60) as u32;
                                let rem_mins =
                                    task.budget.max_minutes.saturating_sub(elapsed_mins).max(1);
                                let rem_steps =
                                    task.budget.max_steps.saturating_sub(run.step_count).max(1);
                                let rem_tokens = task
                                    .budget
                                    .max_tokens
                                    .map(|mt| mt.saturating_sub(run.token_count));

                                let child_budget = TaskBudget {
                                    max_steps: rem_steps,
                                    max_tokens: rem_tokens,
                                    max_minutes: rem_mins,
                                };

                                match store.create_task(
                                    &project.id,
                                    child_goal.to_string(),
                                    child_criteria.map(|s| s.to_string()),
                                    Some(child_budget),
                                    Some(task.id.clone()),
                                ) {
                                    Ok(child) => {
                                        store.emit_event(TaskEvent {
                                        task_id: task.id.clone(),
                                        ts: Utc::now().timestamp_millis() as u64,
                                        kind: "child_created".into(),
                                        payload: serde_json::json!({ "child_id": child.id, "goal": child.goal }),
                                    });

                                        // Auto-spawn child task as implementer run with inherited cancel token
                                        let child_store = Arc::clone(&store);
                                        let child_backend = Arc::clone(&backend);
                                        let child_task = child.clone();
                                        let child_model = run.model.clone();
                                        let child_token = cancel_token.child_token();

                                        tokio::spawn(Box::pin(async move {
                                            let _ = Self::spawn_implementer(
                                                child_store,
                                                child_backend,
                                                child_task,
                                                "implementer".into(),
                                                child_model,
                                                None,
                                                child_token,
                                            )
                                            .await;
                                        }));

                                        messages.push(serde_json::json!({ "role": "user", "content": format!("Successfully created child task #{} ({})", child.id, child.goal) }));
                                    }
                                    Err(e) => {
                                        risks.push(format!("Failed to create child task: {}", e));
                                        messages.push(serde_json::json!({ "role": "user", "content": format!("Failed to create child task: {}", e) }));
                                    }
                                }
                            }
                            "write_file" => {
                                let rel_path = tool_call
                                    .args
                                    .get("path")
                                    .and_then(|p| p.as_str())
                                    .unwrap_or("");
                                let file_content = tool_call
                                    .args
                                    .get("content")
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("");

                                if rel_path.is_empty() {
                                    risks.push("write_file tool called with empty path".into());
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: write_file path is empty" }));
                                    continue;
                                }

                                if !project.allowed_tools.contains(&"write_file".to_string()) {
                                    risks.push(
                                        "write_file tool not allowed by project policy".into(),
                                    );
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: write_file tool is disabled for this project" }));
                                    continue;
                                }

                                match store.write_proposed_file(
                                    &project.root_path,
                                    &task.id,
                                    rel_path,
                                    file_content,
                                ) {
                                    Ok(_) => {
                                        store.emit_event(TaskEvent {
                                        task_id: task.id.clone(),
                                        ts: Utc::now().timestamp_millis() as u64,
                                        kind: "file".into(),
                                        payload: serde_json::json!({ "path": rel_path, "action": "write_proposed" }),
                                    });
                                        messages.push(serde_json::json!({ "role": "user", "content": format!("Successfully wrote proposed file {}", rel_path) }));
                                    }
                                    Err(e) => {
                                        risks.push(format!(
                                            "Failed writing proposed file {}: {}",
                                            rel_path, e
                                        ));
                                        messages.push(serde_json::json!({ "role": "user", "content": format!("Error writing proposed file {}: {}", rel_path, e) }));
                                    }
                                }
                            }
                            "read_file" => {
                                let rel_path = tool_call
                                    .args
                                    .get("path")
                                    .and_then(|p| p.as_str())
                                    .unwrap_or("");
                                let proposed_file = store
                                    .get_task_proposed_dir(&project.root_path, &task.id)
                                    .join(rel_path);
                                let file_text = if proposed_file.exists() {
                                    fs::read_to_string(&proposed_file).unwrap_or_default()
                                } else if let Ok(live_p) =
                                    TaskRuntimeStore::validate_and_resolve_path(
                                        &project.root_path,
                                        rel_path,
                                    )
                                {
                                    fs::read_to_string(&live_p).unwrap_or_default()
                                } else {
                                    "File not found".to_string()
                                };
                                messages.push(serde_json::json!({ "role": "user", "content": format!("File content of {}:\n{}", rel_path, file_text) }));
                            }
                            "run_command" | "exec" | "execute" | "shell" => {
                                let argv: Vec<String> = if let Some(arr) =
                                    tool_call.args.get("argv").and_then(|a| a.as_array())
                                {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                } else if let Some(cmd_str) =
                                    tool_call.args.get("command").and_then(|c| c.as_str())
                                {
                                    cmd_str.split_whitespace().map(|s| s.to_string()).collect()
                                } else {
                                    vec![]
                                };

                                if argv.is_empty() {
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: empty command" }));
                                    continue;
                                }

                                let judge_eval = Judge::evaluate(&argv);
                                store.emit_event(TaskEvent {
                                task_id: task.id.clone(),
                                ts: Utc::now().timestamp_millis() as u64,
                                kind: "judge".into(),
                                payload: serde_json::json!({ "argv": argv, "decision": format!("{:?}", judge_eval) }),
                            });

                                match judge_eval {
                                    JudgeResult::Allow => {
                                        if !project
                                            .allowed_tools
                                            .contains(&"run_command".to_string())
                                            && !project
                                                .allowed_tools
                                                .contains(&"execute".to_string())
                                        {
                                            risks.push(
                                                "Command execution tool not allowed by project policy"
                                                    .into(),
                                            );
                                            messages.push(serde_json::json!({ "role": "user", "content": "Error: command execution is disabled for this project" }));
                                            continue;
                                        }

                                        let output_res = tokio::process::Command::new(&argv[0])
                                            .args(&argv[1..])
                                            .current_dir(&project.root_path)
                                            .output()
                                            .await;

                                        match output_res {
                                            Ok(output) => {
                                                let exit_code = output.status.code().unwrap_or(-1);
                                                let stdout =
                                                    String::from_utf8_lossy(&output.stdout);
                                                let stderr =
                                                    String::from_utf8_lossy(&output.stderr);
                                                let excerpt = format!(
                                                    "STDOUT:\n{}\nSTDERR:\n{}",
                                                    stdout, stderr
                                                );
                                                let trimmed_excerpt = if excerpt.len() > 1000 {
                                                    format!("{}...\n(truncated)", &excerpt[..1000])
                                                } else {
                                                    excerpt
                                                };

                                                commands_executed.push(ReviewCommand {
                                                    argv: argv.clone(),
                                                    judge: JudgeDecision::Allow,
                                                    exit: exit_code,
                                                    excerpt: trimmed_excerpt.clone(),
                                                });

                                                if exit_code == 0 {
                                                    checks.push(format!(
                                                        "Passed: {}",
                                                        argv.join(" ")
                                                    ));
                                                } else {
                                                    risks.push(format!(
                                                        "Command failed (exit {}): {}",
                                                        exit_code,
                                                        argv.join(" ")
                                                    ));
                                                }

                                                messages.push(serde_json::json!({
                                                "role": "user",
                                                "content": format!("Command '{}' exited with {}:\n{}", argv.join(" "), exit_code, trimmed_excerpt)
                                            }));
                                            }
                                            Err(e) => {
                                                commands_executed.push(ReviewCommand {
                                                    argv: argv.clone(),
                                                    judge: JudgeDecision::Allow,
                                                    exit: -1,
                                                    excerpt: format!(
                                                        "Failed to spawn command: {}",
                                                        e
                                                    ),
                                                });
                                                risks.push(format!(
                                                    "Failed to execute command {}: {}",
                                                    argv.join(" "),
                                                    e
                                                ));
                                                messages.push(serde_json::json!({ "role": "user", "content": format!("Failed to execute command: {}", e) }));
                                            }
                                        }
                                    }
                                    JudgeResult::Deny => {
                                        commands_executed.push(ReviewCommand {
                                            argv: argv.clone(),
                                            judge: JudgeDecision::Deny,
                                            exit: -1,
                                            excerpt: "Command denied by Judge security policy"
                                                .into(),
                                        });
                                        risks.push(format!(
                                            "Denied unsafe command execution: {}",
                                            argv.join(" ")
                                        ));
                                        messages.push(serde_json::json!({ "role": "user", "content": "Command denied by Judge security policy" }));
                                    }
                                    JudgeResult::Pause => {
                                        final_status = TaskStatus::Blocked;
                                        commands_executed.push(ReviewCommand {
                                            argv: argv.clone(),
                                            judge: JudgeDecision::Pause,
                                            exit: -1,
                                            excerpt: "Command execution paused pending approval"
                                                .into(),
                                        });
                                        risks.push("Command requires human approval".into());
                                        messages.push(serde_json::json!({ "role": "user", "content": "Command paused pending human approval" }));
                                        should_stop = true;
                                    }
                                }
                            }
                            // MCP tools are named `mcp_{server}::{tool}`. Dispatch
                            // is gated twice: the project must list `mcp_read`,
                            // and `execute_mcp_tool` re-checks the capability
                            // class, so a model that hallucinates a Write-class
                            // name gets a refusal rather than an execution.
                            name if name.starts_with("mcp_") => {
                                if !project
                                    .effective_allowed_tools()
                                    .contains(&"mcp_read".to_string())
                                {
                                    risks.push(format!(
                                    "MCP tool '{}' called but mcp_read is not enabled for this project",
                                    name
                                ));
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: MCP tools are disabled for this project" }));
                                    continue;
                                }
                                let Some((server, tool)) =
                                    crate::mcp_bridge::parse_mcp_tool_name(name)
                                else {
                                    messages.push(serde_json::json!({ "role": "user", "content": format!("Error: malformed MCP tool name '{}'", name) }));
                                    continue;
                                };
                                let Some(registry) = store.mcp_registry() else {
                                    // Record it as a risk, not just a message: the
                                    // message goes back to the model, but the
                                    // ReviewPacket is what a human reads, and a
                                    // tool that could not run must be visible
                                    // there — otherwise the packet claims the
                                    // agent searched when it could not.
                                    risks.push(format!(
                                    "MCP tool '{}' could not run: no MCP registry is configured on this server",
                                    name
                                ));
                                    messages.push(serde_json::json!({ "role": "user", "content": "Error: no MCP registry is configured on this server" }));
                                    continue;
                                };
                                let result = crate::mcp_bridge::execute_mcp_tool(
                                    registry,
                                    &server,
                                    &tool,
                                    &tool_call.args,
                                    &task.id,
                                    std::path::Path::new(&project.root_path),
                                )
                                .await;
                                match result {
                                    Ok(text) => {
                                        checks.push(format!(
                                            "MCP {}.{} returned output",
                                            server, tool
                                        ));
                                        messages.push(serde_json::json!({
                                            "role": "user",
                                            "content": format!("MCP tool {}.{} result:
                                        {}", server, tool, text)
                                        }));
                                    }
                                    Err(e) => {
                                        risks.push(format!(
                                            "MCP tool {}.{} refused: {}",
                                            server, tool, e
                                        ));
                                        messages.push(serde_json::json!({
                                        "role": "user",
                                        "content": format!("Error: MCP tool {}.{} failed: {}", server, tool, e)
                                    }));
                                    }
                                }
                            }
                            _ => {
                                messages.push(serde_json::json!({ "role": "user", "content": format!("Unknown or unhandled tool: {}", tool_call.name) }));
                            }
                        }

                        if should_stop {
                            break;
                        }
                    }

                    if should_stop {
                        break;
                    }
                }

                if run.step_count >= max_steps && final_status != TaskStatus::Blocked {
                    risks.push("Step budget exhausted (max_steps)".into());
                }

                if context_dropped_total > 0 {
                    // Surface it rather than letting compaction happen silently:
                    // a reviewer should know the agent was working from a
                    // truncated transcript, not the whole history.
                    risks.push(format!(
                        "Context compacted: {} older message(s) dropped to fit the \
                     {}-token budget (system prompt and task goal pinned)",
                        context_dropped_total, governor.token_budget
                    ));
                }

                if cancel_token.is_cancelled() {
                    return Err(anyhow!("Task execution cancelled by user"));
                }

                // Generate ReviewPacket from proposed/ tree vs live root
                let proposed_dir = store.get_task_proposed_dir(&project.root_path, &task.id);
                let mut rel_proposed_files = Vec::new();
                let _ = list_dir_relative(&proposed_dir, &proposed_dir, &mut rel_proposed_files);

                let mut diffs = Vec::new();
                for rel_path in rel_proposed_files {
                    let proposed_file_path = proposed_dir.join(&rel_path);
                    let proposed_content =
                        fs::read_to_string(&proposed_file_path).unwrap_or_default();

                    let original_content = if let Ok(live_path) =
                        TaskRuntimeStore::validate_and_resolve_path(&project.root_path, &rel_path)
                    {
                        fs::read_to_string(live_path).unwrap_or_default()
                    } else {
                        "".to_string()
                    };

                    let unified_diff =
                        unified_diff(&rel_path, &original_content, &proposed_content);

                    diffs.push(ReviewDiff {
                        path: rel_path,
                        unified_diff,
                        original: original_content,
                        proposed: proposed_content,
                    });
                }

                // ---- Verification ------------------------------------------------
                //
                // Run the project's own definition of done against the *merged*
                // result, not the staged tree in isolation: a change is only good
                // if it works alongside the code it is being merged into. The
                // proposed files are applied to a scratch copy of the project so
                // the live tree is never touched.
                let plan = VerificationPlan::detect(&project.root_path);
                let mut verification: Vec<VerificationResult> = Vec::new();

                match &plan {
                    None => {
                        risks.push(
                            "No verification plan detected (no Cargo.toml / package.json \
                         test script / pytest config) — this change is UNVERIFIED"
                                .into(),
                        );
                    }
                    Some(plan) => {
                        let disallowed = plan.disallowed_commands();
                        if !disallowed.is_empty() {
                            // A plan that asks for something the Judge refuses is a
                            // configuration error, not a reason to run it anyway.
                            for argv in disallowed {
                                risks.push(format!(
                                    "Verification plan command refused by Judge policy: {}",
                                    argv.join(" ")
                                ));
                            }
                        } else if !diffs.is_empty() {
                            store.emit_event(TaskEvent {
                                task_id: task.id.clone(),
                                ts: Utc::now().timestamp_millis() as u64,
                                kind: "verification_started".into(),
                                payload: serde_json::json!({
                                    "commands": plan.commands,
                                }),
                            });

                            match Self::verify_on_scratch_copy(
                                &store,
                                &project.root_path,
                                &task.id,
                                plan,
                            )
                            .await
                            {
                                Ok(results) => {
                                    for r in &results {
                                        if r.passed {
                                            checks.push(r.summary());
                                        } else {
                                            risks.push(r.summary());
                                        }
                                    }
                                    verification = results;
                                }
                                Err(e) => risks.push(format!(
                                    "Verification could not run: {e} — change is UNVERIFIED"
                                )),
                            }
                        }

                        // Be explicit about the unverified case rather than letting
                        // an empty `checks` read as success.
                        //
                        // Two different situations produce an empty `verification`, and
                        // conflating them is misleading in both directions. When the
                        // implementer wrote no files there is nothing to verify -- the
                        // run did not fail, it simply has nothing to show, and saying
                        // verification "produced no results" implies a check ran and
                        // came back empty. Observed live: 23 reviews carried that exact
                        // risk when the real cause was a backend inference error three
                        // steps earlier, so the message pointed at the wrong thing
                        // entirely.
                        if verification.is_empty() && !plan.commands.is_empty() {
                            if diffs.is_empty() {
                                risks.push(
                                "Verification skipped: the implementer proposed no file                                  changes, so there was nothing to verify"
                                    .into(),
                            );
                            } else {
                                risks.push(
                                    "Verification produced no results — change is UNVERIFIED"
                                        .into(),
                                );
                            }
                        }
                    }
                }

                // ---- Auto-fix -------------------------------------------
                //
                // Verification ran and something failed. Give the model one
                // more chance *before* the packet is produced, so the human
                // sees a repaired proposal rather than a known-broken one.
                // Bounded by MAX_AUTO_FIX_ATTEMPTS and by the remaining step
                // budget - the `while` above re-enters with `run.step_count`
                // already advanced, so this cannot loop.
                //
                // The Judge still gates every command the retry issues, and
                // the staging tree is still the only thing it can write to.
                //
                // `plan` is only a signal here — its `commands` were already run by
                // the block above — so the binding is intentionally unused; what
                // matters is that a plan existed at all.
                if plan.is_some() {
                    let any_failed = verification.iter().any(|r| !r.passed);
                    let has_changes = !diffs.is_empty();
                    if any_failed
                        && has_changes
                        && auto_fix_attempts < MAX_AUTO_FIX_ATTEMPTS
                        && run.step_count < max_steps
                        && !cancel_token.is_cancelled()
                    {
                        auto_fix_attempts += 1;
                        let failure_summary: String = verification
                            .iter()
                            .filter(|r| !r.passed)
                            .map(|r| r.summary())
                            .collect::<Vec<_>>()
                            .join(
                                "
",
                            );
                        store.emit_event(TaskEvent {
                            task_id: task.id.clone(),
                            ts: Utc::now().timestamp_millis() as u64,
                            kind: "auto_fix_triggered".into(),
                            payload: serde_json::json!({
                                "attempt": auto_fix_attempts,
                                "max_attempts": MAX_AUTO_FIX_ATTEMPTS,
                                "failures": failure_summary,
                                "steps_used": run.step_count,
                                "max_steps": max_steps,
                            }),
                        });
                        messages.push(serde_json::json!({
                            "role": "user",
                            "content": format!(
                                "Verification failed:
{}

Fix the proposed files so the verification \
                                 passes. Update the files in the proposed/ directory. This is your \
                                 last automatic attempt ({} of {}); after it the change goes to \
                                 human review as-is.",
                                failure_summary, auto_fix_attempts, MAX_AUTO_FIX_ATTEMPTS
                            ),
                        }));
                        // Clear the failed results: the retry's own
                        // verification replaces them, and a stale failure must
                        // not survive into the packet if the retry passes.
                        verification.clear();
                        continue 'agent_loop;
                    }
                }

                let accelerator_usage = placement.as_ref().map(|p| AcceleratorUsage {
                    node_id: p.assigned_node.clone(),
                    compute_capability: p.node_capability.compute_capability.clone(),
                    gpu_name: p.node_capability.gpu_name.clone(),
                    vram_free_gb: p.node_capability.available_vram_gb,
                    commands_executed: commands_executed.len() as u32,
                    tokens_generated: run.token_count,
                });
                let context_summary = if context_dropped_total > 0 {
                    Some(ContextSummary {
                        original_turn_count: context_dropped_total,
                        summary_text: format!(
                            "{} older message(s) were dropped to fit the {}-token context budget. \
                         The system prompt and the original task goal were pinned; the most \
                         recent turns were kept in full.",
                            context_dropped_total, governor.token_budget
                        ),
                        key_decisions: Vec::new(),
                        files_modified: files_touched_in_compacted_turns(&messages),
                    })
                } else {
                    None
                };
                let review = ReviewPacket {
                    id: format!("rev_{}", Uuid::new_v4().simple()),
                    task_id: task.id.clone(),
                    run_id: run.id.clone(),
                    summary: format!("Proposed changes for task: {}", task.goal),
                    diffs,
                    commands: commands_executed,
                    checks,
                    risks,
                    verification,
                    placement,
                    accelerator_usage,
                    context_summary,
                    created_at: Utc::now().to_rfc3339(),
                };

                // Close the outer agent loop, carrying the finished packet out.
                // Reached only when the auto-fix path did not re-enter it.
                break 'agent_loop (review, final_status);
            };

            Ok((review, final_status))
        })
    }

    /// Apply the task's staged files to a scratch copy of the project, run the
    /// verification plan there, and clean up. The live tree is never modified.
    async fn verify_on_scratch_copy(
        store: &Arc<TaskRuntimeStore>,
        project_root: &str,
        task_id: &str,
        plan: &VerificationPlan,
    ) -> Result<Vec<VerificationResult>> {
        let scratch = std::env::temp_dir().join(format!(
            "ghostlink_verify_{}_{}",
            task_id,
            Uuid::new_v4().simple()
        ));
        let _ = fs::remove_dir_all(&scratch);

        copy_dir_recursive(Path::new(project_root), &scratch)
            .with_context(|| "failed to copy project to scratch dir for verification")?;

        // Overlay the staged files onto the copy.
        let proposed_dir = store.get_task_proposed_dir(project_root, task_id);
        if proposed_dir.exists() {
            copy_dir_recursive(&proposed_dir, &scratch)
                .with_context(|| "failed to apply staged files to scratch copy")?;
        }

        let results = run_verification_plan(
            plan,
            &scratch.to_string_lossy(),
            std::time::Duration::from_secs(600),
        )
        .await;

        let _ = fs::remove_dir_all(&scratch);
        Ok(results)
    }
}

/// Every file path named in a `write_file` tool call across the transcript.
///
/// Used to populate `ContextSummary::files_modified` when compaction drops
/// turns: the reviewer needs to know what the agent touched even if the
/// turns describing it are gone. Paths come from the tool-call arguments,
/// not from inference, so this cannot name a file the agent never wrote.
fn files_touched_in_compacted_turns(messages: &[serde_json::Value]) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for msg in messages {
        if let Some(calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
            for call in calls {
                if call.get("name").and_then(|n| n.as_str()) == Some("write_file") {
                    if let Some(path) = call
                        .get("args")
                        .and_then(|a| a.get("path"))
                        .and_then(|p| p.as_str())
                    {
                        if !files.iter().any(|f| f == path) {
                            files.push(path.to_string());
                        }
                    }
                }
            }
        }
    }
    files
}

/// Recursively copy `src` into `dst`, skipping the task staging tree and VCS
/// metadata (copying `.git` would be both slow and wrong for a scratch build).
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    if !src.is_dir() {
        return Ok(());
    }
    // Refuse a destination inside the source. `read_dir(src)` would then yield the
    // destination itself and recurse into it indefinitely -- an unbounded copy and a
    // stack overflow, rather than a clean error. The scratch dir is created
    // separately today, so this never fires in production; it is here because the
    // failure mode is silent and total.
    if dst.starts_with(src) {
        anyhow::bail!(
            "verification copy destination {} is inside the source {}",
            dst.display(),
            src.display()
        );
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip anything that is not source. This list is not cosmetic: the copy
        // exists so `cargo test` can build in isolation, and on a real project the
        // heavyweight directories dominate it. Against this repository the copy was
        // 40 GB -- 38.8 GB of it `models/*.gguf` -- which made verification
        // impossible rather than slow, since the copy alone outran the timeout and
        // filled the disk.
        //
        // `models` holds GGUF weights that no build step reads. `third_party` holds
        // a vendored llama.cpp whose own `target/` is nested and whose sources are
        // gigabytes of unrelated build output. Neither participates in the host
        // workspace build, so copying them buys nothing.
        if matches!(
            name_str.as_ref(),
            ".git" | "target" | "node_modules" | "models" | "third_party" | "dist" | "build"
        ) {
            continue;
        }
        // Don't copy the staging tree into its own verification copy.
        if name_str == ".ghostlink" {
            continue;
        }
        // Large generated artefacts that are not source either: a RAG index can be
        // hundreds of megabytes of embeddings and is regenerated by `/index`.
        if entry
            .metadata()
            .map(|m| m.len() > 64 * 1024 * 1024)
            .unwrap_or(false)
            && !entry.path().is_dir()
        {
            tracing::debug!(
                target: "assistant_trace",
                path = %entry.path().display(),
                "skipping large non-source file in verification copy"
            );
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
pub struct FakeBackend {
    pub responses: Arc<Mutex<Vec<AgentResponse>>>,
}

#[cfg(test)]
#[async_trait::async_trait]
impl AgentBackend for FakeBackend {
    async fn chat(
        &self,
        _messages: &[serde_json::Value],
        _allowed_tools: &[String],
    ) -> Result<AgentResponse> {
        let mut guard = self.responses.lock().await;
        if guard.is_empty() {
            Ok(AgentResponse {
                content: Some("Finished".into()),
                tool_calls: vec![],
                usage_tokens: None,
            })
        } else {
            Ok(guard.remove(0))
        }
    }
}

#[cfg(test)]
mod slice1_tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_backend_write_file_and_acceptance() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_slice1_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_slice1_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();
        let root_str = proj_dir.to_string_lossy().to_string();

        let proj = store
            .create_project(
                "Slice1 Proj".into(),
                ProjectKind::Code,
                root_str.clone(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "Create output file".into(), None, None, None)
            .unwrap();

        let fake_backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![
                AgentResponse {
                    content: Some("Writing file".into()),
                    tool_calls: vec![ToolCall {
                        id: "tc_1".into(),
                        name: "write_file".into(),
                        args: serde_json::json!({
                            "path": "test_output.txt",
                            "content": "Hello Slice 1"
                        }),
                    }],
                    usage_tokens: None,
                },
                AgentResponse {
                    content: Some("All done".into()),
                    tool_calls: vec![],
                    usage_tokens: None,
                },
            ])),
        });

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let mut run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role: "implementer".into(),
            model: "mock-model".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        };

        let (review, final_status) = TaskRunner::run_loop(
            store.clone(),
            fake_backend,
            task.clone(),
            &mut run,
            None,
            cancel_token,
        )
        .await
        .unwrap();

        assert_eq!(final_status, TaskStatus::NeedsReview);
        assert_eq!(review.diffs.len(), 1);
        assert_eq!(review.diffs[0].path, "test_output.txt");

        let proposed_file = store
            .get_task_proposed_dir(&root_str, &task.id)
            .join("test_output.txt");
        let live_file = proj_dir.join("test_output.txt");

        assert!(proposed_file.exists());
        assert!(!live_file.exists());

        store.apply_proposed_changes(&root_str, &task.id).unwrap();
        assert!(live_file.exists());
        assert_eq!(fs::read_to_string(live_file).unwrap(), "Hello Slice 1");

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    #[tokio::test]
    async fn test_judge_deny_tool_is_not_executed() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_deny_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir = std::env::temp_dir().join(format!("ghostlink_deny_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Deny Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "Try dangerous command".into(), None, None, None)
            .unwrap();

        let fake_backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("Exec rm".into()),
                tool_calls: vec![ToolCall {
                    id: "tc_deny".into(),
                    name: "run_command".into(),
                    args: serde_json::json!({
                        "argv": ["rm", "-rf", "/"]
                    }),
                }],
                usage_tokens: None,
            }])),
        });

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let mut run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role: "implementer".into(),
            model: "mock-model".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        };

        let (review, _status) = TaskRunner::run_loop(
            store.clone(),
            fake_backend,
            task.clone(),
            &mut run,
            None,
            cancel_token,
        )
        .await
        .unwrap();

        assert_eq!(review.commands.len(), 1);
        assert_eq!(review.commands[0].judge, JudgeDecision::Deny);
        assert!(review
            .risks
            .iter()
            .any(|r| r.contains("Denied unsafe command")));

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    #[tokio::test]
    async fn test_judge_pause_sets_blocked() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_pause_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_pause_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Pause Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "Try unknown command".into(), None, None, None)
            .unwrap();

        let fake_backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("Exec custom".into()),
                tool_calls: vec![ToolCall {
                    id: "tc_pause".into(),
                    name: "run_command".into(),
                    args: serde_json::json!({
                        "argv": ["custom_unknown_binary", "--arg"]
                    }),
                }],
                usage_tokens: None,
            }])),
        });

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let mut run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role: "implementer".into(),
            model: "mock-model".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        };

        let (review, final_status) = TaskRunner::run_loop(
            store.clone(),
            fake_backend,
            task.clone(),
            &mut run,
            None,
            cancel_token,
        )
        .await
        .unwrap();

        assert_eq!(final_status, TaskStatus::Blocked);
        assert_eq!(review.commands.len(), 1);
        assert_eq!(review.commands[0].judge, JudgeDecision::Pause);

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    #[tokio::test]
    async fn test_cancel_mid_loop_sets_cancelled() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_cancel_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_cancel_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Cancel Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "Cancel task".into(), None, None, None)
            .unwrap();

        let fake_backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("Step 1".into()),
                tool_calls: vec![],
                usage_tokens: None,
            }])),
        });

        let cancel_token = tokio_util::sync::CancellationToken::new();
        cancel_token.cancel();

        let mut run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role: "implementer".into(),
            model: "mock-model".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        };

        let res = TaskRunner::run_loop(
            store.clone(),
            fake_backend,
            task.clone(),
            &mut run,
            None,
            cancel_token,
        )
        .await;

        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("cancelled"));

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    #[tokio::test]
    async fn test_budget_max_steps_stops_and_emits_review() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_budget_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_budget_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Budget Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let budget = TaskBudget {
            max_steps: 1,
            ..Default::default()
        };

        let task = store
            .create_task(&proj.id, "Budget task".into(), None, Some(budget), None)
            .unwrap();

        let fake_backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![
                AgentResponse {
                    content: Some("Step 1".into()),
                    tool_calls: vec![ToolCall {
                        id: "tc_1".into(),
                        name: "read_file".into(),
                        args: serde_json::json!({ "path": "nonexistent.txt" }),
                    }],
                    usage_tokens: None,
                },
                AgentResponse {
                    content: Some("Step 2 - should not run".into()),
                    tool_calls: vec![],
                    usage_tokens: None,
                },
            ])),
        });

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let mut run = AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task.id.clone(),
            role: "implementer".into(),
            model: "mock-model".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        };

        let (review, _status) = TaskRunner::run_loop(
            store.clone(),
            fake_backend,
            task.clone(),
            &mut run,
            None,
            cancel_token,
        )
        .await
        .unwrap();

        assert_eq!(run.step_count, 1);
        assert!(review
            .risks
            .iter()
            .any(|r| r.contains("Step budget exhausted")));

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&proj_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_roundtrip_and_atomic_write() {
        let temp_dir =
            std::env::temp_dir().join(format!("ghostlink_test_{}", uuid::Uuid::new_v4()));
        let store = TaskRuntimeStore::new(&temp_dir).unwrap();

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_proj_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Test Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();

        assert_eq!(proj.name, "Test Proj");
        let fetched_proj = store.get_project(&proj.id).unwrap();
        assert_eq!(fetched_proj.id, proj.id);

        let task = store
            .create_task(&proj.id, "Build feature X".into(), None, None, None)
            .unwrap();
        assert_eq!(task.goal, "Build feature X");
        assert_eq!(task.status, TaskStatus::Queued);

        let fetched_task = store.get_task(&task.id).unwrap();
        assert_eq!(fetched_task.id, task.id);

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&proj_dir);
    }

    #[test]
    fn test_path_traversal_rejection() {
        let root_dir =
            std::env::temp_dir().join(format!("ghostlink_root_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_dir).unwrap();
        let root = root_dir.to_string_lossy().to_string();

        let valid = TaskRuntimeStore::validate_and_resolve_path(&root, "src/main.rs");
        assert!(valid.is_ok());

        let invalid = TaskRuntimeStore::validate_and_resolve_path(&root, "../../../etc/passwd");
        assert!(invalid.is_err());

        let _ = std::fs::remove_dir_all(&root_dir);
    }

    #[test]
    fn test_judge_policy() {
        assert_eq!(
            Judge::evaluate(&["ls".into(), "-la".into()]),
            JudgeResult::Allow
        );
        assert_eq!(
            Judge::evaluate(&["cargo".into(), "test".into()]),
            JudgeResult::Allow
        );
        assert_eq!(
            Judge::evaluate(&["git".into(), "status".into()]),
            JudgeResult::Allow
        );

        // Read-only git subcommands an agent legitimately needs: `show` for a
        // commit, `branch` for where it is, `stash` to park work-in-progress.
        // Each is non-destructive, so each is Allow rather than Pause.
        for sub in ["status", "diff", "log", "show", "branch", "stash"] {
            assert_eq!(
                Judge::evaluate(&["git".into(), sub.into()]),
                JudgeResult::Allow,
                "git {sub} must be allowed"
            );
        }

        // Destructive git operations stay refused regardless of subcommand.
        for argv in [
            vec!["git".to_string(), "reset".to_string(), "--hard".to_string()],
            vec!["git".to_string(), "push".to_string(), "--force".to_string()],
            vec!["git".to_string(), "push".to_string(), "-f".to_string()],
        ] {
            assert_eq!(
                Judge::evaluate(&argv),
                JudgeResult::Deny,
                "{argv:?} must be denied"
            );
        }

        // A bare `git` with no subcommand is not a read: it falls through
        // `is_allowed`'s `argv.len() >= 2` guard and lands on the default
        // Pause (unrecognized), which still requires human approval — so it
        // does not execute unattended either way.
        assert_eq!(Judge::evaluate(&["git".into()]), JudgeResult::Pause);

        assert_eq!(
            Judge::evaluate(&["rm".into(), "-rf".into(), "/".into()]),
            JudgeResult::Deny
        );
        assert_eq!(
            Judge::evaluate(&["curl".into(), "http://example.com".into()]),
            JudgeResult::Deny
        );
        assert_eq!(
            Judge::evaluate(&["sudo".into(), "reboot".into()]),
            JudgeResult::Deny
        );

        assert_eq!(
            Judge::evaluate(&["custom_tool".into(), "arg".into()]),
            JudgeResult::Pause
        );
    }

    #[test]
    fn test_accept_reject_and_request_changes() {
        let temp_dir =
            std::env::temp_dir().join(format!("ghostlink_test_{}", uuid::Uuid::new_v4()));
        let store = TaskRuntimeStore::new(&temp_dir).unwrap();

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_proj_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&proj_dir).unwrap();
        let root_str = proj_dir.to_string_lossy().to_string();

        let proj = store
            .create_project(
                "Sample".into(),
                ProjectKind::Code,
                root_str.clone(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "Add file".into(), None, None, None)
            .unwrap();

        // Write proposed file
        store
            .write_proposed_file(&root_str, &task.id, "hello.txt", "Hello World")
            .unwrap();

        let live_file = proj_dir.join("hello.txt");
        assert!(!live_file.exists());

        // Reject test
        store.discard_proposed_changes(&root_str, &task.id).unwrap();
        let proposed_dir = store.get_task_proposed_dir(&root_str, &task.id);
        assert!(!proposed_dir.exists());

        // Re-write proposed file & Accept test
        store
            .write_proposed_file(&root_str, &task.id, "hello.txt", "Hello World")
            .unwrap();
        let applied = store.apply_proposed_changes(&root_str, &task.id).unwrap();
        assert_eq!(applied, vec!["hello.txt"]);
        assert!(live_file.exists());
        assert_eq!(std::fs::read_to_string(&live_file).unwrap(), "Hello World");

        // Request changes test
        let requeued = store.requeue_task(&task.id).unwrap();
        assert_eq!(requeued.status, TaskStatus::Queued);

        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&proj_dir);
    }
}

#[cfg(test)]
mod fanout_tests {
    use super::*;

    #[test]
    fn test_fanout_depth_and_child_caps() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_fanout_{}", Uuid::new_v4()));
        let store = TaskRuntimeStore::new(&temp_dir).unwrap();

        let proj_dir =
            std::env::temp_dir().join(format!("ghostlink_fanout_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Fanout Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();

        let parent = store
            .create_task(&proj.id, "Parent Goal".into(), None, None, None)
            .unwrap();

        // Create 4 children (max_children=4)
        for i in 0..4 {
            let child = store
                .create_task(
                    &proj.id,
                    format!("Child {}", i),
                    None,
                    None,
                    Some(parent.id.clone()),
                )
                .unwrap();
            assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
        }

        // 5th child fails
        let child5_err = store.create_task(
            &proj.id,
            "Child 5".into(),
            None,
            None,
            Some(parent.id.clone()),
        );
        assert!(child5_err.is_err());
        assert!(child5_err
            .unwrap_err()
            .to_string()
            .contains("max_children=4"));

        // Sub-child fails (max_depth=1)
        let child1 = store
            .list_project_tasks(&proj.id)
            .unwrap()
            .into_iter()
            .find(|t| t.parent_id == Some(parent.id.clone()))
            .unwrap();
        let subchild_err = store.create_task(
            &proj.id,
            "Sub-child".into(),
            None,
            None,
            Some(child1.id.clone()),
        );
        assert!(subchild_err.is_err());
        assert!(subchild_err
            .unwrap_err()
            .to_string()
            .contains("max_depth=1"));

        // Parent accept blocked
        let check_accept = store.check_parent_accept_allowed(&proj.id, &parent.id);
        assert!(check_accept.is_err());

        // Cancel cascade
        let _ = store.cancel_task_run(&parent.id);
        let check_accept_after_cancel = store.check_parent_accept_allowed(&proj.id, &parent.id);
        assert!(check_accept_after_cancel.is_ok());

        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_dir_all(&proj_dir);
    }
}

#[cfg(test)]
mod parallel_and_compact_tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_isolated_tools_execution() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_par_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir = std::env::temp_dir().join(format!("ghostlink_par_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();
        fs::write(proj_dir.join("file1.txt"), "content 1").unwrap();
        fs::write(proj_dir.join("file2.txt"), "content 2").unwrap();

        let proj = store
            .create_project(
                "Parallel Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();

        let task = store
            .create_task(
                &proj.id,
                "Read two files in parallel".into(),
                None,
                None,
                None,
            )
            .unwrap();

        let responses = Arc::new(Mutex::new(vec![
            AgentResponse {
                content: Some("Reading files...".into()),
                tool_calls: vec![
                    ToolCall {
                        id: "call_1".into(),
                        name: "read_file".into(),
                        args: serde_json::json!({ "path": "file1.txt" }),
                    },
                    ToolCall {
                        id: "call_2".into(),
                        name: "read_file".into(),
                        args: serde_json::json!({ "path": "file2.txt" }),
                    },
                ],
                usage_tokens: None,
            },
            AgentResponse {
                content: Some("Done reading".into()),
                tool_calls: vec![],
                usage_tokens: None,
            },
        ]));

        let backend = Arc::new(FakeBackend { responses });
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let run = TaskRunner::spawn_implementer_joined(
            store.clone(),
            backend,
            task.clone(),
            "implementer".into(),
            "test-model".into(),
            None,
            cancel_token,
        )
        .await
        .unwrap();

        assert_eq!(run.status, "finished");
        let review = store.get_task_review(&task.id).unwrap();
        assert_eq!(review.task_id, task.id);
    }

    #[tokio::test]
    async fn test_parent_review_waits_for_child_completion() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_wait_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir = std::env::temp_dir().join(format!("ghostlink_wait_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();

        let proj = store
            .create_project(
                "Wait Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();

        let parent_task = store
            .create_task(&proj.id, "Parent task goal".into(), None, None, None)
            .unwrap();

        let responses = Arc::new(Mutex::new(vec![
            AgentResponse {
                content: Some("Spawning child...".into()),
                tool_calls: vec![ToolCall {
                    id: "call_spawn".into(),
                    name: "spawn_subagent".into(),
                    args: serde_json::json!({ "goal": "Child goal" }),
                }],
                usage_tokens: None,
            },
            AgentResponse {
                content: Some("Parent finished".into()),
                tool_calls: vec![],
                usage_tokens: None,
            },
            AgentResponse {
                content: Some("Child finished".into()),
                tool_calls: vec![],
                usage_tokens: None,
            },
        ]));

        let backend = Arc::new(FakeBackend { responses });
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let run = TaskRunner::spawn_implementer_joined(
            store.clone(),
            backend,
            parent_task.clone(),
            "planner".into(),
            "test-model".into(),
            None,
            cancel_token,
        )
        .await
        .unwrap();

        assert_eq!(run.status, "finished");
        let children = store.list_child_tasks(&parent_task.id).unwrap();
        assert_eq!(children.len(), 1);
    }

    #[tokio::test]
    async fn test_compact_review_diff_generation() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_diff_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir = std::env::temp_dir().join(format!("ghostlink_diff_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();
        fs::write(proj_dir.join("code.rs"), "fn hello() {}\n").unwrap();

        let proj = store
            .create_project(
                "Diff Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();

        let task = store
            .create_task(&proj.id, "Update code".into(), None, None, None)
            .unwrap();

        store
            .write_proposed_file(
                &proj.root_path,
                &task.id,
                "code.rs",
                "fn hello() {\n    println!(\"world\");\n}\n",
            )
            .unwrap();

        let responses = Arc::new(Mutex::new(vec![AgentResponse {
            content: Some("Done".into()),
            tool_calls: vec![],
            usage_tokens: None,
        }]));

        let backend = Arc::new(FakeBackend { responses });
        let cancel_token = tokio_util::sync::CancellationToken::new();

        let _run = TaskRunner::spawn_implementer_joined(
            store.clone(),
            backend,
            task.clone(),
            "implementer".into(),
            "test-model".into(),
            None,
            cancel_token,
        )
        .await
        .unwrap();

        let review = store.get_task_review(&task.id).unwrap();
        assert_eq!(review.diffs.len(), 1);
        assert!(review.diffs[0].unified_diff.contains("--- a/code.rs"));
        assert!(review.diffs[0].unified_diff.contains("+++ b/code.rs"));
        // The old implementation emitted a hardcoded `@@ -0,0 +1,3 @@`
        // regardless of content; assert real line numbers now.
        assert!(
            review.diffs[0].unified_diff.contains("@@ -1,1 +1,3 @@"),
            "expected a real hunk header, got:\n{}",
            review.diffs[0].unified_diff
        );
    }

    #[test]
    fn unified_diff_reports_real_hunk_headers() {
        let d = unified_diff(
            "a.rs",
            "fn hello() {}\n",
            "fn hello() {\n    println!(\"world\");\n}\n",
        );
        assert!(d.contains("@@ -1,1 +1,3 @@"), "{d}");
        assert!(d.contains("-fn hello() {}"), "{d}");
        assert!(d.contains("+    println!(\"world\");"), "{d}");
    }

    #[test]
    fn unified_diff_is_empty_when_unchanged() {
        assert_eq!(unified_diff("a.rs", "same\n", "same\n"), "");
    }

    #[test]
    fn unified_diff_handles_pure_insert_and_delete() {
        let added = unified_diff("a.rs", "", "new\n");
        assert!(added.contains("+new"), "{added}");

        let removed = unified_diff("a.rs", "gone\n", "");
        assert!(removed.contains("-gone"), "{removed}");
    }

    #[test]
    fn unified_diff_line_numbers_are_correct_mid_file() {
        let original: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        let mut proposed: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        proposed = proposed.replace("line7\n", "line7-changed\n");
        let d = unified_diff("f.rs", &original, &proposed);
        // Change is on line 7, so the hunk starts 3 lines earlier at line 4.
        assert!(d.contains("@@ -4,7 +4,7 @@"), "{d}");
    }

    #[tokio::test]
    async fn rollback_restores_overwritten_file_and_deletes_created_one() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_rb_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());

        let proj_dir = std::env::temp_dir().join(format!("ghostlink_rb_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();
        fs::write(proj_dir.join("existing.rs"), "ORIGINAL\n").unwrap();

        let proj = store
            .create_project(
                "RB Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "Mutate".into(), None, None, None)
            .unwrap();

        // Overwrite one existing file and create one brand-new file.
        store
            .write_proposed_file(&proj.root_path, &task.id, "existing.rs", "CLOBBERED\n")
            .unwrap();
        store
            .write_proposed_file(&proj.root_path, &task.id, "brand_new.rs", "NEW\n")
            .unwrap();

        let applied = store
            .apply_proposed_changes(&proj.root_path, &task.id)
            .unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(
            fs::read_to_string(proj_dir.join("existing.rs")).unwrap(),
            "CLOBBERED\n"
        );
        assert!(proj_dir.join("brand_new.rs").exists());

        let touched = store
            .rollback_applied_changes(&proj.root_path, &task.id)
            .unwrap();
        assert_eq!(touched.len(), 2);
        // Overwritten file is restored...
        assert_eq!(
            fs::read_to_string(proj_dir.join("existing.rs")).unwrap(),
            "ORIGINAL\n"
        );
        // ...and the created file is removed.
        assert!(!proj_dir.join("brand_new.rs").exists());

        // A second rollback must fail rather than silently succeed.
        assert!(store
            .rollback_applied_changes(&proj.root_path, &task.id)
            .is_err());
    }

    #[test]
    fn rollback_without_snapshot_errors() {
        let temp_dir = std::env::temp_dir().join(format!("ghostlink_rb2_{}", Uuid::new_v4()));
        let store = Arc::new(TaskRuntimeStore::new(&temp_dir).unwrap());
        let proj_dir = std::env::temp_dir().join(format!("ghostlink_rb2_proj_{}", Uuid::new_v4()));
        fs::create_dir_all(&proj_dir).unwrap();
        let proj = store
            .create_project(
                "P".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let task = store
            .create_task(&proj.id, "g".into(), None, None, None)
            .unwrap();
        assert!(store
            .rollback_applied_changes(&proj.root_path, &task.id)
            .is_err());
    }

    fn msg(role: &str, content: &str) -> serde_json::Value {
        serde_json::json!({ "role": role, "content": content })
    }

    #[test]
    fn governor_keeps_short_transcripts_intact() {
        let g = ContextGovernor {
            token_budget: 100_000,
            reserve_completion_tokens: 0,
            keep_last_turns: 6,
        };
        let msgs = vec![
            msg("system", "sys"),
            msg("user", "goal"),
            msg("assistant", "a1"),
            msg("user", "t1"),
        ];
        let (fitted, dropped) = g.fit(&msgs);
        assert_eq!(dropped, 0);
        assert_eq!(fitted.len(), msgs.len());
    }

    #[test]
    fn governor_pins_system_and_goal_and_keeps_recent_turns() {
        // Tiny budget so older turns cannot fit.
        let g = ContextGovernor {
            token_budget: 40,
            reserve_completion_tokens: 0,
            keep_last_turns: 2,
        };
        let mut msgs = vec![msg("system", "SYSTEM-PROMPT"), msg("user", "TASK-GOAL")];
        for i in 0..20 {
            msgs.push(msg(
                "assistant",
                &format!("assistant turn number {i} padding padding"),
            ));
        }
        let (fitted, dropped) = g.fit(&msgs);
        assert!(dropped > 0, "expected compaction, got none");

        let text: String = fitted
            .iter()
            .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
            .collect::<Vec<_>>()
            .join("|");
        // Pinned prefix survives...
        assert!(text.contains("SYSTEM-PROMPT"), "{text}");
        assert!(text.contains("TASK-GOAL"), "{text}");
        // ...and the most recent turn survives, while an early one is gone.
        assert!(text.contains("assistant turn number 19"), "{text}");
        assert!(!text.contains("assistant turn number 0 "), "{text}");
    }

    #[test]
    fn governor_never_drops_below_pinned_prefix() {
        // Budget smaller than the pinned prompt itself.
        let g = ContextGovernor {
            token_budget: 4,
            reserve_completion_tokens: 0,
            keep_last_turns: 0,
        };
        let msgs = vec![
            msg(
                "system",
                "a-very-long-system-prompt-that-exceeds-the-budget",
            ),
            msg("user", "the-goal"),
            msg("assistant", "x"),
        ];
        let (fitted, _dropped) = g.fit(&msgs);
        assert!(fitted.len() >= 2, "pinned prefix must survive: {fitted:?}");
        let text: String = fitted
            .iter()
            .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
            .collect::<Vec<_>>()
            .join("|");
        assert!(text.contains("a-very-long-system-prompt"), "{text}");
        assert!(text.contains("the-goal"), "{text}");
    }

    #[test]
    fn governor_reserves_completion_budget() {
        let msgs: Vec<serde_json::Value> = (0..50)
            .map(|_| msg("assistant", &"x".repeat(400)))
            .collect();
        // ~100 tokens/msg. With a 1000 budget and 900 reserved, little fits
        // beyond the pinned prefix.
        let g = ContextGovernor {
            token_budget: 1000,
            reserve_completion_tokens: 900,
            keep_last_turns: 1,
        };
        let (fitted, _d) = g.fit(&msgs);
        assert!(fitted.len() < msgs.len());
        // keep_last_turns still guarantees the most recent message.
        assert!(fitted.len() >= 3, "{fitted:?}");
    }

    // ---- Verification -----------------------------------------------------

    #[test]
    fn verification_plan_detects_cargo_project() {
        let dir = std::env::temp_dir().join(format!("gl_vp_cargo_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Cargo.toml"), "[workspace]\n").unwrap();

        let plan = VerificationPlan::detect(&dir.to_string_lossy()).expect("should detect");
        assert!(plan.required);
        assert_eq!(plan.commands, vec![vec!["cargo", "test", "--workspace"]]);
        // A detected plan must never contain something the Judge would refuse.
        assert!(plan.disallowed_commands().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verification_plan_requires_a_package_test_script() {
        let dir = std::env::temp_dir().join(format!("gl_vp_npm_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        // No test script -> not a verification target.
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"build":"vite build"}}"#,
        )
        .unwrap();
        assert!(VerificationPlan::detect(&dir.to_string_lossy()).is_none());

        // With one -> detected.
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"test":"vitest run"}}"#,
        )
        .unwrap();
        let plan = VerificationPlan::detect(&dir.to_string_lossy()).expect("should detect");
        assert_eq!(plan.commands, vec![vec!["npx", "vitest", "run"]]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verification_plan_none_for_unknown_project() {
        let dir = std::env::temp_dir().join(format!("gl_vp_none_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        assert!(VerificationPlan::detect(&dir.to_string_lossy()).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verification_plan_flags_disallowed_commands() {
        // A plan asking for something the Judge denies must be reportable,
        // not silently executed.
        let plan = VerificationPlan {
            commands: vec![
                vec!["cargo".into(), "test".into()],
                vec!["rm".into(), "-rf".into(), "/".into()],
            ],
            required: true,
        };
        let bad = plan.disallowed_commands();
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0][0], "rm");
    }

    #[tokio::test]
    async fn verification_runs_commands_and_reports_failures() {
        let dir = std::env::temp_dir().join(format!("gl_vr_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        // `cargo test` with no Cargo.toml fails; `cargo check` likewise. Use
        // two allowed commands so we exercise both pass and fail paths without
        // depending on a toolchain being present for a real test run.
        let plan = VerificationPlan {
            commands: vec![
                // `cargo check` in an empty dir exits non-zero -> failure path
                vec!["cargo".into(), "check".into()],
            ],
            required: true,
        };
        let results = run_verification_plan(
            &plan,
            &dir.to_string_lossy(),
            std::time::Duration::from_secs(120),
        )
        .await;

        assert_eq!(results.len(), 1);
        // Either it ran and failed (no manifest), or the toolchain is absent and
        // it failed to spawn — both are `passed == false`, which is the point.
        assert!(!results[0].passed, "expected failure: {:?}", results[0]);
        assert!(results[0].summary().contains("FAIL") || results[0].summary().contains("TIMED"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// The verification copy must exclude everything that is not source.
    ///
    /// This is a size test, not a tidiness test. `copy_dir_recursive` feeds the
    /// scratch tree that `cargo test` builds in, and on this repository the copy
    /// was 40 GB -- 38.8 GB of it `models/*.gguf`. Verification therefore could not
    /// run at all: the copy outlasted the timeout and consumed the disk. A test
    /// that only checked "does it copy Cargo.toml" would have passed while the
    /// feature was unusable.
    #[test]
    fn verification_copy_excludes_models_and_third_party() {
        // dst must NOT live inside src: copy_dir_recursive enumerates src, so a
        // nested destination is recursed into forever.
        let root = std::env::temp_dir().join(format!("gl_copy_{}", Uuid::new_v4()));
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap();

        // Source files that must survive.
        fs::write(src.join("Cargo.toml"), "[workspace]").unwrap();
        fs::create_dir_all(src.join("crates/core/src")).unwrap();
        fs::write(src.join("crates/core/src/lib.rs"), "pub fn f() {}").unwrap();

        // Heavyweight things that must not.
        fs::create_dir_all(src.join("models")).unwrap();
        fs::write(src.join("models/big.gguf"), [0u8; 4096]).unwrap();
        fs::create_dir_all(src.join("third_party/llama.cpp")).unwrap();
        fs::write(src.join("third_party/llama.cpp/x.cpp"), "// x").unwrap();
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join(".git/HEAD"), "ref: x").unwrap();
        fs::create_dir_all(src.join("node_modules")).unwrap();
        fs::write(src.join("node_modules/x.js"), "// x").unwrap();

        copy_dir_recursive(&src, &dst).unwrap();

        assert!(dst.join("Cargo.toml").exists(), "manifest must be copied");
        assert!(
            dst.join("crates/core/src/lib.rs").exists(),
            "source must be copied"
        );
        for skipped in ["models", "third_party", ".git", "node_modules"] {
            assert!(
                !dst.join(skipped).exists(),
                "{skipped} must not be copied into the verification tree"
            );
        }

        let _ = fs::remove_dir_all(&root);
    }

    /// A destination inside the source is refused rather than recursed into.
    ///
    /// `copy_dir_recursive` enumerates the source, so a nested destination is
    /// yielded as an entry and copied into itself forever. The scratch directory is
    /// created separately in production so this cannot fire today -- but the
    /// failure mode is a silent unbounded copy and a stack overflow, which is the
    /// kind of thing worth making impossible rather than merely unlikely.
    #[test]
    fn verification_copy_refuses_a_destination_inside_the_source() {
        let root = std::env::temp_dir().join(format!("gl_nest_{}", Uuid::new_v4()));
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), "x").unwrap();

        let nested = src.join("dst");
        let err = copy_dir_recursive(&src, &nested).unwrap_err().to_string();
        assert!(
            err.contains("inside the source"),
            "expected a clear refusal, got: {err}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// A file over the size threshold is dropped even outside a known directory
    /// name, so an unrecognised multi-hundred-megabyte artefact (a RAG index, a
    /// bundled model under a different name) cannot quietly refill the copy.
    #[test]
    fn verification_copy_drops_oversized_files() {
        let root = std::env::temp_dir().join(format!("gl_big_{}", Uuid::new_v4()));
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("small.txt"), "keep me").unwrap();
        // Written in chunks: a single 65MB `vec!` allocation overflows a test
        // thread's stack before the copy code ever sees it.
        {
            use std::io::Write;
            let mut f = fs::File::create(src.join("huge.bin")).unwrap();
            let chunk = vec![0u8; 1024 * 1024];
            for _ in 0..65 {
                f.write_all(&chunk).unwrap();
            }
        }

        copy_dir_recursive(&src, &dst).unwrap();

        assert!(dst.join("small.txt").exists(), "small files must be copied");
        assert!(
            !dst.join("huge.bin").exists(),
            "a 65MB file should be skipped by the size guard"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// The verification path itself, independent of whether a model chose to
    /// write a file.
    ///
    /// The end-to-end runs were inconclusive: with no model driving the tool loop
    /// the implementer proposes nothing, so verification is (correctly) skipped
    /// and an end-to-end assertion cannot distinguish "verification is broken" from
    /// "the model did nothing". This exercises the part that matters -- that a
    /// real change in a real project is checked by the project's own definition of
    /// done, and that a failing check is recorded as a risk rather than as success.
    #[tokio::test]
    async fn a_real_change_is_verified_by_the_projects_own_test_command() {
        let dir = std::env::temp_dir().join(format!("gl_vreal_{}", Uuid::new_v4()));
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        // A crate whose test suite genuinely exercises the source, so a passing
        // run means something.
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"gl_vreal\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )
        .unwrap();
        fs::write(
            src_dir.join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn adds() { assert_eq!(add(2, 2), 4); }\n}\n",
        )
        .unwrap();

        let plan = VerificationPlan::detect(&dir.to_string_lossy())
            .expect("a crate with Cargo.toml must yield a plan");
        assert_eq!(
            plan.commands,
            vec![vec![
                "cargo".to_string(),
                "test".to_string(),
                "--workspace".to_string()
            ]]
        );
        assert!(
            plan.disallowed_commands().is_empty(),
            "the plan must be runnable"
        );

        let results = run_verification_plan(
            &plan,
            &dir.to_string_lossy(),
            std::time::Duration::from_secs(300),
        )
        .await;
        assert_eq!(results.len(), 1, "one command, one result");
        let r = &results[0];
        // If the toolchain is unavailable the command cannot spawn; that is a
        // recorded failure, not a pass. Either way the outcome must be reported.
        assert!(
            r.passed || !r.excerpt.is_empty(),
            "a result must either pass or explain itself: {r:?}"
        );
        if r.passed {
            assert!(
                r.excerpt.contains("test result"),
                "unexpected pass: {}",
                r.excerpt
            );
        } else {
            assert!(
                r.excerpt.contains("Failed to spawn") || r.excerpt.contains("STDOUT"),
                "a failure must say why: {}",
                r.excerpt
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// A change that breaks the build must be reported as a risk, never as a pass.
    #[tokio::test]
    async fn a_broken_change_fails_verification() {
        let dir = std::env::temp_dir().join(format!("gl_vbroken_{}", Uuid::new_v4()));
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"gl_vbroken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )
        .unwrap();
        // Deliberately does not compile.
        fs::write(
            src_dir.join("lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + }\n",
        )
        .unwrap();

        let plan = VerificationPlan::detect(&dir.to_string_lossy()).expect("plan");
        let results = run_verification_plan(
            &plan,
            &dir.to_string_lossy(),
            std::time::Duration::from_secs(300),
        )
        .await;
        assert_eq!(results.len(), 1);
        assert!(
            !results[0].passed,
            "a crate that cannot compile must not pass verification: {:?}",
            results[0]
        );
        assert!(
            results[0].summary().starts_with("FAIL"),
            "{}",
            results[0].summary()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn verification_refuses_disallowed_command() {
        let dir = std::env::temp_dir().join(format!("gl_vr2_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let plan = VerificationPlan {
            commands: vec![vec!["sudo".into(), "rm".into(), "-rf".into(), "/".into()]],
            required: true,
        };
        let results = run_verification_plan(
            &plan,
            &dir.to_string_lossy(),
            std::time::Duration::from_secs(5),
        )
        .await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert!(results[0].excerpt.contains("Refused"), "{:?}", results[0]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verification_passed_distinguishes_unverified_from_failed() {
        // The distinction that matters: no results is NOT the same as passing.
        let empty = ReviewPacket {
            id: "r".into(),
            task_id: "t".into(),
            run_id: "run".into(),
            summary: String::new(),
            diffs: vec![],
            commands: vec![],
            checks: vec![],
            risks: vec![],
            verification: vec![],
            placement: None,
            accelerator_usage: None,
            context_summary: None,
            created_at: String::new(),
        };
        assert_eq!(
            empty.verification_passed(),
            None,
            "unverified is not 'passed'"
        );

        let failed = ReviewPacket {
            verification: vec![VerificationResult {
                argv: vec!["cargo".into(), "test".into()],
                exit: 101,
                passed: false,
                excerpt: String::new(),
                timed_out: false,
            }],
            ..empty.clone()
        };
        assert_eq!(failed.verification_passed(), Some(false));

        let ok = ReviewPacket {
            verification: vec![VerificationResult {
                argv: vec!["cargo".into(), "test".into()],
                exit: 0,
                passed: true,
                excerpt: String::new(),
                timed_out: false,
            }],
            ..empty.clone()
        };
        assert_eq!(ok.verification_passed(), Some(true));
    }
}

/// Tests for the heterogeneous-agentic-task features: cluster placement,
/// MCP dispatch, auto-fix, and context summarization.
#[cfg(test)]
mod heterogeneous_tests {
    use super::*;

    fn store_in(dir: &std::path::Path) -> Arc<TaskRuntimeStore> {
        Arc::new(TaskRuntimeStore::new(dir).unwrap())
    }

    /// Same as [`store_in`] but with a cluster attached, still wrapped in an
    /// `Arc` — `with_cluster` consumes and returns the bare struct.
    fn store_in_with_cluster(
        dir: &std::path::Path,
        cluster: Arc<ghostlink_core::cluster::ClusterState>,
    ) -> Arc<TaskRuntimeStore> {
        Arc::new(TaskRuntimeStore::new(dir).unwrap().with_cluster(cluster))
    }

    fn proj_with_root(store: &TaskRuntimeStore, root: &std::path::Path) -> Project {
        fs::create_dir_all(root).unwrap();
        store
            .create_project(
                "Hetero Proj".into(),
                ProjectKind::Code,
                root.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap()
    }

    fn fake_run(task_id: &str) -> AgentRun {
        AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task_id.to_string(),
            role: "implementer".into(),
            model: "m".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        }
    }

    // ---- placement -------------------------------------------------------

    /// Single-node mode is the default and must stay byte-identical: no
    /// placement decision, no `placement_decided` event, and the new
    /// ReviewPacket fields absent from the JSON rather than null.
    #[tokio::test]
    async fn single_node_run_has_no_placement_and_no_placement_event() {
        let temp = std::env::temp_dir().join(format!("gl_het_single_{}", Uuid::new_v4()));
        let proj_dir = std::env::temp_dir().join(format!("gl_het_single_p_{}", Uuid::new_v4()));
        let store = store_in(&temp);
        let proj = proj_with_root(&store, &proj_dir);

        let task = store
            .create_task(&proj.id, "Do a thing".into(), None, None, None)
            .unwrap();

        let backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("done".into()),
                tool_calls: vec![],
                usage_tokens: None,
            }])),
        });
        let mut run = fake_run(&task.id);
        let (review, _status) = TaskRunner::run_loop(
            Arc::clone(&store),
            backend,
            task.clone(),
            &mut run,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();

        assert!(review.placement.is_none(), "single-node must not place");
        assert!(review.accelerator_usage.is_none());
        assert!(review.context_summary.is_none());

        let events = store.get_task_events(&task.id);
        assert!(
            !events.iter().any(|e| e.kind == "placement_decided"),
            "single-node must emit no placement_decided"
        );

        // Absent, not null: the fields must not appear in the serialized packet.
        let json = serde_json::to_value(&review).unwrap();
        assert!(json.get("placement").is_none());
        assert!(json.get("accelerator_usage").is_none());
        assert!(json.get("context_summary").is_none());

        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    /// A cluster with one active node produces a placement decision, the SSE
    /// event, and accelerator usage carried into the packet.
    #[tokio::test]
    async fn clustered_run_places_and_emits_event() {
        let temp = std::env::temp_dir().join(format!("gl_het_cluster_{}", Uuid::new_v4()));
        let proj_dir = std::env::temp_dir().join(format!("gl_het_cluster_p_{}", Uuid::new_v4()));
        let cluster = Arc::new(ghostlink_core::cluster::ClusterState::new());
        cluster.register(ghostlink_core::protocol::NodeResources::new(
            "gpu-one",
            24.0,
            64.0,
            "cuda",
            Some("RTX 4090".to_string()),
        ));
        let store = store_in_with_cluster(&temp, Arc::clone(&cluster));
        let proj = proj_with_root(&store, &proj_dir);

        let task = store
            .create_task(&proj.id, "Heavy coding task".into(), None, None, None)
            .unwrap();

        let backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("done".into()),
                tool_calls: vec![],
                usage_tokens: None,
            }])),
        });
        let mut run = fake_run(&task.id);
        let (review, _status) = TaskRunner::run_loop(
            Arc::clone(&store),
            backend,
            task.clone(),
            &mut run,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();

        let placement = review.placement.as_ref().expect("must place on a cluster");
        assert_eq!(placement.assigned_node, "gpu-one");
        assert_eq!(placement.node_capability.compute_capability, "cuda");
        assert!(
            placement.rationale.contains("VRAM"),
            "rationale: {}",
            placement.rationale
        );

        let usage = review
            .accelerator_usage
            .as_ref()
            .expect("usage must be recorded");
        assert_eq!(usage.node_id, "gpu-one");
        assert_eq!(usage.gpu_name.as_deref(), Some("RTX 4090"));

        let events = store.get_task_events(&task.id);
        let ev = events
            .iter()
            .find(|e| e.kind == "placement_decided")
            .expect("placement_decided must be emitted");
        assert_eq!(ev.payload["assigned_node"], "gpu-one");

        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    // ---- MCP dispatch ----------------------------------------------------

    /// An MCP tool call with no registry attached must be refused, not
    /// silently dropped — and must not appear as a successful check.
    #[tokio::test]
    async fn mcp_tool_without_registry_is_refused() {
        let temp = std::env::temp_dir().join(format!("gl_het_mcp_{}", Uuid::new_v4()));
        let proj_dir = std::env::temp_dir().join(format!("gl_het_mcp_p_{}", Uuid::new_v4()));
        let store = store_in(&temp);
        let proj = proj_with_root(&store, &proj_dir);
        // Grant `mcp_read` through the store's own setter so the policy the
        // run_loop reads back is the one actually persisted.
        store
            .update_project(&proj.id, None, None, Some(proj.allowed_tools))
            .unwrap();
        let mut proj = store.get_project(&proj.id).unwrap();
        proj.allowed_tools.push("mcp_read".to_string());
        store
            .update_project(&proj.id, None, None, Some(proj.allowed_tools))
            .unwrap();
        let proj = store.get_project(&proj.id).unwrap();
        assert!(
            proj.allowed_tools.contains(&"mcp_read".to_string()),
            "test precondition: mcp_read must be granted"
        );

        let task = store
            .create_task(&proj.id, "Search the docs".into(), None, None, None)
            .unwrap();

        let backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("searching".into()),
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "mcp_rag::search".into(),
                    args: serde_json::json!({ "query": "x" }),
                }],
                usage_tokens: None,
            }])),
        });
        let mut run = fake_run(&task.id);
        let (review, _status) = TaskRunner::run_loop(
            Arc::clone(&store),
            backend,
            task.clone(),
            &mut run,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();

        assert!(
            review.risks.iter().any(|r| r.contains("no MCP registry")),
            "the refusal must be recorded as a risk, got: {:?}",
            review.risks
        );
        assert!(
            !review.checks.iter().any(|c| c.contains("MCP")),
            "a refused call must not read as a passed check"
        );

        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    /// A project without `mcp_read` must refuse the call even if a registry
    /// were attached — the project policy is the outer gate.
    #[tokio::test]
    async fn mcp_tool_without_project_permission_is_refused() {
        let temp = std::env::temp_dir().join(format!("gl_het_mcp2_{}", Uuid::new_v4()));
        let proj_dir = std::env::temp_dir().join(format!("gl_het_mcp2_p_{}", Uuid::new_v4()));
        let store = store_in(&temp);
        let proj = proj_with_root(&store, &proj_dir); // no mcp_read

        let task = store
            .create_task(&proj.id, "Search".into(), None, None, None)
            .unwrap();

        let backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("searching".into()),
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "mcp_rag::search".into(),
                    args: serde_json::json!({ "query": "x" }),
                }],
                usage_tokens: None,
            }])),
        });
        let mut run = fake_run(&task.id);
        let (review, _status) = TaskRunner::run_loop(
            Arc::clone(&store),
            backend,
            task.clone(),
            &mut run,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();

        assert!(
            review
                .risks
                .iter()
                .any(|r| r.contains("mcp_read is not enabled")),
            "got: {:?}",
            review.risks
        );

        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    // ---- context summary -------------------------------------------------

    /// The helper recovers file paths from `write_file` tool calls and
    /// deduplicates them. It reads arguments, never the filesystem, so it
    /// cannot name a file that was never written.
    #[test]
    fn files_touched_helper_reads_tool_call_args_and_dedupes() {
        let msgs = vec![
            serde_json::json!({ "role": "assistant", "tool_calls": [
                { "name": "write_file", "args": { "path": "a.rs" } },
                { "name": "write_file", "args": { "path": "a.rs" } },
                { "name": "write_file", "args": { "path": "b.rs" } },
                { "name": "read_file",  "args": { "path": "c.rs" } },
            ]}),
            serde_json::json!({ "role": "user", "content": "plain text" }),
        ];
        let files = files_touched_in_compacted_turns(&msgs);
        assert_eq!(files, vec!["a.rs".to_string(), "b.rs".to_string()]);
    }

    #[test]
    fn files_touched_helper_ignores_messages_without_tool_calls() {
        let msgs = vec![
            serde_json::json!({ "role": "user", "content": "hello" }),
            serde_json::json!({ "role": "assistant", "content": "hi" }),
        ];
        assert!(files_touched_in_compacted_turns(&msgs).is_empty());
    }

    // ---- placement rationale ---------------------------------------------

    /// The rationale must name the class the chooser actually matched, or the
    /// ReviewPacket lies to the human reading it.
    #[test]
    fn placement_rationale_matches_chooser_branches() {
        assert!(placement_rationale("vision task").contains("NPU"));
        assert!(placement_rationale("heavy coding").contains("VRAM"));
        assert!(placement_rationale("light verification").contains("CPU"));
        assert!(placement_rationale("do something").contains("first active node"));
    }
}

/// Auto-fix loop tests. The loop is the one place the runtime re-enters its
/// own step loop, so its bounds are what matter.
#[cfg(test)]
mod autofix_tests {
    use super::*;

    fn store_in(dir: &std::path::Path) -> Arc<TaskRuntimeStore> {
        Arc::new(TaskRuntimeStore::new(dir).unwrap())
    }

    fn make_run(task_id: &str) -> AgentRun {
        AgentRun {
            id: format!("run_{}", Uuid::new_v4().simple()),
            task_id: task_id.to_string(),
            role: "implementer".into(),
            model: "m".into(),
            status: "running".into(),
            step_count: 0,
            token_count: 0,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            error: None,
        }
    }

    /// A Cargo project whose `cargo test` genuinely fails, so the verification
    /// plan is detected AND fails — the precondition for auto-fix.
    fn failing_cargo_project(root: &std::path::Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"af\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "#[test]\nfn broken() { assert_eq!(1, 2, \"deliberately failing test\"); }\n",
        )
        .unwrap();
    }

    /// The auto-fix loop must not exceed its bound. `MAX_AUTO_FIX_ATTEMPTS` is
    /// 1, so a failing verification produces exactly one `auto_fix_triggered`
    /// event, and the run terminates rather than looping.
    #[tokio::test]
    async fn auto_fix_fires_exactly_once_on_failing_verification() {
        let temp = std::env::temp_dir().join(format!("gl_af_{}", Uuid::new_v4()));
        let proj_dir = std::env::temp_dir().join(format!("gl_af_p_{}", Uuid::new_v4()));
        let store = store_in(&temp);
        failing_cargo_project(&proj_dir);

        let proj = store
            .create_project(
                "AF Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();

        let budget = TaskBudget {
            max_steps: 20,
            ..Default::default()
        };
        let task = store
            .create_task(
                &proj.id,
                "Fix the failing test".into(),
                None,
                Some(budget),
                None,
            )
            .unwrap();

        // Turn 1 writes a file (so `diffs` is non-empty and verification runs),
        // then every later turn is empty so the loop terminates.
        let backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![
                AgentResponse {
                    content: Some("writing".into()),
                    tool_calls: vec![ToolCall {
                        id: "c1".into(),
                        name: "write_file".into(),
                        args: serde_json::json!({ "path": "src/lib.rs", "content": "still broken" }),
                    }],
                    usage_tokens: None,
                },
                AgentResponse {
                    content: Some("done".into()),
                    tool_calls: vec![],
                    usage_tokens: None,
                },
                AgentResponse {
                    content: Some("done".into()),
                    tool_calls: vec![],
                    usage_tokens: None,
                },
            ])),
        });

        let mut run = make_run(&task.id);
        let (review, _status) = TaskRunner::run_loop(
            Arc::clone(&store),
            backend,
            task.clone(),
            &mut run,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();

        let events = store.get_task_events(&task.id);
        let autofix: Vec<_> = events
            .iter()
            .filter(|e| e.kind == "auto_fix_triggered")
            .collect();
        assert_eq!(
            autofix.len(),
            1,
            "auto-fix must fire exactly once, got {} events",
            autofix.len()
        );
        assert_eq!(autofix[0].payload["attempt"], 1);
        assert_eq!(autofix[0].payload["max_attempts"], 1);
        assert!(
            autofix[0].payload["failures"]
                .as_str()
                .unwrap_or("")
                .contains("FAIL"),
            "the failure summary must name what failed: {:?}",
            autofix[0].payload["failures"]
        );

        // The retry's verification replaced the stale failure rather than
        // keeping both, and the packet still reports the real state.
        assert!(
            review.verification.iter().any(|r| !r.passed),
            "the packet must still show the failing verification"
        );

        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&proj_dir);
    }

    /// With no failing verification there is no auto-fix at all: a passing run
    /// must not be re-entered.
    #[tokio::test]
    async fn auto_fix_does_not_fire_when_verification_passes() {
        let temp = std::env::temp_dir().join(format!("gl_af_ok_{}", Uuid::new_v4()));
        let proj_dir = std::env::temp_dir().join(format!("gl_af_ok_p_{}", Uuid::new_v4()));
        let store = store_in(&temp);
        fs::create_dir_all(proj_dir.join("src")).unwrap();
        fs::write(
            proj_dir.join("Cargo.toml"),
            "[package]\nname = \"ok\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            proj_dir.join("src/lib.rs"),
            "#[test]\nfn fine() { assert_eq!(1, 1); }\n",
        )
        .unwrap();

        let proj = store
            .create_project(
                "OK Proj".into(),
                ProjectKind::Code,
                proj_dir.to_string_lossy().to_string(),
                None,
                None,
            )
            .unwrap();
        let budget = TaskBudget {
            max_steps: 20,
            ..Default::default()
        };
        let task = store
            .create_task(
                &proj.id,
                "Write a passing test".into(),
                None,
                Some(budget),
                None,
            )
            .unwrap();

        let backend = Arc::new(FakeBackend {
            responses: Arc::new(Mutex::new(vec![AgentResponse {
                content: Some("done".into()),
                tool_calls: vec![],
                usage_tokens: None,
            }])),
        });
        let mut run = make_run(&task.id);
        let _ = TaskRunner::run_loop(
            Arc::clone(&store),
            backend,
            task.clone(),
            &mut run,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();

        let events = store.get_task_events(&task.id);
        assert!(
            !events.iter().any(|e| e.kind == "auto_fix_triggered"),
            "a passing run must not trigger auto-fix"
        );

        let _ = fs::remove_dir_all(&temp);
        let _ = fs::remove_dir_all(&proj_dir);
    }
}
