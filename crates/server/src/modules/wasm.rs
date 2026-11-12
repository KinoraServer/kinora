//! Runs installed modules: WebAssembly files in a sandbox (module ABI 1,
//! docs/modules.md).
//!
//! A call is JSON in, JSON out. Every call gets a fresh instance with its own
//! memory, a memory cap and a fuel budget, so a module can't keep secrets
//! between calls, eat the server's RAM or spin forever. The only way out of
//! the sandbox is `kinora.host_call`, which offers a handful of functions and
//! checks each against the permissions in the module's manifest.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use kinora_plugin_api::registry::{ModuleSpec, PackageManifest, MODULE_ENTRY};
use kinora_plugin_api::ExtensionPoint;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use wasmi::{Caller, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use crate::state::AppState;

/// Memory one call may use.
const MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
/// Work one call may do: about a second or two of interpreting, far more than
/// parsing a large JSON answer takes. Waiting on the host doesn't count.
#[cfg(not(test))]
const FUEL: u64 = 500_000_000;
/// Unoptimised test builds interpret slowly; keep the runaway test quick.
#[cfg(test)]
const FUEL: u64 = 30_000_000;
/// Host calls one call may make.
const MAX_HOST_CALLS: u32 = 200;
const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_KEY_BYTES: usize = 256;
const MAX_VALUE_BYTES: usize = 256 * 1024;
const HTTP_CACHE_ENTRIES: usize = 64;
const HTTP_CACHE_MAX_SECONDS: u32 = 3600;

/// An installed module, from its package manifest.
#[derive(Debug, Clone)]
pub struct InstalledModule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub spec: ModuleSpec,
}

impl InstalledModule {
    pub fn from_manifest(m: &PackageManifest) -> Option<Self> {
        Some(InstalledModule {
            id: m.id.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            version: m.version.clone(),
            spec: m.module.clone()?,
        })
    }
}

struct CachedResponse {
    until: Instant,
    status: u16,
    body: String,
}

struct Inner {
    engine: Engine,
    /// For modules' requests: no redirects, so an allowed host can't send a module elsewhere.
    http: reqwest::Client,
    installed: RwLock<Option<Arc<Vec<Arc<InstalledModule>>>>>,
    /// Compiled modules by id, with the version they were compiled from.
    compiled: Mutex<HashMap<String, (String, Module)>>,
    http_cache: Mutex<HashMap<String, CachedResponse>>,
}

/// Shared by every request handler; cheap to clone.
#[derive(Clone)]
pub struct ModuleHost {
    inner: Arc<Inner>,
}

fn engine() -> Engine {
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    Engine::new(&config)
}

/// Check a module file before it is installed: valid WebAssembly that exports
/// what the ABI needs and imports nothing but `kinora.host_call`.
pub fn check(wasm: &[u8]) -> Result<(), String> {
    let module = Module::new(&engine(), wasm).map_err(|e| format!("not a valid WebAssembly module: {e}"))?;
    for import in module.imports() {
        if (import.module(), import.name()) != ("kinora", "host_call") {
            return Err(format!(
                "the module imports {}.{}, which Kinora doesn't offer",
                import.module(),
                import.name()
            ));
        }
    }
    for export in ["memory", "kn_alloc", "kn_call"] {
        if module.get_export(export).is_none() {
            return Err(format!("the module doesn't export {export}"));
        }
    }
    Ok(())
}

