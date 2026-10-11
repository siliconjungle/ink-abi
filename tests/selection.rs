use serde_json::{json, Value};
fn fixture() -> Value {
    json!({"policy":"adaptive","target_key":"device","cost_profile":{"schema":1,"target_key":"device","models":{"kernel":{"boundary":"captured-owned","domains":{"input_elements":[0,65536],"input_bytes":[0,262144]},"backends":{"cpu":{"intercept_ns":100,"terms":{"input_elements":10}},"javascript":{"intercept_ns":20,"terms":{"input_elements":20}},"gpu":{"intercept_ns":1000,"terms":{"input_elements":1}}}}}}})
}
#[test]
fn same_decision_in_native_and_browser_wasm() {
    let options = fixture();
    let mut cases = vec![];
    for n in [0, 1, 10, 1000, 65536, 65537] {
        for eligible in [
            vec!["cpu", "gpu", "javascript"],
            vec!["javascript", "cpu", "gpu"],
            vec!["cpu"],
        ] {
            for boundary in ["captured-owned", "resident-input-owned-readback"] {
                let request = json!({"options":options,"context":{"scope":"kernel","boundary":boundary,"features":{"input_elements":n,"input_bytes":n*4}},"eligible":eligible});
                let native = ink_abi::selection::request(&request).unwrap();
                cases.push(json!({"request":request,"expected":native}));
            }
        }
    }
    for iterations in [0, 1, 65536] {
        let request = json!({"operation":"features","workload":{"inputs":[{"list":true,"count":65536,"bytes":262144},{"list":false,"count":1,"bytes":8}],"outputs":[1,65536],"stages":[{"dispatches":2},{"dispatches":0}],"iterations":iterations}});
        let expected = ink_abi::selection::request(&request).unwrap();
        assert_eq!(expected["input_elements"], 65536);
        assert_eq!(expected["input_bytes"], 262152);
        assert_eq!(expected["dispatches"], iterations * 2);
        cases.push(json!({"request":request,"expected":expected}));
    }
    let dir = std::env::temp_dir().join(format!("ink-selection-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ink-selection.wasm"), ink_abi::SELECTION_WASM).unwrap();
    std::fs::write(dir.join("selection.mjs"), ink_abi::SELECTION_JS_SOURCE).unwrap();
    std::fs::write(dir.join("cases.json"), serde_json::to_vec(&cases).unwrap()).unwrap();
    std::fs::write(dir.join("run.mjs"),"import assert from 'node:assert/strict';import {readFile} from 'node:fs/promises';import {selectionRequest} from './selection.mjs';for(const c of JSON.parse(await readFile(new URL('./cases.json',import.meta.url))))assert.deepEqual(await selectionRequest(c.request),c.expected);").unwrap();
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
fn invalid_profiles_and_absent_baseline_reject() {
    let mut options = fixture();
    options["target_key"] = json!("other");
    assert!(ink_abi::selection::decide(&options, &json!({}), &["cpu".into()]).is_err());
    assert!(ink_abi::selection::decide(&json!({}), &json!({}), &["gpu".into()]).is_err());
    let mut options = fixture();
    options["cost_profile"]["models"]["kernel"]["domains"]["input_elements"] = json!([10, 0]);
    assert!(ink_abi::selection::decide(&options, &json!({}), &["cpu".into()]).is_err());
}
