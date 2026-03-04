use chrono::Utc;
use reqwest::Client;
use reqwest::RequestBuilder;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use uuid::Uuid;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:42618";
const DEFAULT_CHANNEL: &str = "cli";
const DEFAULT_SOURCE: &str = "worker_codex";
const DEFAULT_WORKER_TYPE: &str = "codex";
const DEFAULT_PRIORITY: &str = "p1";
const DEFAULT_MAX_ATTEMPTS: i64 = 3;
const DEFAULT_SUMMARY_SOURCE: &str = "learn-sync";
const DEFAULT_TIMEOUT_SECS: u64 = 25;
const DEFAULT_TAKEOVER_TAIL_MESSAGES: usize = 8;
const MANAGED_TASK_STATE_RELATIVE_PATH: &str = "lcodex/managed_task.json";
const MANAGED_SYNC_POLL_INTERVAL_SECS: u64 = 2;
const MANAGED_SYNC_MAX_MESSAGE_CHARS: usize = 1800;
const NEED_HUMAN_MAX_QUESTION_CHARS: usize = 800;
const NEED_HUMAN_MAX_DETAIL_CHARS: usize = 1600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeBlazeAction {
    Takeover,
    Learn,
    Detach,
    Done,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeHotkeyRequest {
    pub(crate) key: String,
    pub(crate) action: String,
    pub(crate) codex_home: PathBuf,
    pub(crate) cwd: PathBuf,
    pub(crate) model: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) thread_name: Option<String>,
    pub(crate) resume_command: Option<String>,
    pub(crate) rollout_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct NativeHotkeyResult {
    pub(crate) summary: String,
    pub(crate) details: Option<String>,
}

#[derive(Debug, Clone)]
struct BlazeConfig {
    base_url: String,
    admin_token: Option<String>,
    channel: String,
    source: String,
    worker_type: String,
    priority: String,
    max_attempts: i64,
    timeout_secs: u64,
    summary_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedTaskState {
    task_id: String,
    session_id: String,
    external_session_id: String,
    #[serde(default)]
    worker_id: Option<String>,
    thread_id: Option<String>,
    project_cwd: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct SessionContext {
    session_id: String,
    external_session_id: String,
    worker_id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionEnvelope {
    session: SessionInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionInfo {
    #[serde(rename = "id")]
    id: String,
}

#[derive(Debug, Clone)]
struct BlazeClient {
    client: Client,
    base_url: String,
    admin_token: Option<String>,
}

struct StreamerHandle {
    stop: Arc<AtomicBool>,
    task_id: String,
    join: JoinHandle<()>,
}

static MANAGED_STREAMERS: OnceLock<Mutex<HashMap<String, StreamerHandle>>> = OnceLock::new();

pub(crate) fn parse_native_action(action: &str) -> Option<NativeBlazeAction> {
    match action.trim().to_ascii_lowercase().as_str() {
        "takeover" => Some(NativeBlazeAction::Takeover),
        "learn" => Some(NativeBlazeAction::Learn),
        "detach" => Some(NativeBlazeAction::Detach),
        "done" => Some(NativeBlazeAction::Done),
        _ => None,
    }
}

pub(crate) fn native_blaze_enabled() -> bool {
    parse_bool_env("LCODEX_NATIVE_BLAZE_ENABLED", true)
}

pub(crate) fn native_blaze_prefer_hook() -> bool {
    parse_bool_env("LCODEX_NATIVE_BLAZE_PREFER_HOOK", false)
}

pub(crate) async fn run_native_action(
    req: NativeHotkeyRequest,
    action: NativeBlazeAction,
) -> Result<NativeHotkeyResult, String> {
    let config = BlazeConfig::from_env();
    let client = BlazeClient::new(&config)?;
    let context = open_session_context(&client, &config, &req).await?;

    match action {
        NativeBlazeAction::Takeover => handle_takeover(&client, &config, &req, &context).await,
        NativeBlazeAction::Learn => handle_learn(&client, &config, &req, &context).await,
        NativeBlazeAction::Detach => handle_detach(&client, &req, &context).await,
        NativeBlazeAction::Done => handle_done(&client, &req, &context).await,
    }
}

impl BlazeConfig {
    fn from_env() -> Self {
        let base_url = env::var("BLAZECLAW_ADDNEW_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("BLAZECLAW_BASE_URL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let channel = env::var("BLAZECLAW_ADDNEW_CHANNEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CHANNEL.to_string());
        let source = env::var("BLAZECLAW_ADDNEW_SOURCE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SOURCE.to_string());
        let worker_type = env::var("BLAZECLAW_ADDNEW_WORKER_TYPE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_WORKER_TYPE.to_string());
        let priority = env::var("BLAZECLAW_ADDNEW_PRIORITY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PRIORITY.to_string());
        let max_attempts = env::var("BLAZECLAW_ADDNEW_MAX_ATTEMPTS")
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok())
            .filter(|value| *value >= 1)
            .unwrap_or(DEFAULT_MAX_ATTEMPTS);
        let timeout_secs = env::var("BLAZECLAW_HTTP_MAX_TIME")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value >= 5)
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        let summary_source = env::var("BLAZECLAW_SESSION_SUMMARY_SOURCE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SUMMARY_SOURCE.to_string());

        Self {
            base_url,
            admin_token: env::var("BLAZECLAW_ADMIN_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            channel,
            source,
            worker_type,
            priority,
            max_attempts,
            timeout_secs,
            summary_source,
        }
    }
}

impl BlazeClient {
    fn new(config: &BlazeConfig) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|err| format!("failed to initialize BlazeClaw HTTP client: {err}"))?;
        Ok(Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            admin_token: config.admin_token.clone(),
        })
    }

    async fn post_json(&self, path: &str, payload: Value) -> Result<Value, String> {
        let request = self.client.post(self.url(path)).json(&payload);
        self.send_json("POST", path, request).await
    }

    async fn patch_json(&self, path: &str, payload: Value) -> Result<Value, String> {
        let request = self.client.patch(self.url(path)).json(&payload);
        self.send_json("PATCH", path, request).await
    }

    async fn get_json_with_query(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, String> {
        let request = self.client.get(self.url(path)).query(query);
        self.send_json("GET", path, request).await
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }

    async fn send_json(
        &self,
        method: &str,
        path: &str,
        request: RequestBuilder,
    ) -> Result<Value, String> {
        let mut request = request;
        if let Some(token) = self.admin_token.as_deref() {
            request = request.header("x-blazeclaw-admin-token", token);
        }

        let response = request
            .send()
            .await
            .map_err(|err| format!("{method} {path} failed: {err}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("{method} {path} response read failed: {err}"))?;
        if !status.is_success() {
            return Err(format!(
                "{method} {path} failed ({status}): {}",
                truncate_error_body(&body, 400)
            ));
        }
        if body.trim().is_empty() {
            return Ok(json!({}));
        }
        serde_json::from_str::<Value>(&body)
            .map_err(|err| format!("{method} {path} returned invalid JSON: {err}"))
    }
}

async fn open_session_context(
    client: &BlazeClient,
    config: &BlazeConfig,
    req: &NativeHotkeyRequest,
) -> Result<SessionContext, String> {
    let external_session_id = resolve_external_session_id(req);
    let payload = json!({
        "channel": config.channel,
        "externalSessionId": external_session_id,
    });
    let response = client.post_json("/api/sessions/open", payload).await?;
    let parsed: SessionEnvelope = serde_json::from_value(response)
        .map_err(|err| format!("invalid /api/sessions/open response: {err}"))?;

    Ok(SessionContext {
        session_id: parsed.session.id,
        external_session_id,
        worker_id: resolve_worker_id(req),
    })
}

async fn handle_takeover(
    client: &BlazeClient,
    config: &BlazeConfig,
    req: &NativeHotkeyRequest,
    context: &SessionContext,
) -> Result<NativeHotkeyResult, String> {
    let rollout_raw = read_rollout_raw(req.rollout_path.as_deref())?;
    if let Some(rollout_raw) = rollout_raw.as_deref() {
        let summary_payload = json!({
            "summary": rollout_raw,
            "source": "takeover-sync",
        });
        let path = format!("/api/sessions/{}/summary", context.session_id);
        client.post_json(path.as_str(), summary_payload).await?;
    }

    let user_tail = rollout_raw
        .as_deref()
        .map(|raw| extract_user_messages(raw, DEFAULT_TAKEOVER_TAIL_MESSAGES))
        .unwrap_or_default();
    let title = build_takeover_title(&user_tail, req.cwd.as_path());
    let description = build_takeover_description(req, &user_tail);
    let summary = build_takeover_summary(&user_tail);
    let idempotency_key = format!("lcodex-takeover-{}", Uuid::new_v4());

    let payload = json!({
        "sessionId": context.session_id,
        "title": title,
        "description": description,
        "summary": summary,
        "priority": config.priority,
        "source": config.source,
        "sourceWorkerId": context.worker_id,
        "sourceWorkerType": config.worker_type,
        "maxAttempts": config.max_attempts,
        "requiresHuman": true,
        "idempotencyKey": idempotency_key
    });
    let response = client.post_json("/api/tasks/addnew", payload).await?;
    let task_id = response
        .pointer("/task/id")
        .and_then(Value::as_str)
        .ok_or_else(|| "invalid /api/tasks/addnew response: missing task.id".to_string())?;
    let task_status = response
        .pointer("/task/status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    let managed_state = ManagedTaskState {
        task_id: task_id.to_string(),
        session_id: context.session_id.clone(),
        external_session_id: context.external_session_id.clone(),
        worker_id: Some(context.worker_id.clone()),
        thread_id: req.thread_id.clone(),
        project_cwd: req.cwd.display().to_string(),
        updated_at: Utc::now().to_rfc3339(),
    };
    persist_managed_state(req.codex_home.as_path(), &managed_state)?;
    start_managed_streamer(
        req.clone(),
        config.clone(),
        context.clone(),
        task_id.to_string(),
    )?;

    Ok(NativeHotkeyResult {
        summary: format!("Takeover task created: {task_id} ({task_status})"),
        details: Some(format!(
            "sessionId: {}\nexternalSessionId: {}\nworkerId: {}",
            context.session_id, context.external_session_id, context.worker_id
        )),
    })
}

async fn handle_learn(
    client: &BlazeClient,
    config: &BlazeConfig,
    req: &NativeHotkeyRequest,
    context: &SessionContext,
) -> Result<NativeHotkeyResult, String> {
    let rollout_raw = read_rollout_raw(req.rollout_path.as_deref())?.unwrap_or_else(|| {
        format!(
            "no rollout available\nthread_id={}\nresume_command={}",
            req.thread_id.as_deref().unwrap_or("unknown"),
            req.resume_command.as_deref().unwrap_or("unknown")
        )
    });

    let payload = json!({
        "summary": rollout_raw,
        "source": config.summary_source,
    });
    let path = format!("/api/sessions/{}/summary", context.session_id);
    client.post_json(path.as_str(), payload).await?;

    Ok(NativeHotkeyResult {
        summary: format!("Learn sync uploaded to session {}", context.session_id),
        details: Some(format!(
            "externalSessionId: {}\nsource: {}",
            context.external_session_id, config.summary_source
        )),
    })
}

async fn handle_detach(
    client: &BlazeClient,
    req: &NativeHotkeyRequest,
    context: &SessionContext,
) -> Result<NativeHotkeyResult, String> {
    let task_id = resolve_target_task_id(client, req, context).await?;
    let Some(task_id) = task_id else {
        return Ok(NativeHotkeyResult {
            summary: "No managed task found to detach.".to_string(),
            details: Some(format!("sessionId: {}", context.session_id)),
        });
    };

    let payload = json!({
        "signal": "pause",
        "reason": format!(
            "detached from lcodex hotkey {}",
            req.key
        ),
    });
    let path = format!("/api/tasks/{task_id}/signal");
    client.post_json(path.as_str(), payload).await?;
    clear_managed_state(req.codex_home.as_path())?;
    stop_managed_streamer(req.codex_home.as_path());

    Ok(NativeHotkeyResult {
        summary: format!("Detached managed task: {task_id}"),
        details: Some("signal: pause".to_string()),
    })
}

async fn handle_done(
    client: &BlazeClient,
    req: &NativeHotkeyRequest,
    context: &SessionContext,
) -> Result<NativeHotkeyResult, String> {
    let task_id = resolve_target_task_id(client, req, context).await?;
    let Some(task_id) = task_id else {
        return Ok(NativeHotkeyResult {
            summary: "No managed task found to mark done.".to_string(),
            details: Some(format!("sessionId: {}", context.session_id)),
        });
    };

    let summary = format!(
        "completed from lcodex native hotkey\naction={}\nthread_id={}\nresume={}",
        req.action,
        req.thread_id.as_deref().unwrap_or("unknown"),
        req.resume_command.as_deref().unwrap_or("unknown")
    );
    let payload = json!({
        "status": "done",
        "summary": summary,
        "currentWorkerType": env::var("BLAZECLAW_ADDNEW_WORKER_TYPE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_WORKER_TYPE.to_string()),
        "currentWorkerId": resolve_worker_id(req),
        "setActive": false
    });
    let path = format!("/api/tasks/{task_id}");
    client.patch_json(path.as_str(), payload).await?;
    clear_managed_state(req.codex_home.as_path())?;
    stop_managed_streamer(req.codex_home.as_path());

    Ok(NativeHotkeyResult {
        summary: format!("Marked task done: {task_id}"),
        details: None,
    })
}

async fn resolve_target_task_id(
    client: &BlazeClient,
    req: &NativeHotkeyRequest,
    context: &SessionContext,
) -> Result<Option<String>, String> {
    if let Some(state) = load_managed_state(req.codex_home.as_path())?
        && !state.task_id.trim().is_empty()
    {
        return Ok(Some(state.task_id));
    }

    let response = client
        .get_json_with_query(
            "/api/tasks/active",
            &[("sessionId", context.session_id.as_str())],
        )
        .await?;
    Ok(response
        .pointer("/task/id")
        .and_then(Value::as_str)
        .map(|value| value.to_string()))
}

fn build_takeover_title(user_tail: &[String], cwd: &Path) -> String {
    if let Some(last_user) = user_tail.last() {
        let compact = normalize_single_line(last_user);
        if !compact.is_empty() {
            return truncate_chars(format!("Takeover: {compact}"), 80);
        }
    }
    let project_name = cwd
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    truncate_chars(format!("Takeover: {project_name}"), 80)
}

fn build_takeover_description(req: &NativeHotkeyRequest, user_tail: &[String]) -> String {
    let mut lines = vec![
        "Take over this lcodex worker session for supervision and acceptance.".to_string(),
        "Keep execution on the local worker; focus on status tracking and review.".to_string(),
        format!("project_cwd: {}", req.cwd.display()),
        format!("model: {}", req.model),
        format!(
            "thread_id: {}",
            req.thread_id.as_deref().unwrap_or("unknown")
        ),
        format!(
            "thread_name: {}",
            req.thread_name.as_deref().unwrap_or("unknown")
        ),
        format!(
            "resume_command: {}",
            req.resume_command.as_deref().unwrap_or("unknown")
        ),
    ];

    if !user_tail.is_empty() {
        lines.push("recent_user_messages:".to_string());
        for message in user_tail {
            lines.push(format!("- {}", normalize_single_line(message)));
        }
    }

    lines.join("\n")
}

fn build_takeover_summary(user_tail: &[String]) -> String {
    if let Some(last_user) = user_tail.last() {
        let text = normalize_single_line(last_user);
        if !text.is_empty() {
            return truncate_chars(text, 160);
        }
    }
    "lcodex takeover".to_string()
}

fn resolve_external_session_id(req: &NativeHotkeyRequest) -> String {
    if let Ok(override_id) = env::var("BLAZECLAW_ADDNEW_EXTERNAL_SESSION_ID")
        && !override_id.trim().is_empty()
    {
        return override_id;
    }
    if let Some(thread_id) = req.thread_id.as_deref() {
        return format!("lcodex-thread:{thread_id}");
    }

    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string());
    let host = env::var("HOSTNAME").unwrap_or_else(|_| "host".to_string());
    let project = req
        .cwd
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    format!(
        "lcodex:{user}:{host}:{}:{}",
        sanitize_token(&project),
        std::process::id()
    )
}

fn resolve_worker_id(req: &NativeHotkeyRequest) -> String {
    resolve_worker_id_from(req.cwd.as_path(), req.thread_id.as_deref())
}

fn resolve_worker_id_from(cwd: &Path, thread_id: Option<&str>) -> String {
    if let Ok(override_id) = env::var("BLAZECLAW_ADDNEW_WORKER_ID")
        && !override_id.trim().is_empty()
    {
        return override_id;
    }

    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string());
    let host = env::var("HOSTNAME").unwrap_or_else(|_| "host".to_string());
    let project = cwd
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());
    let instance = thread_id
        .map(short_thread_instance)
        .unwrap_or_else(|| format!("p{}", std::process::id()));

    format!(
        "codex-{}-{}-{}-{}",
        sanitize_token(&user),
        sanitize_token(&host),
        sanitize_token(&project),
        sanitize_token(&instance)
    )
}

fn short_thread_instance(thread_id: &str) -> String {
    let compact: String = thread_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(10)
        .collect();
    if compact.is_empty() {
        "thread".to_string()
    } else {
        format!("t{compact}")
    }
}

fn parse_bool_env(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn read_rollout_raw(path: Option<&Path>) -> Result<Option<String>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read rollout {}: {err}", path.display()))?;
    Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
}

fn extract_user_messages(raw_rollout: &str, limit: usize) -> Vec<String> {
    let mut all_messages = Vec::new();
    for line in raw_rollout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let payload = value.get("payload");
        let Some(payload) = payload else {
            continue;
        };
        if payload.get("type").and_then(Value::as_str) != Some("user_message") {
            continue;
        }
        let message = payload.get("message").and_then(Value::as_str).unwrap_or("");
        let message = message.trim();
        if !message.is_empty() {
            all_messages.push(message.to_string());
        }
    }
    let take_count = all_messages.len().min(limit);
    let start_index = all_messages.len().saturating_sub(take_count);
    all_messages.into_iter().skip(start_index).collect()
}

fn managed_task_state_path(codex_home: &Path) -> PathBuf {
    codex_home.join(MANAGED_TASK_STATE_RELATIVE_PATH)
}

fn managed_streamers() -> &'static Mutex<HashMap<String, StreamerHandle>> {
    MANAGED_STREAMERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn codex_home_key(codex_home: &Path) -> String {
    codex_home.display().to_string()
}

fn stop_managed_streamer(codex_home: &Path) {
    let key = codex_home_key(codex_home);
    let mut registry = managed_streamers().lock().expect("streamer registry lock");
    if let Some(handle) = registry.remove(&key) {
        handle.stop.store(true, Ordering::SeqCst);
        handle.join.abort();
    }
}

fn start_managed_streamer(
    req: NativeHotkeyRequest,
    config: BlazeConfig,
    context: SessionContext,
    task_id: String,
) -> Result<(), String> {
    let key = codex_home_key(req.codex_home.as_path());
    let mut registry = managed_streamers().lock().expect("streamer registry lock");
    if let Some(existing) = registry.get(&key)
        && existing.task_id == task_id
    {
        return Ok(());
    }
    if let Some(existing) = registry.remove(&key) {
        existing.stop.store(true, Ordering::SeqCst);
        existing.join.abort();
    }

    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_task = Arc::clone(&stop);
    let req_for_task = req.clone();
    let config_for_task = config.clone();
    let context_for_task = context.clone();
    let task_id_for_task = task_id.clone();

    let join = tokio::spawn(async move {
        run_managed_streamer_loop(
            req_for_task,
            config_for_task,
            context_for_task,
            task_id_for_task,
            stop_for_task,
        )
        .await;
    });

    registry.insert(
        key,
        StreamerHandle {
            stop,
            task_id,
            join,
        },
    );
    Ok(())
}

async fn run_managed_streamer_loop(
    req: NativeHotkeyRequest,
    config: BlazeConfig,
    context: SessionContext,
    task_id: String,
    stop: Arc<AtomicBool>,
) {
    let Some(rollout_path) = req.rollout_path.clone() else {
        return;
    };
    let Ok(client) = BlazeClient::new(&config) else {
        return;
    };

    let mut sent_line_count: usize = 0;
    let mut idle_ticks: u32 = 0;
    let mut completion_sent: bool = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(Some(state)) = load_managed_state(req.codex_home.as_path()) else {
            return;
        };
        if state.task_id != task_id {
            return;
        }

        let raw = match fs::read_to_string(&rollout_path) {
            Ok(content) => content,
            Err(_) => {
                sleep(Duration::from_secs(MANAGED_SYNC_POLL_INTERVAL_SECS)).await;
                continue;
            }
        };

        let lines: Vec<&str> = raw.lines().collect();
        if lines.len() < sent_line_count {
            sent_line_count = 0;
        }
        let mut pushed_updates = false;
        for (idx, line) in lines.iter().enumerate().skip(sent_line_count) {
            if !completion_sent {
                if let Some(last_message) = parse_rollout_task_complete_line(line) {
                    let output = normalize_single_line(&last_message);
                    let output = if output.is_empty() {
                        None
                    } else {
                        Some(truncate_chars(output, MANAGED_SYNC_MAX_MESSAGE_CHARS))
                    };
                    if notify_task_completed(&client, &config, &context, &task_id, output)
                        .await
                        .is_ok()
                    {
                        completion_sent = true;
                    }
                }
            }
            let Some((role, message)) = parse_rollout_conversation_line(line) else {
                continue;
            };
            let message = normalize_single_line(&message);
            if message.is_empty() {
                continue;
            }
            let formatted = truncate_chars(
                format!(
                    "thread={} role={} seq={} {}",
                    req.thread_id.as_deref().unwrap_or("unknown"),
                    role,
                    idx + 1,
                    message
                ),
                MANAGED_SYNC_MAX_MESSAGE_CHARS,
            );
            let payload = json!({ "message": formatted });
            let path = format!("/api/tasks/{task_id}/callbacks/progress");
            if client.post_json(path.as_str(), payload).await.is_ok() {
                pushed_updates = true;
            }
        }
        sent_line_count = lines.len();

        // Emit lightweight heartbeat every ~30 seconds in idle periods.
        if pushed_updates {
            idle_ticks = 0;
        } else {
            idle_ticks = idle_ticks.saturating_add(1);
        }
        if idle_ticks >= 15 {
            let heartbeat = json!({
                "message": format!(
                    "heartbeat worker={} externalSession={} syncedLines={}",
                    context.worker_id,
                    context.external_session_id,
                    sent_line_count
                )
            });
            let path = format!("/api/tasks/{task_id}/callbacks/progress");
            let _ = client.post_json(path.as_str(), heartbeat).await;
            idle_ticks = 0;
        }

        sleep(Duration::from_secs(MANAGED_SYNC_POLL_INTERVAL_SECS)).await;
    }
}

fn parse_rollout_conversation_line(line: &str) -> Option<(String, String)> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = value.get("payload")?;
    let payload_type = payload.get("type").and_then(Value::as_str)?;
    let role = match payload_type {
        "user_message" => "user",
        "agent_message" => "assistant",
        _ => return None,
    };
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some((role.to_string(), message))
}

fn parse_rollout_task_complete_line(line: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = value.get("payload")?;
    let payload_type = payload.get("type").and_then(Value::as_str)?;
    if payload_type != "task_complete" && payload_type != "turn_complete" {
        return None;
    }
    let message = payload
        .get("last_agent_message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(message)
}

async fn fetch_latest_attempt_id(
    client: &BlazeClient,
    task_id: &str,
) -> Result<String, String> {
    let path = format!("/api/tasks/{task_id}/attempts");
    let response = client.get_json_with_query(path.as_str(), &[("limit", "5")]).await?;
    let attempts = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "invalid attempts response: missing data".to_string())?;
    if attempts.is_empty() {
        return Err("no attempts found for task".to_string());
    }
    for attempt in attempts {
        let ended_at = attempt.get("endedAt");
        if ended_at.is_none() || matches!(ended_at, Some(value) if value.is_null()) {
            if let Some(id) = attempt.get("id").and_then(Value::as_str) {
                return Ok(id.to_string());
            }
        }
    }
    let id = attempts[0]
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "attempt id missing".to_string())?;
    Ok(id.to_string())
}

async fn notify_task_completed(
    client: &BlazeClient,
    config: &BlazeConfig,
    context: &SessionContext,
    task_id: &str,
    output: Option<String>,
) -> Result<(), String> {
    let attempt_id = fetch_latest_attempt_id(client, task_id).await?;
    let payload = json!({
        "attemptId": attempt_id,
        "workerType": config.worker_type,
        "workerId": context.worker_id,
        "output": output
    });
    let path = format!("/api/tasks/{task_id}/callbacks/completed");
    client.post_json(path.as_str(), payload).await?;
    Ok(())
}

async fn notify_task_need_human(
    client: &BlazeClient,
    config: &BlazeConfig,
    context: &SessionContext,
    task_id: &str,
    attempt_id: Option<String>,
    question: String,
    risk_level: String,
    threat_detail: Option<String>,
) -> Result<(), String> {
    let mut payload = serde_json::Map::new();
    payload.insert("question".to_string(), Value::String(question));
    payload.insert(
        "workerType".to_string(),
        Value::String(config.worker_type.clone()),
    );
    payload.insert(
        "workerId".to_string(),
        Value::String(context.worker_id.clone()),
    );
    payload.insert("riskLevel".to_string(), Value::String(risk_level));
    if let Some(attempt_id) = attempt_id {
        payload.insert("attemptId".to_string(), Value::String(attempt_id));
    }
    if let Some(threat_detail) = threat_detail {
        payload.insert("threatDetail".to_string(), Value::String(threat_detail));
    }
    let path = format!("/api/tasks/{task_id}/callbacks/need-human");
    client.post_json(path.as_str(), Value::Object(payload)).await?;
    Ok(())
}

fn managed_state_matches_thread(state: &ManagedTaskState, thread_id: Option<&str>) -> bool {
    match (state.thread_id.as_deref(), thread_id) {
        (Some(state_thread), Some(current)) => state_thread == current,
        (Some(_), None) => false,
        _ => true,
    }
}

fn managed_state_context(
    state: &ManagedTaskState,
    thread_id: Option<&str>,
    cwd: &Path,
) -> SessionContext {
    let fallback_cwd = if state.project_cwd.trim().is_empty() {
        cwd
    } else {
        Path::new(state.project_cwd.as_str())
    };
    let resolved_thread = state.thread_id.as_deref().or(thread_id);
    let worker_id = state.worker_id.clone().unwrap_or_else(|| {
        resolve_worker_id_from(fallback_cwd, resolved_thread)
    });
    SessionContext {
        session_id: state.session_id.clone(),
        external_session_id: state.external_session_id.clone(),
        worker_id,
    }
}

fn normalize_question_text(text: String) -> String {
    let compact = normalize_single_line(&text);
    if compact.is_empty() {
        String::new()
    } else {
        truncate_chars(compact, NEED_HUMAN_MAX_QUESTION_CHARS)
    }
}

fn normalize_threat_detail(text: Option<String>) -> Option<String> {
    let raw = text?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(truncate_chars(trimmed.to_string(), NEED_HUMAN_MAX_DETAIL_CHARS))
    }
}

fn normalize_risk_level(value: String) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "low" || normalized == "medium" || normalized == "high" {
        normalized
    } else {
        "medium".to_string()
    }
}

pub(crate) async fn notify_managed_task_completed(
    codex_home: PathBuf,
    thread_id: Option<String>,
    cwd: PathBuf,
    output: Option<String>,
) -> Result<(), String> {
    if !native_blaze_enabled() {
        return Ok(());
    }
    let config = BlazeConfig::from_env();
    let client = BlazeClient::new(&config)?;
    let Some(state) = load_managed_state(codex_home.as_path())? else {
        return Ok(());
    };
    if state.task_id.trim().is_empty() {
        return Ok(());
    }
    if !managed_state_matches_thread(&state, thread_id.as_deref()) {
        return Ok(());
    }
    let context = managed_state_context(&state, thread_id.as_deref(), cwd.as_path());
    let output = output.map(|value| truncate_chars(value, MANAGED_SYNC_MAX_MESSAGE_CHARS));
    notify_task_completed(&client, &config, &context, &state.task_id, output).await?;
    Ok(())
}

pub(crate) async fn notify_managed_need_human(
    codex_home: PathBuf,
    thread_id: Option<String>,
    cwd: PathBuf,
    question: String,
    risk_level: String,
    threat_detail: Option<String>,
) -> Result<(), String> {
    if !native_blaze_enabled() {
        return Ok(());
    }
    let normalized_question = normalize_question_text(question);
    if normalized_question.is_empty() {
        return Ok(());
    }
    let config = BlazeConfig::from_env();
    let client = BlazeClient::new(&config)?;
    let Some(state) = load_managed_state(codex_home.as_path())? else {
        return Ok(());
    };
    if state.task_id.trim().is_empty() {
        return Ok(());
    }
    if !managed_state_matches_thread(&state, thread_id.as_deref()) {
        return Ok(());
    }
    let context = managed_state_context(&state, thread_id.as_deref(), cwd.as_path());
    let attempt_id = fetch_latest_attempt_id(&client, &state.task_id).await.ok();
    let risk_level = normalize_risk_level(risk_level);
    let threat_detail = normalize_threat_detail(threat_detail);
    notify_task_need_human(
        &client,
        &config,
        &context,
        &state.task_id,
        attempt_id,
        normalized_question,
        risk_level,
        threat_detail,
    )
    .await?;
    Ok(())
}

fn persist_managed_state(codex_home: &Path, state: &ManagedTaskState) -> Result<(), String> {
    let path = managed_task_state_path(codex_home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let serialized = serde_json::to_string_pretty(state)
        .map_err(|err| format!("failed to serialize managed state: {err}"))?;
    fs::write(&path, serialized).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn load_managed_state(codex_home: &Path) -> Result<Option<ManagedTaskState>, String> {
    let path = managed_task_state_path(codex_home);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let parsed = serde_json::from_str::<ManagedTaskState>(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    Ok(Some(parsed))
}

fn clear_managed_state(codex_home: &Path) -> Result<(), String> {
    let path = managed_task_state_path(codex_home);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|err| format!("failed to remove {}: {err}", path.display()))
}

fn sanitize_token(raw: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in raw.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch);
            last_dash = false;
        } else if !last_dash {
            output.push('-');
            last_dash = true;
        }
    }
    let output = output.trim_matches('-').to_string();
    if output.is_empty() {
        "unknown".to_string()
    } else {
        output
    }
}

fn truncate_chars(text: String, max_chars: usize) -> String {
    let mut chars = text.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}...")
    } else {
        collected
    }
}