impl ModuleHost {
    pub fn new() -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("Kinora/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(ModuleHost {
            inner: Arc::new(Inner {
                engine: engine(),
                http,
                installed: RwLock::new(None),
                compiled: Mutex::new(HashMap::new()),
                http_cache: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Forget what is cached about installed modules (after an install or uninstall).
    pub async fn invalidate(&self) {
        *self.inner.installed.write().await = None;
        self.inner.compiled.lock().unwrap().clear();
        self.inner.http_cache.lock().unwrap().clear();
    }

    /// Every installed module, enabled or not, sorted by id.
    pub async fn installed(&self, state: &AppState) -> Arc<Vec<Arc<InstalledModule>>> {
        if let Some(list) = self.inner.installed.read().await.as_ref() {
            return list.clone();
        }
        let mut guard = self.inner.installed.write().await;
        if let Some(list) = guard.as_ref() {
            return list.clone();
        }
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, manifest FROM packages WHERE kind = 'module' ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
        let list: Vec<Arc<InstalledModule>> = rows
            .into_iter()
            .filter_map(|(id, manifest)| {
                let module = serde_json::from_str::<PackageManifest>(&manifest)
                    .ok()
                    .and_then(|m| InstalledModule::from_manifest(&m));
                if module.is_none() {
                    tracing::warn!(module = %id, "ignoring an installed module with an unreadable manifest");
                }
                module.map(Arc::new)
            })
            .collect();
        let list = Arc::new(list);
        *guard = Some(list.clone());
        list
    }

    pub async fn find(&self, state: &AppState, id: &str) -> Option<Arc<InstalledModule>> {
        self.installed(state).await.iter().find(|m| m.id == id).cloned()
    }

    /// Installed and switched-on modules that provide an extension point.
    pub async fn providers(&self, state: &AppState, point: ExtensionPoint) -> Vec<Arc<InstalledModule>> {
        let mut out = Vec::new();
        for m in self.installed(state).await.iter() {
            if m.spec.provides.contains(&point) && super::is_enabled(&state.db, &m.id).await {
                out.push(m.clone());
            }
        }
        out
    }

    fn compiled(&self, state: &AppState, module: &InstalledModule) -> anyhow::Result<Module> {
        if let Some((version, compiled)) = self.inner.compiled.lock().unwrap().get(&module.id) {
            if *version == module.version {
                return Ok(compiled.clone());
            }
        }
        let path = state.config.modules_dir().join(&module.id).join(MODULE_ENTRY);
        let wasm = std::fs::read(&path).with_context(|| format!("module {} is missing its file", module.id))?;
        let compiled = Module::new(&self.inner.engine, &wasm).map_err(|e| anyhow!("module {}: {e}", module.id))?;
        self.inner.compiled.lock().unwrap().insert(module.id.clone(), (module.version.clone(), compiled.clone()));
        Ok(compiled)
    }

    /// Call one operation of a module. Errors the module reports come back as `Err`.
    pub async fn call(
        &self,
        state: &AppState,
        module: &Arc<InstalledModule>,
        op: &str,
        args: Value,
    ) -> anyhow::Result<Value> {
        let saved = super::settings(&state.db, &module.id).await;
        let settings = module.spec.settings_with_defaults(&saved);
        let request = json!({ "op": op, "settings": settings, "args": args }).to_string();
        let compiled = self.compiled(state, module)?;
        let ctx = Ctx {
            state: state.clone(),
            host: self.clone(),
            module: module.clone(),
            runtime: tokio::runtime::Handle::current(),
            limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY_BYTES).instances(1).memories(1).tables(4).build(),
            host_calls: 0,
        };
        let engine = self.inner.engine.clone();
        let id = module.id.clone();
        let result = tokio::task::spawn_blocking(move || run(&engine, &compiled, ctx, request.as_bytes())).await?;
        result.map_err(|e| anyhow!("module {id}: {e}"))
    }
}

/// What a running module can reach.
struct Ctx {
    state: AppState,
    host: ModuleHost,
    module: Arc<InstalledModule>,
    runtime: tokio::runtime::Handle,
    limits: StoreLimits,
    host_calls: u32,
}

fn trap(message: impl Into<String>) -> wasmi::Error {
    wasmi::Error::new(message.into())
}

/// Copy bytes into the module's memory (it allocates) and return `ptr << 32 | len`.
fn give(
    mut ctx: impl wasmi::AsContextMut,
    alloc: &wasmi::TypedFunc<u32, u32>,
    memory: &wasmi::Memory,
    bytes: &[u8],
) -> Result<u64, wasmi::Error> {
    let len = u32::try_from(bytes.len()).map_err(|_| trap("message too large"))?;
    let ptr = alloc.call(&mut ctx, len)?;
    memory.write(&mut ctx, ptr as usize, bytes).map_err(|_| trap("the module returned a bad pointer"))?;
    Ok((u64::from(ptr) << 32) | u64::from(len))
}

fn take(ctx: impl wasmi::AsContext, memory: &wasmi::Memory, ptr: u32, len: u32) -> Result<Vec<u8>, wasmi::Error> {
    if len as usize > MAX_MESSAGE_BYTES {
        return Err(trap("message too large"));
    }
    let mut buf = vec![0u8; len as usize];
    memory.read(&ctx, ptr as usize, &mut buf).map_err(|_| trap("the module passed a bad pointer"))?;
    Ok(buf)
}

fn host_call(mut caller: Caller<'_, Ctx>, ptr: u32, len: u32) -> Result<u64, wasmi::Error> {
    let memory = caller.get_export("memory").and_then(Extern::into_memory).ok_or_else(|| trap("no memory"))?;
    let alloc = caller
        .get_export("kn_alloc")
        .and_then(Extern::into_func)
        .ok_or_else(|| trap("no kn_alloc"))?
        .typed::<u32, u32>(&caller)?;
    let request = take(&caller, &memory, ptr, len)?;
    let ctx = caller.data_mut();
    ctx.host_calls += 1;
    if ctx.host_calls > MAX_HOST_CALLS {
        return Err(trap("too many host calls in one request"));
    }
    let answer = match serde_json::from_slice::<Value>(&request) {
        Ok(request) => {
            let ctx = &*ctx;
            ctx.runtime.block_on(answer(ctx, request))
        }
        Err(e) => Err(format!("bad host request: {e}")),
    };
    let answer = match answer {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e }),
    };
    give(&mut caller, &alloc, &memory, answer.to_string().as_bytes())
}

