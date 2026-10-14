use ink_abi::{
    json_lookup::{Budget, Lookup, Part},
    json_value::{self, Json},
    value_wire::Limits,
};
use serde_json::json;
fn path(keys: &[&str]) -> Vec<Part> {
    keys.iter().map(|k| Part::Key((*k).into())).collect()
}
#[test]
fn incremental_owned_json_index_preserves_exact_values_and_local_invalidations() {
    let text =
        r#"{"a":{"first":-0,"huge":1.2300e+1000,"null":null},"b":{"x":true},"__proto__":false}"#;
    let mut view = Lookup::new(
        text,
        Limits::default(),
        Budget {
            step_entries: 1,
            min_reads: 1,
            ..Budget::default()
        },
    )
    .unwrap();
    let mut cases = vec![];
    for keys in [
        &["a", "huge"][..],
        &["a", "absent"],
        &["b", "x"],
        &["__proto__"],
    ]
    .into_iter()
    .cycle()
    .take(24)
    {
        let p = path(keys);
        let before = view.stats().built_entries;
        let result = view.lookup(&p).unwrap();
        assert!(view.stats().built_entries - before <= 1);
        cases.push(json!({"path":keys,"value":result.as_ref().map(Json::to_ink),"found":result.is_some(),"stats":view.stats()}));
    }
    assert_eq!(view.stringify().unwrap(), text);
    let before = view.stats().invalidations;
    view.set(&path(&["a"]), "first".into(), "false").unwrap();
    assert_eq!(view.stats().invalidations - before, 1);
    assert_eq!(
        view.lookup(&path(&["a", "first"])).unwrap(),
        Some(Json::Boolean(false))
    );
    let saved = view.stringify().unwrap();
    assert!(view
        .set(&path(&["a"]), "bad".into(), "{\"x\":1,\"x\":2}")
        .is_err());
    assert_eq!(view.stringify().unwrap(), saved);
    let dir = std::env::temp_dir().join(format!("ink-json-index-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("json.mjs"), ink_abi::JSON_VALUE_JS_SOURCE).unwrap();
    std::fs::write(
        dir.join("cases.json"),
        serde_json::to_vec(&json!({"text":text,"cases":cases})).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("run.mjs"),r#"import assert from 'node:assert/strict';import fs from 'node:fs';import{createJsonLookup,stringifyJson}from './json.mjs';
 const {text,cases}=JSON.parse(fs.readFileSync(new URL('./cases.json',import.meta.url)));const v=createJsonLookup(text,{stepEntries:1,minReads:1});
 for(const c of cases){const r=v.lookup(c.path);assert.equal(r.found,c.found);if(r.found)assert.deepEqual(r.value,c.value);assert.deepEqual(v.stats(),c.stats);}
 assert.equal(v.stringify(),text);const before=v.stats().invalidations;v.set(['a'],'first','false');assert.equal(v.stats().invalidations-before,1);assert.equal(stringifyJson(v.lookup(['a','first']).value),'false');
 const saved=v.stringify();assert.throws(()=>v.set(['a'],'bad','{"x":1,"x":2}'));assert.equal(v.stringify(),saved);
 const detached=v.lookup(['a']).value;detached.payload={};assert.equal(v.stringify(),saved);
 assert.throws(()=>v.lookup(['__proto__','x']),/object/);v.replace('[null,123]');assert.equal(stringifyJson(v.lookup([0]).value),'null');assert.equal(v.lookup([2]).found,false);
 const accessor=[];Object.defineProperty(accessor,'0',{get(){throw Error('must not invoke');}});assert.throws(()=>v.lookup(accessor),/JSON lookup path/);
 const noIterator=[0];noIterator[Symbol.iterator]=()=>{throw Error('must not iterate');};assert.equal(stringifyJson(v.lookup(noIterator).value),'null');assert.throws(()=>v.lookup(new Array(1)),/JSON lookup path/);
 "#).unwrap();
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
fn budgets_and_random_operations_match_independent_linear_lookup() {
    let limits = Limits::default();
    for step in [0, 1, 16] {
        let mut view = Lookup::new(
            "{}",
            limits,
            Budget {
                step_entries: step,
                min_reads: 1,
                max_entries: 7,
                max_objects: 2,
                max_index_bytes: 1024,
            },
        )
        .unwrap();
        let mut expected = json_value::parse("{}", limits).unwrap();
        let mut seed = 17u64;
        for i in 0..256 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let key = format!("k{}", (seed >> 32) % 16);
            if i % 3 == 0 {
                view.set(&[], key.clone(), &i.to_string()).unwrap();
                if let Json::Object(ref mut members) = expected {
                    members.retain(|(k, _)| k != &key);
                    members.insert(0, (key.clone(), Json::Number(i.to_string())));
                }
            }
            let actual = view.lookup(&[Part::Key(key.clone())]).unwrap();
            let expected_value = if let Json::Object(ref members) = expected {
                members
                    .iter()
                    .find(|(k, _)| k == &key)
                    .map(|(_, v)| v.clone())
            } else {
                None
            };
            assert_eq!(actual, expected_value);
            assert!(view.stats().index_entries <= 7);
            assert!(view.stats().index_bytes <= 1024);
            assert_eq!(
                view.stringify().unwrap(),
                expected.stringify(limits).unwrap()
            );
        }
    }
}
