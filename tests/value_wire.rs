use ink_abi::{
    typed_json::TypedJsonCodec,
    value_wire::{Limits, Schema},
};
use serde_json::json;
use std::collections::BTreeMap;
#[test]
fn exact_typed_json_and_binary_round_trips() {
    let l = Limits::default();
    for (kind, text, want) in [
        ("u32", "4294967295", json!(u32::MAX)),
        ("u32", "4.00e2", json!(400)),
        ("u64", "18446744073709551615", json!(u64::MAX)),
        ("i32", "-2147483648", json!(i32::MIN)),
        (
            "int",
            "123456789012345678901234567890",
            json!({"Int":"123456789012345678901234567890"}),
        ),
        ("int", "-1.20e2", json!({"Int":"-120"})),
        ("int", "0e100000", json!({"Int":"0"})),
        ("f32", "-0", json!({"F32Bits":0x80000000u32})),
        ("bool", "true", json!(true)),
        ("string", "\"雪\\u0000😀\"", json!("雪\u{0}😀")),
        ("unit", "null", json!(null)),
    ] {
        let s = Schema::new(json!({"kind":kind}), BTreeMap::new()).unwrap();
        let c = TypedJsonCodec::new(s.clone());
        let v = c.parse(text, l).unwrap();
        assert_eq!(v, want, "{kind} {text}");
        let bytes = c.parse_binary(text, l).unwrap();
        assert_eq!(s.decode(&bytes, l).unwrap(), v);
        assert_eq!(
            c.parse(&c.stringify_binary(&bytes, l).unwrap(), l).unwrap(),
            v
        );
    }
    let s=Schema::new(json!({"kind":"record","name":"Message","fields":[{"name":"wide","type":{"kind":"u64"}},{"name":"values","type":{"kind":"list","element":{"kind":"int"}}},{"name":"maybe","type":{"kind":"option","element":{"kind":"bool"}}}]}),BTreeMap::new()).unwrap();
    let c = TypedJsonCodec::new(s.clone());
    let bytes = c
        .parse_binary(
            "{\"wide\":18446744073709551615,\"values\":[1,-2],\"maybe\":{\"Some\":true}}",
            l,
        )
        .unwrap();
    let view = s.validate(&bytes, l).unwrap();
    assert_eq!(
        view.field("values")
            .unwrap()
            .unwrap()
            .items()
            .unwrap()
            .map(|v| v.unwrap().text().unwrap())
            .collect::<Vec<_>>(),
        vec!["1", "-2"]
    );
    assert!(view.field("absent").unwrap().is_none());
    assert!(view
        .field("values")
        .unwrap()
        .unwrap()
        .at(2)
        .unwrap()
        .is_none());
    for text in [
        "{}",
        "{\"wide\":1,\"values\":[],\"maybe\":null}",
        "{\"wide\":1,\"values\":[],\"maybe\":{\"None\":null},\"extra\":0}",
    ] {
        assert!(c.parse(text, l).is_err());
    }
}
#[test]
fn invalid_numeric_and_structural_carriers_reject() {
    let l = Limits::default();
    for (kind, bad) in [
        ("u32", "4294967296"),
        ("u64", "18446744073709551616"),
        ("u64", "-1"),
        ("i32", "2147483648"),
        ("int", "1.25"),
        ("int", "1e100000"),
        ("f32", "1e100000"),
    ] {
        let c = TypedJsonCodec::new(Schema::new(json!({"kind":kind}), BTreeMap::new()).unwrap());
        assert!(c.parse(bad, l).is_err(), "{kind} {bad}");
    }
    let s = Schema::new(json!({"kind":"f32"}), BTreeMap::new()).unwrap();
    let c = TypedJsonCodec::new(s.clone());
    for bits in [0x7f800000u32, 0xff800000, 0x7fc00001, 0x7fa00001] {
        let value = json!({"F32Bits":bits});
        let bytes = s.encode(&value, l).unwrap();
        assert_eq!(s.decode(&bytes, l).unwrap(), value);
        assert!(c.stringify_binary(&bytes, l).is_err());
    }
    assert!(s.encode(&json!({"F32Bits":0,"extra":1}), l).is_err());
    assert!(s.encode(&json!(0), Limits { nodes: 0, ..l }).is_err());
    assert!(s
        .encode(
            &json!(0),
            Limits {
                depth: usize::MAX,
                ..l
            }
        )
        .is_err());
    let s = Schema::new(
        json!({"kind":"list","element":{"kind":"bool"}}),
        BTreeMap::new(),
    )
    .unwrap();
    let bytes = s.encode(&json!([true]), l).unwrap();
    for (pos, value) in [(44, u32::MAX), (48, 10)] {
        let mut b = bytes.clone();
        b[pos..pos + 4].copy_from_slice(&value.to_le_bytes());
        assert!(s.validate(&b, l).is_err());
    }
    let mut b = bytes.clone();
    *b.last_mut().unwrap() = 2;
    assert!(s.validate(&b, l).is_err());
}
#[test]
fn schema_bound_frames_and_depth_are_checked() {
    let l = Limits::default();
    let descriptor = json!({"kind":"record","name":"Node","fields":[{"name":"next","type":{"kind":"option","element":{"kind":"ref","name":"Node"}}}]} );
    let s = Schema::new(
        descriptor.clone(),
        BTreeMap::from([("Node".into(), descriptor)]),
    )
    .unwrap();
    let v = json!({"next":{"Some":{"next":{"None":null}}}});
    let bytes = s.encode(&v, l).unwrap();
    assert_eq!(s.decode(&bytes, l).unwrap(), v);
    assert!(s.validate(&bytes, Limits { depth: 1, ..l }).is_err());
    assert!(Schema::new(json!({"kind":"u32","extra":1}), BTreeMap::new()).is_err());
    assert!(Schema::new(json!({"kind":"ref","name":"unknown"}), BTreeMap::new()).is_err());
    let mut bad = s.descriptor();
    bad["identity"] = json!("0".repeat(64));
    assert!(Schema::from_descriptor(&bad).is_err());
}