fn run(engine: &Engine, module: &Module, ctx: Ctx, request: &[u8]) -> anyhow::Result<Value> {
    let mut store = Store::new(engine, ctx);
    store.limiter(|ctx| &mut ctx.limits);
    store.set_fuel(FUEL).map_err(|e| anyhow!("{e}"))?;
    let mut linker = Linker::<Ctx>::new(engine);
    linker.func_wrap("kinora", "host_call", host_call).map_err(|e| anyhow!("{e}"))?;

    let explain = |e: wasmi::Error| match e.as_trap_code() {
        Some(wasmi::TrapCode::OutOfFuel) => anyhow!("it ran for too long and was stopped"),
        _ => anyhow!("it crashed: {e}"),
    };
    let instance = linker.instantiate_and_start(&mut store, module).map_err(explain)?;
    let memory = instance.get_memory(&store, "memory").context("no memory export")?;
    let alloc = instance.get_typed_func::<u32, u32>(&store, "kn_alloc").map_err(|e| anyhow!("kn_alloc: {e}"))?;
    let call = instance.get_typed_func::<(u32, u32), u64>(&store, "kn_call").map_err(|e| anyhow!("kn_call: {e}"))?;

    let packed = give(&mut store, &alloc, &memory, request).map_err(explain)?;
    let packed = call.call(&mut store, ((packed >> 32) as u32, packed as u32)).map_err(explain)?;
    let answer = take(&store, &memory, (packed >> 32) as u32, packed as u32).map_err(explain)?;
    let mut answer: Value = serde_json::from_slice(&answer).context("its answer is not JSON")?;
    if let Some(error) = answer.get("error").and_then(Value::as_str) {
        bail!("{error}");
    }
    Ok(answer.get_mut("ok").map(Value::take).unwrap_or(Value::Null))
}

// ---------------------------------------------------------------------------
// Host functions
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct HttpRequest {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    cache_seconds: u32,
}

#[derive(Deserialize)]
struct KvRequest {
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    key: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    prefix: String,
}

#[derive(Deserialize)]
struct ActivityRequest {
    event: String,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    payload: Value,
}

