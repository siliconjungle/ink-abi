//! One decision implementation, linked by native hosts and compiled to Wasm for JS.
//! Costs are untrusted profitability evidence; eligible candidates are caller supplied.
use serde_json::{json, Value};
const FEATURES: &[&str] = &[
    "input_elements",
    "input_bytes",
    "output_capacity_elements",
    "dispatches",
    "iterations",
];
const BACKENDS: &[&str] = &["cpu", "javascript", "gpu"];
fn object<'a>(v: &'a Value) -> Result<&'a serde_json::Map<String, Value>, String> {
    v.as_object().ok_or("expected profile object".into())
}
fn price(v: &Value) -> Result<f64, String> {
    let n = v.as_f64().ok_or("expected profile price")?;
    if !n.is_finite() || !(0.0..=1e12).contains(&n) {
        return Err("invalid profile price".into());
    }
    Ok(n)
}

pub fn validate_profile(v: &Value, target: &str) -> Result<(), String> {
    if v.is_null() {
        return Ok(());
    }
    if target.is_empty()
        || target.len() > 4096
        || serde_json::to_vec(v).map_err(|e| e.to_string())?.len() > 4_000_000
    {
        return Err("cost profile target/byte budget".into());
    }
    if (v["schema"] != 1 && v["schema"] != 2) || v["target_key"].as_str() != Some(target) {
        return Err("invalid cost profile target/schema".into());
    }
    let models = object(&v["models"])?;
    if models.len() > 256 {
        return Err("cost profile model budget".into());
    }
    for (scope, m) in models {
        if scope.is_empty()
            || scope.len() > 4096
            || !matches!(
                m["boundary"].as_str(),
                Some("captured-owned" | "resident-input-owned-readback")
            )
        {
            return Err("invalid profile scope/boundary".into());
        }
        let domains = object(&m["domains"])?;
        for (n, r) in domains {
            let a = r.as_array().ok_or("invalid profile domain")?;
            if !FEATURES.contains(&n.as_str()) || a.len() != 2 {
                return Err("invalid profile domain".into());
            }
            let lo = price(&a[0])?;
            let hi = price(&a[1])?;
            if lo > hi {
                return Err("invalid profile domain".into());
            }
        }
        for (n, p) in object(&m["backends"])? {
            if !BACKENDS.contains(&n.as_str()) {
                return Err("invalid profile backend".into());
            }
            price(&p["intercept_ns"])?;
            for (n, w) in object(&p["terms"])? {
                if !FEATURES.contains(&n.as_str()) || !domains.contains_key(n) {
                    return Err("invalid profile term".into());
                }
                price(w)?;
            }
            if let Some(products) = p.get("products") {
                if v["schema"] != 2 { return Err("product costs require profile schema 2".into()); }
                let products = products.as_array().ok_or("invalid profile products")?;
                if products.len() > 32 { return Err("profile product budget".into()); }
                for product in products {
                    let factors = product["features"].as_array().ok_or("invalid product features")?;
                    if !(2..=3).contains(&factors.len()) { return Err("profile product degree".into()); }
                    for factor in factors {
                        let name = factor.as_str().ok_or("invalid product feature")?;
                        if !FEATURES.contains(&name) || !domains.contains_key(name) { return Err("unbounded product feature".into()); }
                    }
                    price(&product["coefficient_ns"])?;
                }
            }
        }
    }
    Ok(())
}

pub fn decide(options: &Value, context: &Value, eligible: &[String]) -> Result<Value, String> {
    if eligible.len() > 3
        || eligible
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != eligible.len()
        || !eligible.iter().any(|n| n == "cpu")
        || eligible.iter().any(|n| !BACKENDS.contains(&n.as_str()))
    {
        return Err("compiled CPU baseline and known candidates required".into());
    }
    let policy = options["policy"].as_str().unwrap_or("adaptive");
    if !matches!(policy, "adaptive" | "static") {
        return Err("invalid execution policy; profiling belongs to the planner".into());
    }
    let static_backend = options["static_backend"].as_str().unwrap_or("cpu");
    if !BACKENDS.contains(&static_backend) {
        return Err("invalid static backend".into());
    }
    let profile = &options["cost_profile"];
    validate_profile(profile, options["target_key"].as_str().unwrap_or(""))?;
    let mut backend = "cpu".to_owned();
    let mut costs = serde_json::Map::new();
    let mut reason = "no comparable saved profile; compiled CPU baseline";
    if policy == "static" {
        backend = static_backend.into();
        reason = "static execution policy";
    } else if let Some(model) = context["scope"]
        .as_str()
        .and_then(|s| profile["models"].get(s))
    {
        let features = &context["features"];
        let comparable = model["boundary"] == context["boundary"]
            && object(&model["domains"])?.iter().all(|(n, r)| {
                features[n].as_f64().is_some_and(|v| {
                    v.is_finite() && v >= r[0].as_f64().unwrap() && v <= r[1].as_f64().unwrap()
                })
            });
        if comparable {
            for candidate in eligible {
                if let Some(p) = model["backends"].get(candidate) {
                    let mut estimate = object(&p["terms"])?
                        .iter()
                        .fold(price(&p["intercept_ns"])?, |v, (n, w)| {
                            v + w.as_f64().unwrap() * features[n].as_f64().unwrap()
                        });
                    if let Some(products) = p.get("products").and_then(Value::as_array) {
                        for product in products {
                            let value = product["features"].as_array().unwrap().iter().fold(
                                product["coefficient_ns"].as_f64().unwrap(),
                                |v, n| v * features[n.as_str().unwrap()].as_f64().unwrap());
                            estimate += value;
                        }
                    }
                    if estimate.is_finite() {
                        costs.insert(candidate.clone(), json!(estimate));
                    }
                }
            }
            if let Some(baseline) = costs.get("cpu").and_then(Value::as_f64) {
                let mut best = "cpu";
                let mut best_cost = baseline;
                // Canonical tie order independent of target enumeration or host language.
                for name in BACKENDS {
                    if let Some(c) = costs.get(*name).and_then(Value::as_f64) {
                        if c < best_cost {
                            best = name;
                            best_cost = c;
                        }
                    }
                }
                if best_cost < baseline * 0.9 {
                    backend = best.into();
                }
                reason = "saved target profile; 10% minimum predicted saving; no runtime probes";
            }
        }
    }
    if !eligible.contains(&backend) {
        backend = "cpu".into();
        reason = "static target unavailable; compiled CPU baseline";
    }
    Ok(json!({"backend":backend,"reason":reason,"cost_ns":costs,"policy":policy}))
}

