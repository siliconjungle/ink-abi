use ink_abi::{selection, usage};
use serde_json::{json, Value};
#[test]
fn bounded_features_preserve_locality_and_mark_incomplete_samples() {
    let events: Vec<usage::Event> = serde_json::from_value(json!([
    {"kind":"read","key":1,"path":3,"shape":5,"size":128,"scanned":100},
    {"kind":"read","key":1,"path":3,"shape":5,"size":128,"scanned":1},
    {"kind":"write","key":2,"path":3,"touched":1,"invalidated":10,"failed":true}
    ]))
    .unwrap();
    let s = usage::summarize(&events, false).unwrap();
    assert_eq!(s.kinds, [2, 1, 0, 0]);
    assert_eq!(s.repeated_keys, 1);
    assert_eq!(s.consecutive_same_key, 1);
    assert_eq!(s.longest_kind_run, 2);
    assert_eq!(s.size_log2[8], 2);
    assert_eq!(s.scanned, 101);
    let many: Vec<usage::Event> = (0..100)
        .map(|key| serde_json::from_value(json!({"kind":"read","key":key})).unwrap())
        .collect();
    assert!(usage::summarize(&many, false).unwrap().saturated);
    assert!(usage::summarize(&vec![many[0].clone(); 257], false).is_err());
}
#[test]
fn usage_features_are_identical_in_native_and_wasm() {
    let mut cases = vec![];
    for kind in ["read", "write", "scan", "invalidate"] {
        let r = json!({"operation":"usage-summary","events":[{"kind":kind,"key":1,"size":9007199254740991u64}],"overflow":false});
        cases.push(json!({"request":r,"expected":selection::request(&r).unwrap()}));
    }
    let previous = cases[0]["expected"]["summary"].clone();
    let r = json!({"operation":"usage-summary","events":[{"kind":"write"}],"previous":previous});
    let result = selection::request(&r).unwrap();
    assert_eq!(result["operation_mix_drift"], 1.0);
    cases.push(json!({"request":r,"expected":result}));
    let dir = std::env::temp_dir().join(format!("ink-usage-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ink-selection.wasm"), ink_abi::SELECTION_WASM).unwrap();
    std::fs::write(dir.join("selection.mjs"), ink_abi::SELECTION_JS_SOURCE).unwrap();
    std::fs::write(dir.join("cases.json"), serde_json::to_vec(&cases).unwrap()).unwrap();
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
    let bad: Value = json!({"operation":"usage-summary","events":[{"kind":"read","key":-1}]});
    assert!(selection::request(&bad).is_err());
}
