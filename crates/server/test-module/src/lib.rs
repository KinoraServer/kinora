//! The module the server's tests install: a little of every extension point,
//! plus a few operations that misbehave on purpose. Real modules use the SDK
//! in the kinora-registry repository; this one spells the ABI out by hand so
//! the server's tests don't depend on another repository.

use serde_json::{json, Value};

#[link(wasm_import_module = "kinora")]
extern "C" {
    fn host_call(ptr: u32, len: u32) -> u64;
}

fn give(bytes: Vec<u8>) -> u64 {
    let bytes = Box::leak(bytes.into_boxed_slice());
    ((bytes.as_mut_ptr() as u64) << 32) | bytes.len() as u64
}

unsafe fn take(ptr: u32, len: u32) -> Vec<u8> {
    Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr as *mut u8, len as usize)).into_vec()
}

#[no_mangle]
pub extern "C" fn kn_alloc(len: u32) -> u32 {
    (give(vec![0u8; len as usize]) >> 32) as u32
}

#[no_mangle]
pub extern "C" fn kn_call(ptr: u32, len: u32) -> u64 {
    let request: Value = serde_json::from_slice(&unsafe { take(ptr, len) }).unwrap_or_default();
    let answer = match handle(&request) {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e }),
    };
    give(answer.to_string().into_bytes())
}

/// Ask the server for something; the raw `{"ok": ..}` / `{"error": ..}` answer.
fn host_raw(request: &Value) -> Value {
    let text = request.to_string();
    let packed = unsafe { host_call(text.as_ptr() as u32, text.len() as u32) };
    serde_json::from_slice(&unsafe { take((packed >> 32) as u32, packed as u32) }).unwrap_or_default()
}

fn host(request: Value) -> Result<Value, String> {
    let mut answer = host_raw(&request);
    match answer.get("error").and_then(Value::as_str) {
        Some(e) => Err(e.to_string()),
        None => Ok(answer["ok"].take()),
    }
}

fn handle(req: &Value) -> Result<Value, String> {
    let (settings, args) = (&req["settings"], &req["args"]);
    let setting = |key: &str| settings[key].as_str().unwrap_or_default().to_string();
    let user = &args["user_id"];
    let key = |scope: &Value| format!("choice:{}:{}", scope["scope"].as_str().unwrap_or(""), scope["id"].as_str().unwrap_or(""));
    let scopes = || args["scopes"].as_array().cloned().unwrap_or_default();

    match req["op"].as_str().unwrap_or_default() {
        // --- metadata_provider ---
        "metadata.ready" => Ok(match setting("api_key").is_empty() {
            true => json!({ "ready": false, "reason": "no key" }),
            false => json!({ "ready": true }),
        }),
        "metadata.search" => {
            let title = args["title"].as_str().unwrap_or_default();
            if setting("api_key") == "rejected" {
                return Err("the test provider rejected the API key".into());
            }
            if !setting("base_url").is_empty() {
                // Look it up over HTTP, to exercise the http host function.
                let url = format!("{}/search?title={}", setting("base_url"), title.replace(' ', "+"));
                let response = host(json!({ "fn": "http", "method": "GET", "url": url, "cache_seconds": 60 }))?;
                return serde_json::from_str(response["body"].as_str().unwrap_or("null")).map_err(|e| e.to_string());
            }
            Ok(match title {
                "Nothing" => Value::Null,
                _ => json!({ "provider_id": "42", "title": format!("{title} (matched)"), "year": args["year"], "genres": [] }),
            })
        }
        "metadata.details" => Ok(json!({
            "provider_id": args["provider_id"],
            "title": format!("Details for {}", args["provider_id"].as_str().unwrap_or("?")),
            "overview": format!("language {}", setting("language")),
            "genres": ["Test"],
        })),
        "metadata.episode" => Ok(json!({
            "provider_id": "e",
            "title": format!("{} S{}E{}", args["series_provider_id"].as_str().unwrap_or("?"), args["season"], args["episode"]),
            "genres": [],
        })),

        // --- track_selection: remembers the indexes as they are, per scope ---
        "tracks.remember" => {
            let choice = json!({ "audio_index": args["audio_index"], "subtitle_index": args["subtitle_index"] }).to_string();
            for scope in scopes() {
                host(json!({ "fn": "kv_set", "user": user, "key": key(&scope), "value": choice }))?;
            }
            Ok(Value::Null)
        }
        "tracks.pick" => {
            for scope in scopes() {
                if scope["scope"] == "season" && setting("per_season") != "true" {
                    continue;
                }
                let saved = host(json!({ "fn": "kv_get", "user": user, "key": key(&scope) }))?;
                if let Some(choice) = saved.as_str().and_then(|s| serde_json::from_str::<Value>(s).ok()) {
                    return Ok(json!({
                        "from": scope["scope"],
                        "audio_index": choice["audio_index"],
                        "subtitle_index": choice["subtitle_index"],
                    }));
                }
            }
            Ok(Value::Null)
        }
        "tracks.forget" => host(json!({ "fn": "kv_clear", "user": user, "prefix": "choice:" })),

        // --- event_subscriber ---
        "event" => {
            let event = &args["event"];
            host(json!({ "fn": "activity_record", "event": event["type"], "user_id": event["user_id"], "payload": event }))
        }

        // --- for the sandbox tests ---
        "test.host" => Ok(host_raw(args)),
        "test.spin" => loop {
            std::hint::black_box(0);
        },
        "test.grow" => {
            let big = vec![1u8; 200 * 1024 * 1024];
            Ok(json!(big.len()))
        }
        "test.panic" => panic!("on purpose"),
        other => Err(format!("unknown operation {other}")),
    }
}