/// Common workload features from checked transport/stage descriptors, never values.
pub fn features(v: &Value) -> Result<Value, String> {
    fn number(v: &Value) -> Result<u64, String> {
        v.as_u64()
            .filter(|n| *n <= 1_000_000_000_000)
            .ok_or("invalid workload count".into())
    }
    fn add(a: u64, b: u64) -> Result<u64, String> {
        a.checked_add(b)
            .filter(|n| *n <= 1_000_000_000_000)
            .ok_or("workload count overflow".into())
    }
    let inputs = v["inputs"].as_array().ok_or("missing input descriptors")?;
    let stages = v["stages"].as_array().ok_or("missing stage descriptors")?;
    let outputs = v["outputs"]
        .as_array()
        .ok_or("missing output descriptors")?;
    if inputs.len() > 4096 || stages.len() > 4096 || outputs.len() > 4096 {
        return Err("workload descriptor budget".into());
    }
    let mut elements = 0;
    let mut bytes = 0;
    let mut capacity = 0;
    let mut dispatches = 0;
    for input in inputs {
        if input["list"].as_bool() == Some(true) {
            elements = add(elements, number(&input["count"])?)?;
        }
        bytes = add(bytes, number(&input["bytes"])?)?;
    }
    for output in outputs {
        capacity = add(capacity, number(output)?)?;
    }
    for stage in stages {
        dispatches = add(dispatches, number(&stage["dispatches"])?)?;
    }
    let iterations = number(&v["iterations"])?;
    let dispatches = dispatches
        .checked_mul(iterations)
        .filter(|n| *n <= 1_000_000_000_000)
        .ok_or("workload dispatch overflow")?;
    Ok(
        json!({"input_elements":elements,"input_bytes":bytes,"output_capacity_elements":capacity,"dispatches":dispatches,"iterations":iterations}),
    )
}

pub fn request(v: &Value) -> Result<Value, String> {
    if v["operation"] == "usage-summary" {
        let events: Vec<crate::usage::Event> =
            serde_json::from_value(v["events"].clone()).map_err(|e| e.to_string())?;
        let summary = crate::usage::summarize(&events, v["overflow"].as_bool().unwrap_or(false))?;
        let previous: Option<crate::usage::Summary> = if v["previous"].is_null() {
            None
        } else {
            Some(serde_json::from_value(v["previous"].clone()).map_err(|e| e.to_string())?)
        };
        let drift = previous
            .as_ref()
            .and_then(|p| crate::usage::drift(p, &summary));
        return Ok(json!({"summary":summary,"operation_mix_drift":drift}));
    }
    if v["operation"] == "adaptive-state" {
        let request = serde_json::from_value(v["request"].clone()).map_err(|e| e.to_string())?;
        return serde_json::to_value(crate::adaptive::decide(&request)?).map_err(|e| e.to_string());
    }
    if v["operation"] == "features" {
        return features(&v["workload"]);
    }
    if v["operation"] == "validate" {
        validate_profile(&v["profile"], v["target_key"].as_str().unwrap_or(""))?;
        return Ok(json!({"status":"valid"}));
    }
    let eligible = v["eligible"]
        .as_array()
        .ok_or("missing eligible candidates")?
        .iter()
        .map(|n| {
            n.as_str()
                .map(str::to_owned)
                .ok_or("invalid candidate".into())
        })
        .collect::<Result<Vec<_>, String>>()?;
    decide(&v["options"], &v["context"], &eligible)
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn ink_selection_alloc(n: u32) -> *mut u8 {
    if n > 4_000_000 {
        return std::ptr::null_mut();
    }
    Box::into_raw(vec![0u8; n as usize].into_boxed_slice()).cast()
}
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn ink_selection_free(p: *mut u8, n: u32) {
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
        p, n as usize,
    )));
}
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn ink_selection(p: *const u8, n: u32) -> u64 {
    let result = serde_json::from_slice::<Value>(std::slice::from_raw_parts(p, n as usize))
        .map_err(|e| e.to_string())
        .and_then(|v| request(&v));
    let output = serde_json::to_vec(&result.unwrap_or_else(|error| json!({"error":error})))
        .unwrap()
        .into_boxed_slice();
    let n = output.len() as u64;
    let p = Box::into_raw(output).cast::<u8>() as u64;
    (n << 32) | p
}