fn parse<T: serde::de::DeserializeOwned>(request: Value) -> Result<T, String> {
    serde_json::from_value(request).map_err(|e| format!("bad host request: {e}"))
}

async fn answer(ctx: &Ctx, request: Value) -> Result<Value, String> {
    let permissions = &ctx.module.spec.permissions;
    let name = request.get("fn").and_then(Value::as_str).unwrap_or_default().to_string();
    let need = |allowed: bool, what: &str| {
        allowed.then_some(()).ok_or_else(|| format!("this module has no permission for {what}"))
    };
    match name.as_str() {
        "log" => {
            let message = request.get("message").and_then(Value::as_str).unwrap_or_default();
            let message: String = message.chars().take(2000).collect();
            let module = ctx.module.id.as_str();
            match request.get("level").and_then(Value::as_str) {
                Some("error") => tracing::error!(module, "{message}"),
                Some("warn") => tracing::warn!(module, "{message}"),
                Some("debug") => tracing::debug!(module, "{message}"),
                _ => tracing::info!(module, "{message}"),
            }
            Ok(Value::Null)
        }
        "http" => http(ctx, parse(request)?).await,
        "kv_get" | "kv_set" | "kv_delete" | "kv_clear" => {
            need(permissions.storage, "storage")?;
            kv(ctx, &name, parse(request)?).await
        }
        "activity_record" => {
            need(permissions.activity, "the activity log")?;
            let a: ActivityRequest = parse(request)?;
            if a.event.is_empty() || a.event.len() > 64 {
                return Err("the event name must be 1-64 characters".into());
            }
            let payload = a.payload.to_string();
            if payload.len() > MAX_VALUE_BYTES {
                return Err("the payload is too large".into());
            }
            sqlx::query("INSERT INTO activity_log (time, user_id, event, payload) VALUES (?, ?, ?, ?)")
                .bind(crate::db::now_ms())
                .bind(a.user_id)
                .bind(a.event)
                .bind(payload)
                .execute(&ctx.state.db)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        other => Err(format!("unknown host function {other:?}")),
    }
}

async fn kv(ctx: &Ctx, name: &str, r: KvRequest) -> Result<Value, String> {
    if r.key.len() > MAX_KEY_BYTES || r.prefix.len() > MAX_KEY_BYTES || (name != "kv_clear" && r.key.is_empty()) {
        return Err(format!("keys are 1-{MAX_KEY_BYTES} bytes"));
    }
    let (db, module) = (&ctx.state.db, ctx.module.id.as_str());
    let user = r.user.as_deref().unwrap_or("");
    let done = match name {
        "kv_get" => {
            let value: Option<String> = sqlx::query_scalar(
                "SELECT value FROM module_data WHERE module_id = ? AND COALESCE(user_id, '') = ? AND key = ?",
            )
            .bind(module)
            .bind(user)
            .bind(&r.key)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
            return Ok(json!(value));
        }
        "kv_set" => {
            if r.value.len() > MAX_VALUE_BYTES {
                return Err(format!("values are at most {MAX_VALUE_BYTES} bytes"));
            }
            sqlx::query(
                "INSERT INTO module_data (module_id, user_id, key, value, updated_at) VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (module_id, COALESCE(user_id, ''), key) DO UPDATE SET
                    value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(module)
            .bind(r.user.as_deref())
            .bind(&r.key)
            .bind(&r.value)
            .bind(crate::db::now_ms())
            .execute(db)
            .await
        }
        "kv_delete" => {
            sqlx::query("DELETE FROM module_data WHERE module_id = ? AND COALESCE(user_id, '') = ? AND key = ?")
                .bind(module)
                .bind(user)
                .bind(&r.key)
                .execute(db)
                .await
        }
        _ => {
            // Keys are compared as text; escape LIKE's wildcards in the prefix.
            let pattern = format!("{}%", r.prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
            sqlx::query(
                "DELETE FROM module_data WHERE module_id = ? AND COALESCE(user_id, '') = ? AND key LIKE ? ESCAPE '\\'",
            )
            .bind(module)
            .bind(user)
            .bind(pattern)
            .execute(db)
            .await
        }
    };
    // A viewer who no longer exists (foreign key) is the usual failure here.
    done.map(|_| Value::Null).map_err(|e| format!("storage: {e}"))
}

/// Is this module allowed to request this URL? HTTPS on the standard port, to a listed host.
fn allowed_url(url: &str, hosts: &[String]) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "invalid URL".to_string())?;
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if !hosts.contains(&host) {
        return Err(format!("this module has no permission to reach {host:?}"));
    }
    // Tests talk to a local server over plain HTTP.
    let local_test = cfg!(test) && parsed.scheme() == "http";
    if !local_test && (parsed.scheme() != "https" || parsed.port().is_some()) {
        return Err("only https:// URLs on the standard port are allowed".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URLs with a user name or password are not allowed".into());
    }
    Ok(parsed)
}

async fn http(ctx: &Ctx, r: HttpRequest) -> Result<Value, String> {
    let url = allowed_url(&r.url, &ctx.module.spec.permissions.http)?;
    let method = match r.method.to_ascii_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        other => return Err(format!("method {other} is not allowed (GET or POST)")),
    };
    let inner = &ctx.host.inner;
    let cache_seconds = if method == reqwest::Method::GET { r.cache_seconds.min(HTTP_CACHE_MAX_SECONDS) } else { 0 };
    // Headers are part of the key: they can hold the credentials a response was given for.
    let cache_key = format!("{}\n{}\n{:?}", ctx.module.id, r.url, r.headers);
    if cache_seconds > 0 {
        if let Some(hit) = inner.http_cache.lock().unwrap().get(&cache_key).filter(|c| c.until > Instant::now()) {
            return Ok(json!({ "status": hit.status, "body": hit.body }));
        }
    }

    let mut request = inner.http.request(method, url);
    for (name, value) in &r.headers {
        if matches!(name.to_ascii_lowercase().as_str(), "host" | "content-length" | "transfer-encoding" | "connection")
        {
            continue;
        }
        request = request.header(name, value);
    }
    if let Some(body) = r.body {
        request = request.body(body);
    }
    let mut response = request.send().await.map_err(|e| format!("request failed: {}", e.without_url()))?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| format!("request failed: {}", e.without_url()))? {
        if body.len() + chunk.len() > MAX_HTTP_BODY_BYTES {
            return Err("the response is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&body).into_owned();

    if cache_seconds > 0 && status == 200 {
        let mut cache = inner.http_cache.lock().unwrap();
        let now = Instant::now();
        cache.retain(|_, c| c.until > now);
        if cache.len() >= HTTP_CACHE_ENTRIES {
            cache.clear();
        }
        let until = now + Duration::from_secs(u64::from(cache_seconds));
        cache.insert(cache_key, CachedResponse { until, status, body: body.clone() });
    }
    Ok(json!({ "status": status, "body": body }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_held_to_the_permission_list() {
        let hosts = vec!["api.themoviedb.org".to_string()];
        assert!(allowed_url("https://api.themoviedb.org/3/movie/1?x=1", &hosts).is_ok());
        assert!(allowed_url("https://API.TheMovieDB.org/3", &hosts).is_ok());
        for bad in [
            "https://example.com/",
            "https://api.themoviedb.org.evil.com/",
            "https://evil.com/api.themoviedb.org",
            "https://api.themoviedb.org:8443/",
            "https://user:pw@api.themoviedb.org/",
            "ftp://api.themoviedb.org/",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(allowed_url(bad, &hosts).is_err(), "{bad}");
        }
    }

    #[test]
    fn module_files_are_checked() {
        assert!(check(b"not wasm").is_err());
        // The smallest valid module: the header alone. It exports nothing.
        let empty = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        assert!(check(&empty).unwrap_err().contains("memory"));
        assert!(check(super::super::test_support::FIXTURE_WASM).is_ok());
    }
}