fn normalize_single_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_error_body(text: &str, max_chars: usize) -> String {
    let compact = normalize_single_line(text);
    truncate_chars(compact, max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_native_action_supported_values() {
        assert_eq!(
            parse_native_action("takeover"),
            Some(NativeBlazeAction::Takeover)
        );
        assert_eq!(parse_native_action("learn"), Some(NativeBlazeAction::Learn));
        assert_eq!(
            parse_native_action("detach"),
            Some(NativeBlazeAction::Detach)
        );
        assert_eq!(parse_native_action("done"), Some(NativeBlazeAction::Done));
        assert_eq!(parse_native_action("unknown"), None);
    }

    #[test]
    fn sanitize_token_normalizes_and_fallbacks() {
        assert_eq!(sanitize_token("My Project"), "my-project");
        assert_eq!(sanitize_token("___"), "unknown");
    }

    #[test]
    fn extract_user_messages_keeps_recent_entries() {
        let raw = r#"{"type":"event_msg","payload":{"type":"user_message","message":"one"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"ignore"}}
{"type":"event_msg","payload":{"type":"user_message","message":"two"}}
{"type":"event_msg","payload":{"type":"user_message","message":"three"}}"#;
        let messages = extract_user_messages(raw, 2);
        assert_eq!(messages, vec!["two".to_string(), "three".to_string()]);
    }

    #[test]
    fn managed_state_roundtrip() {
        let tmp = tempdir().expect("tempdir");
        let state = ManagedTaskState {
            task_id: "task_1".to_string(),
            session_id: "sess_1".to_string(),
            external_session_id: "ext_1".to_string(),
            worker_id: Some("worker_1".to_string()),
            thread_id: Some("thr_1".to_string()),
            project_cwd: "/tmp/demo".to_string(),
            updated_at: "2026-03-03T00:00:00Z".to_string(),
        };

        persist_managed_state(tmp.path(), &state).expect("persist");
        let loaded = load_managed_state(tmp.path())
            .expect("load")
            .expect("state should exist");
        assert_eq!(loaded.task_id, state.task_id);
        assert_eq!(loaded.session_id, state.session_id);

        clear_managed_state(tmp.path()).expect("clear");
        assert!(
            load_managed_state(tmp.path())
                .expect("load after clear")
                .is_none()
        );
    }

    #[test]
    fn build_takeover_title_uses_latest_user_message() {
        let cwd = PathBuf::from("/tmp/sample");
        let title = build_takeover_title(&["first".to_string(), "latest user".to_string()], &cwd);
        assert_eq!(title, "Takeover: latest user");
    }

    #[test]
    fn parse_rollout_conversation_line_extracts_user_and_assistant() {
        let user = r#"{"type":"event_msg","payload":{"type":"user_message","message":"hello"}}"#;
        let assistant =
            r#"{"type":"event_msg","payload":{"type":"agent_message","message":"world"}}"#;
        let unknown = r#"{"type":"event_msg","payload":{"type":"tool_call","message":"x"}}"#;
        assert_eq!(
            parse_rollout_conversation_line(user),
            Some(("user".to_string(), "hello".to_string()))
        );
        assert_eq!(
            parse_rollout_conversation_line(assistant),
            Some(("assistant".to_string(), "world".to_string()))
        );
        assert_eq!(parse_rollout_conversation_line(unknown), None);
    }
}
