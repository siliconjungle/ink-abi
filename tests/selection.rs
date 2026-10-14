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
    // A resident workload prices elements * iterations, with all factors
    // bounded by observed domains. The browser executes this same decision.
    let product_options = json!({"policy":"adaptive","target_key":"device","cost_profile":{"schema":2,"target_key":"device","models":{"session":{"boundary":"captured-owned","domains":{"input_elements":[1,1024],"iterations":[1,64]},"backends":{
        "cpu":{"intercept_ns":10,"terms":{},"products":[{"features":["input_elements","iterations"],"coefficient_ns":10}]},
        "gpu":{"intercept_ns":10000,"terms":{},"products":[{"features":["input_elements","iterations"],"coefficient_ns":1}]}
    }}}}});
    for (n,k,backend) in [(1,1,"cpu"),(1024,1,"cpu"),(1024,32,"gpu"),(1024,65,"cpu")] {
        let request=json!({"options":product_options,"context":{"scope":"session","boundary":"captured-owned","features":{"input_elements":n,"iterations":k}},"eligible":["cpu","gpu"]});
        let native=ink_abi::selection::request(&request).unwrap();
        assert_eq!(native["backend"],backend);
        cases.push(json!({"request":request,"expected":native}));
    }
    for iterations in [0, 1, 65536] {
        let request = json!({"operation":"features","workload":{"inputs":[{"list":true,"count":65536,"bytes":262144},{"list":false,"count":1,"bytes":8}],"outputs":[1,65536],"stages":[{"dispatches":2},{"dispatches":0}],"iterations":iterations}});
        let expected = ink_abi::selection::request(&request).unwrap();
        assert_eq!(expected["input_elements"], 65536);
        assert_eq!(expected["input_bytes"], 262152);
        assert_eq!(expected["dispatches"], iterations * 2);
        cases.push(json!({"request":request,"expected":expected}));
    }
    // Saved joint support narrows ranges without online fitting or probes.
    // A triangle excludes (9,9), although both independent ranges admit it.
    let support=json!({"features":["input_elements","iterations"],"equalities":[],"inequalities":[["0","-1","0"],["0","0","-1"],["-10","1","1"]]});
    let mut joint=json!({"policy":"adaptive","target_key":"device","cost_profile":{"schema":3,"target_key":"device","models":{"joint":{"boundary":"captured-owned","domains":{"input_elements":[0,10],"iterations":[0,10]},"backends":{
        "cpu":{"intercept_ns":1000,"terms":{},"support":support},
        "gpu":{"intercept_ns":1,"terms":{},"support":support}
    }}}}});
    for (n,k,backend) in [(0.0,0.0,"gpu"),(2.0,3.0,"gpu"),(5.0,5.0,"gpu"),(9.0,9.0,"cpu"),(5.0,f64::from_bits(5.0f64.to_bits()+1),"cpu")] {
        let request=json!({"options":joint,"context":{"scope":"joint","boundary":"captured-owned","features":{"input_elements":n,"iterations":k}},"eligible":["cpu","gpu"]});
        let expected=ink_abi::selection::request(&request).unwrap();assert_eq!(expected["backend"],backend);
        cases.push(json!({"request":request,"expected":expected}));
    }
    // A candidate inside its own support cannot switch without supported CPU.
    joint["cost_profile"]["models"]["joint"]["backends"]["cpu"]["support"]["equalities"]=json!([["0","1","-1"]]);
    let request=json!({"options":joint,"context":{"scope":"joint","boundary":"captured-owned","features":{"input_elements":2,"iterations":3}},"eligible":["cpu","gpu"]});
    let expected=ink_abi::selection::request(&request).unwrap();assert_eq!(expected["backend"],"cpu");
    cases.push(json!({"request":request,"expected":expected}));
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

#[test]
fn product_models_require_versioned_bounded_features() {
    let base=json!({"schema":2,"target_key":"device","models":{"session":{"boundary":"captured-owned","domains":{"input_elements":[0,1000],"iterations":[1,32]},"backends":{"cpu":{"intercept_ns":1,"terms":{},"products":[{"features":["input_elements","iterations"],"coefficient_ns":1}]}}}}});
    assert!(ink_abi::selection::validate_profile(&base,"device").is_ok());
    let mut old=base.clone();old["schema"]=json!(1);
    assert!(ink_abi::selection::validate_profile(&old,"device").is_err());
    for features in [json!([]),json!(["iterations"]),json!(["unknown","iterations"]),json!(["input_bytes","iterations"]),json!(["iterations","iterations","iterations","iterations"]),json!([1,"iterations"])] {
        let mut bad=base.clone();bad["models"]["session"]["backends"]["cpu"]["products"][0]["features"]=features;
        assert!(ink_abi::selection::validate_profile(&bad,"device").is_err());
    }
    let mut negative=base;negative["models"]["session"]["backends"]["cpu"]["products"][0]["coefficient_ns"]=json!(-1);
    assert!(ink_abi::selection::validate_profile(&negative,"device").is_err());
}

#[test]
fn joint_domains_require_versioned_bounded_canonical_constraints() {
    let support=json!({"features":["iterations"],"equalities":[],"inequalities":[["-10","1"]]});
    let base=json!({"schema":3,"target_key":"device","models":{"joint":{"boundary":"captured-owned","domains":{"iterations":[0,10]},"backends":{"cpu":{"intercept_ns":1,"terms":{},"support":support}}}}});
    assert!(ink_abi::selection::validate_profile(&base,"device").is_ok());
    for schema in [1,2] {let mut bad=base.clone();bad["schema"]=json!(schema);assert!(ink_abi::selection::validate_profile(&bad,"device").is_err());}
    for row in [json!(["0","0"]),json!(["01","1"]),json!(["-0","1"]),json!(["+1","1"]),json!([0,1]),json!(["1"]),json!(["0","1".repeat(257)])] {
        let mut bad=base.clone();bad["models"]["joint"]["backends"]["cpu"]["support"]["inequalities"]=json!([row]);
        assert!(ink_abi::selection::validate_profile(&bad,"device").is_err());
    }
    for support in [Value::Null,json!({"features":["iterations"],"equalities":[],"inequalities":[]}),json!({"features":["input_bytes"],"equalities":[],"inequalities":[["0","1"]]}),json!({"features":["iterations"],"equalities":[],"inequalities":[["0","1"]],"trusted":true})] {
        let mut bad=base.clone();bad["models"]["joint"]["backends"]["cpu"]["support"]=support;
        assert!(ink_abi::selection::validate_profile(&bad,"device").is_err());
    }
}
