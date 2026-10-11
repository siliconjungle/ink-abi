use serde_json::json;
#[test]
fn packed_words_match_javascript_for_every_flat_carrier_and_rejections() {
    let scalar = |kind: &str| json!({"kind":kind});
    let vector = json!({"kind":"vector","size":3,"element":scalar("f32")});
    let record = json!({"kind":"record","name":"Mixed","fields":[{"name":"word","type":scalar("u64")},{"name":"vector","type":vector},{"name":"flag","type":scalar("bool")} ]});
    let mut cases = vec![];
    for (t, v) in [
        (scalar("u32"), json!(u32::MAX)),
        (scalar("i32"), json!(i32::MIN)),
        (scalar("u64"), json!("18446744073709551615")),
        (scalar("bool"), json!(true)),
        (scalar("f32"), json!({"F32Bits":0x80000000u32})),
        (scalar("f32"), json!({"F32Bits":0x7fc12345u32})),
        (scalar("f32"), json!({"F32Bits":0x7f800000u32})),
        (vector.clone(), json!([1.0,{"F32Bits":0x80000000u32},2.0])),
        (
            record.clone(),
            json!({"word":"9007199254740993","vector":[1.0,2.0,3.0],"flag":false}),
        ),
    ] {
        for (ty, value) in [
            (t.clone(), v.clone()),
            (
                json!({"kind":"list","element":t.clone()}),
                json!([v.clone(), v.clone()]),
            ),
            (json!({"kind":"list","element":t.clone()}), json!([])),
        ] {
            let d = ink_abi::packed::pack(&value, &ty).unwrap();
            let decoded = ink_abi::packed::unpack(&d, &ty);
            let again = ink_abi::packed::pack(&decoded, &ty).unwrap();
            assert_eq!(again.words, d.words);
            cases.push(json!({"type":ty,"value":value,"words":d.words.as_ref(),"count":d.count,"finite":d.finite,"width":ink_abi::packed_width(&ty).unwrap()}));
        }
    }
    let bad = vec![
        (scalar("u32"), json!(-1)),
        (scalar("i32"), json!(2147483648u64)),
        (scalar("u64"), json!("18446744073709551616")),
        (scalar("bool"), json!(1)),
        (vector, json!([1, 2])),
        (record, json!({})),
        (scalar("f32"), json!({"F32Bits":-1})),
        (scalar("f32"), json!(1e100)),
        (
            json!({"kind":"list","element":{"kind":"list","element":scalar("u32")}}),
            json!([[1]]),
        ),
    ];
    let mut errors = vec![];
    for (t, v) in bad {
        assert!(ink_abi::packed::pack(&v, &t).is_err());
        errors.push(json!({"type":t,"value":v}));
    }
    let dir = std::env::temp_dir().join(format!("ink-abi-packed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("packed.mjs"), ink_abi::PACKED_JS_SOURCE).unwrap();
    std::fs::write(
        dir.join("cases.json"),
        serde_json::to_vec(&json!({"cases":cases,"errors":errors})).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("test.mjs"),"import assert from 'node:assert/strict';import{readFile}from'node:fs/promises';import{pack,unpack,width}from'./packed.mjs';const f=JSON.parse(await readFile(new URL('./cases.json',import.meta.url)));for(const c of f.cases){const d=pack(c.value,c.type);assert.deepEqual(Array.from(d.words),c.words);assert.equal(d.count,c.count);assert.equal(d.finite,c.finite);assert.equal(width(c.type),c.width);assert.deepEqual(Array.from(pack(unpack(d.words,c.type),c.type).words),c.words);}for(const c of f.errors)assert.throws(()=>pack(c.value,c.type));").unwrap();
    let run = std::process::Command::new("node")
        .arg(dir.join("test.mjs"))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    std::fs::write(dir.join("native.mjs"), include_str!("fixtures/native-destination.mjs")).unwrap();
    let native = std::process::Command::new("node").arg(dir.join("native.mjs")).output().unwrap();
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    std::fs::remove_dir_all(dir).unwrap();
}
