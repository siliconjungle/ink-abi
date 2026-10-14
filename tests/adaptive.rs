use serde_json::{json, Value};
fn fixture() -> Value {
    let binding = json!({"source":"a".repeat(64),"compiler":"b".repeat(64),"target":"test","boundary":"owned-serial-state-v1","variants":{"direct":"c".repeat(64),"indexed":"d".repeat(64)}});
    json!({"operation":"adaptive-state","request":{"binding":binding,"profile":{"binding":binding,"variants":{
 "direct":{"cells":{"read@small":{"low_ns":1000,"high_ns":1100},"write@small":{"low_ns":10,"high_ns":11}},"peak_bytes":100},
 "indexed":{"cells":{"read@small":{"low_ns":10,"high_ns":11},"write@small":{"low_ns":1000,"high_ns":1100}},"peak_bytes":200}},"migrations":[{"from":"direct","to":"indexed","high_ns":100,"max_pause_ns":100,"peak_bytes":400,"min_state_elements":0,"max_state_elements":100}]},
 "policy":{"horizon_calls":100,"min_residence_calls":10,"stable_windows":2,"min_gain":0.15,"migration_margin":2,"memory_limit_bytes":500,"max_pause_ns":1000,"max_switches":4},
 "state":{"pending":"indexed","confirmations":1,"last_switch_call":0,"switches":0},"window":{"counts":{"read@small":20},"calls":20,"total_calls":20,"state_elements":10,"saturated":false},"current":"direct"}})
}
fn decide(v: &Value) -> Value {
    ink_abi::selection::request(v).unwrap()
}
#[test]
fn cost_guards_and_native_wasm_parity() {
    let base = fixture();
    assert_eq!(decide(&base)["selected"], "indexed");
    let mut cases = vec![base.clone()];
    for path in [
        "/profile/variants/indexed/cells/read@small/high_ns",
        "/profile/migrations/0/high_ns",
        "/profile/migrations/0/peak_bytes",
        "/profile/migrations/0/max_pause_ns",
    ] {
        let mut v = base.clone();
        *v["request"].pointer_mut(path).unwrap() = json!(1_000_000_000u64);
        assert_eq!(decide(&v)["selected"], "direct");
        cases.push(v);
    }
    for (key, value) in [("saturated", json!(true)), ("state_elements", json!(101))] {
        let mut v = base.clone();
        v["request"]["window"][key] = value;
        assert_eq!(decide(&v)["selected"], "direct");
        cases.push(v);
    }
    let mut v = base.clone();
    v["request"]["window"]["counts"] = json!({"unknown@small":20});
    assert_eq!(decide(&v)["selected"], "direct");
    cases.push(v);
    let mut v = base.clone();
    v["request"]["state"]["last_switch_call"] = json!(19);
    assert_eq!(decide(&v)["selected"], "direct");
    cases.push(v);
    let mut v = base.clone();
    v["request"]["state"]["switches"] = json!(4);
    assert_eq!(decide(&v)["selected"], "direct");
    cases.push(v);
    let mut v = base.clone();
    v["request"]["state"]["confirmations"] = json!(0);
    assert_eq!(decide(&v)["selected"], "direct");
    cases.push(v);
    let mut v = base.clone();
    v["request"]["window"]["counts"] = json!({"write@small":20});
    assert_eq!(decide(&v)["state"]["confirmations"], 0);
    cases.push(v);
    let rows: Vec<_> = cases
        .iter()
        .map(|v| json!({"request":v,"expected":decide(v)}))
        .collect();
    let dir = std::env::temp_dir().join(format!("ink-adaptive-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ink-selection.wasm"), ink_abi::SELECTION_WASM).unwrap();
    std::fs::write(dir.join("selection.mjs"), ink_abi::SELECTION_JS_SOURCE).unwrap();
    std::fs::write(dir.join("cases.json"), serde_json::to_vec(&rows).unwrap()).unwrap();
    std::fs::write(dir.join("run.mjs"),"import assert from 'node:assert/strict';import fs from 'node:fs';import {selectionRequest}from './selection.mjs';for(const c of JSON.parse(fs.readFileSync(new URL('./cases.json',import.meta.url))))assert.deepEqual(await selectionRequest(c.request),c.expected);").unwrap();
    let run = std::process::Command::new("node")
        .arg(dir.join("run.mjs"))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn malformed_and_stale_evidence_reject() {
    for (path, value) in [
        ("/binding/source", json!("e".repeat(64))),
        ("/window/calls", json!(21)),
        ("/policy/min_gain", json!(-1)),
        (
            "/profile/variants/direct/cells/read@small/low_ns",
            json!(2000),
        ),
        ("/window/total_calls", json!(0)),
    ] {
        let mut v = fixture();
        *v["request"].pointer_mut(path).unwrap() = value;
        assert!(ink_abi::selection::request(&v).is_err(), "{path}");
    }
}
