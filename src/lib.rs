//! Target-neutral value descriptors and transport contracts. No Ink compiler dependency.
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub const PACKED_JS_SOURCE: &str = include_str!("packed.mjs");
pub const VALUE_ABI: &str = "ink-values-v1";
pub const MAX_TYPE_DEPTH: usize = 32;
pub mod float64;

/// Describe one type against a program's declarations. Every call serializes
/// typed declarations afresh, so a caller describing many types of one
/// program should serialize `records`, `enums` and `ids` to `Value` once and
/// pass those: arguments that already are JSON values are borrowed as they are.
pub fn describe<T, R, E, I>(ty: &T, records: &R, enums: &E, ids: &I) -> Result<Value, String>
where
    T: Serialize + 'static,
    R: Serialize + 'static,
    E: Serialize + 'static,
    I: Serialize + 'static,
{
    fn json<S: Serialize + 'static>(value: &S) -> Result<std::borrow::Cow<'_, Value>, String> {
        match (value as &dyn std::any::Any).downcast_ref::<Value>() {
            Some(value) => Ok(std::borrow::Cow::Borrowed(value)),
            None => serde_json::to_value(value)
                .map(std::borrow::Cow::Owned)
                .map_err(|e| e.to_string()),
        }
    }
    let (ty, records, enums, ids) = (json(ty)?, json(records)?, json(enums)?, json(ids)?);
    describe_json(&ty, &records, &enums, &ids, &mut BTreeSet::new(), 0)
}

fn describe_json(
    t: &Value,
    records: &Value,
    enums: &Value,
    ids: &Value,
    seen: &mut BTreeSet<String>,
    depth: usize,
) -> Result<Value, String> {
    if depth > MAX_TYPE_DEPTH {
        return Err("value descriptor type depth exceeded".into());
    }
    if let Some(n) = t.as_str() {
        let kind = match n {
            "U32" => "u32",
            "U64" => "u64",
            "I32" => "i32",
            "F32" => "f32",
            "F64" => "f64",
            "Bool" => "bool",
            "Unit" => "unit",
            "String" => "string",
            "Int" => "int",
            _ => return Err(format!("unsupported ABI type {n}")),
        };
        return Ok(json!({"kind":kind}));
    }
    let obj = t.as_object().ok_or("invalid ABI type")?;
    if obj.len() != 1 {
        return Err("invalid ABI type constructor".into());
    }
    let (kind, arg) = obj.iter().next().unwrap();
    let mut recur = |t: &Value| describe_json(t, records, enums, ids, seen, depth + 1);
    Ok(match kind.as_str() {
        "List" | "Option" => {
            json!({"kind":if kind=="List"{"list"}else{"option"},"element":recur(arg)?})
        }
        "Vector" => json!({"kind":"vector","element":recur(&arg[0])?,"size":arg[1]}),
        "Result" => json!({"kind":"result","ok":recur(&arg[0])?,"error":recur(&arg[1])?}),
        "Buffer" | "Map" => json!({"kind":"temporary"}),
        "Named" => {
            let n = arg.as_str().ok_or("invalid nominal ABI type")?;
            if ids
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(n)))
            {
                json!({"kind":"id","name":n})
            } else if n == "ArithmeticError" {
                json!({"kind":"enum","name":n,"variants":["ArithmeticError.Overflow"]})
            } else if let Some(variants) = enums.get(n).and_then(Value::as_array) {
                json!({"kind":"enum","name":n,"variants":variants.iter().map(|v|v.as_str().map(|v|format!("{n}.{v}")).ok_or("invalid enum ABI variant")).collect::<Result<Vec<_>,_>>()?})
            } else if !seen.insert(n.into()) {
                json!({"kind":"ref","name":n})
            } else {
                let fields=records.get(n).and_then(Value::as_array).ok_or("unknown ABI record")?.iter().map(|f| {
                    let name=f[0].as_str().ok_or("invalid ABI field")?;
                    Ok(json!({"name":name,"type":describe_json(&f[1],records,enums,ids,seen,depth+1)?}))
                }).collect::<Result<Vec<Value>,String>>()?;
                seen.remove(n);
                json!({"kind":"record","name":n,"fields":fields})
            }
        }
        _ => return Err(format!("unsupported ABI constructor {kind}")),
    })
}

pub fn packed_width(t: &Value) -> Result<usize, String> {
    let inner = if t["kind"] == "list" {
        &t["element"]
    } else {
        t
    };
    validate_flat(inner, 0)?;
    let n = width(inner, 0)?;
    if n == 0 {
        return Err("packed width must be nonzero".into());
    }
    Ok(n)
}
fn width(t: &Value, depth: usize) -> Result<usize, String> {
    if depth > MAX_TYPE_DEPTH {
        return Err("packed value type depth exceeded".into());
    }
    match t["kind"].as_str().ok_or("missing ABI kind")? {
        "list" => width(&t["element"], depth + 1),
        "vector" => width(&t["element"], depth + 1)?
            .checked_mul(
                usize::try_from(t["size"].as_u64().ok_or("invalid vector size")?)
                    .map_err(|_| "vector size overflow")?,
            )
            .ok_or("packed width overflow".into()),
        "record" => t["fields"]
            .as_array()
            .ok_or("missing ABI fields")?
            .iter()
            .try_fold(0usize, |n, f| {
                n.checked_add(width(&f["type"], depth + 1)?)
                    .ok_or("packed width overflow".into())
            }),
        "u64" | "f64" => Ok(2),
        "u32" | "i32" | "f32" | "bool" => Ok(1),
        _ => {
            Err("packed compute values must be flat numeric, Bool, vector or record values".into())
        }
    }
}

fn validate_flat(t: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_TYPE_DEPTH {
        return Err("packed value type depth exceeded".into());
    }
    match t["kind"].as_str() {
        Some("u32" | "i32" | "f32" | "bool") => Ok(()),
        Some("u64" | "f64") => Ok(()),
        Some("vector") => {
            if t["size"].as_u64().is_none_or(|n| n == 0) {
                return Err("invalid vector size".into());
            }
            validate_flat(&t["element"], depth + 1)
        }
        Some("record") => {
            let fs = t["fields"].as_array().ok_or("missing ABI fields")?;
            if fs.is_empty() {
                return Err("packed record must have fields".into());
            }
            let mut names = BTreeSet::new();
            for f in fs {
                let name = f["name"].as_str().ok_or("missing field name")?;
                if !names.insert(name) {
                    return Err("duplicate ABI field".into());
                }
                validate_flat(&f["type"], depth + 1)?;
            }
            Ok(())
        }
        _ => Err("packed compute values must be flat numeric components".into()),
    }
}
pub fn flat_width(t: &Value) -> Result<usize, String> {
    validate_flat(t, 0)?;
    packed_width(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_layout_and_nominal_recursion() {
        let records = json!({"R":[["x","U64"],["v",{"Vector":["F32",3]}]]});
        let d = describe(&json!({"Named":"R"}), &records, &json!({}), &json!([])).unwrap();
        assert_eq!(flat_width(&d).unwrap(), 5);
        assert_eq!(packed_width(&d).unwrap(), 5);
        let recursive = json!({"Node":[["next",{"Option":{"Named":"Node"}}]]});
        let d = describe(&json!({"Named":"Node"}), &recursive, &json!({}), &json!([])).unwrap();
        assert_eq!(d["fields"][0]["type"]["element"]["kind"], "ref");
        assert!(packed_width(&d).is_err());
    }
    #[test]
    fn typed_and_json_declarations_describe_alike() {
        // Typed declarations are serialized per call; JSON ones are borrowed.
        let records = std::collections::BTreeMap::from([
            ("R".to_string(), vec![("x", json!("U64")), ("next", json!({"Option":{"Named":"R"}})), ("k", json!({"Named":"K"}))]),
        ]);
        let enums = std::collections::BTreeMap::from([("K".to_string(), vec!["A", "B"])]);
        let ids = vec!["Id"];
        let json = |v: Value| v;
        for ty in [json!({"Named":"R"}), json!({"List":{"Named":"K"}}), json!({"Named":"Id"}), json!("F32"), json!({"Named":"Absent"})] {
            assert_eq!(
                describe(&ty, &records, &enums, &ids),
                describe(&ty, &json(serde_json::to_value(&records).unwrap()), &json(serde_json::to_value(&enums).unwrap()), &json(serde_json::to_value(&ids).unwrap())),
            );
        }
    }
    #[test]
    fn overflow_and_unsupported_carriers_reject() {
        assert!(
            packed_width(&json!({"kind":"vector","size":u64::MAX,"element":{"kind":"u64"}}))
                .is_err()
        );
        assert!(packed_width(&json!({"kind":"ref","name":"Node"})).is_err());
    }
}

pub mod packed;

mod joint_domain;
pub mod objective;
pub mod selection;
pub const SELECTION_WASM: &[u8] = include_bytes!("../selection.wasm");
pub const SELECTION_JS_SOURCE: &str = include_str!("selection.mjs");

mod wire_tags;
pub use wire_tags::InkValueKind as ValueKind;
pub const VALUE_KIND_RUST_SOURCE: &str = include_str!("wire_tags.rs");

pub mod json_value;
pub mod value_wire;
pub const VALUE_WIRE_JS_SOURCE: &str = include_str!("value-wire.mjs");
pub const JSON_VALUE_JS_SOURCE: &str = include_str!("json-value.mjs");
pub const VALUE_WIRE_RUST_SOURCE: &str = include_str!("value_wire.rs");

pub mod wire_contract;

pub mod typed_json;

mod json_stream;
pub mod wire_packed;

pub mod adaptive;

pub mod json_lookup;
pub mod usage;
#[cfg(test)]
mod scalar_wire_fuzz {
    // The fixed-width scalar list fast path in value_wire::walk (taken when
    // nothing is materialised) must accept and reject exactly the bytes the
    // general element walk does, with the same error.
    use crate::value_wire::{walk, Limits, Schema};
    use serde_json::{json, Value};
    #[test]
    fn scalar_lists_validate_like_the_element_walk() {
        let mut seed = 0x2545f4914f6cdd1du64;
        let mut next = move || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let kinds = ["u32", "i32", "f32", "u64", "f64", "bool", "unit"];
        let value = |kind: &str, k: u64| -> Value {
            match kind {
                "u32" => json!(k as u32), "i32" => json!(k as i32), "u64" => json!(k),
                "f32" => json!({"F32Bits": k as u32}), "f64" => json!({"F64Bits": k.to_string()}),
                "bool" => json!(k % 2 == 0), _ => Value::Null,
            }
        };
        let mut checked = 0;
        for round in 0..3000 {
            let kind = kinds[round % kinds.len()];
            let vector = round % 5 == 0;
            let n = (next() % 6) as usize + usize::from(vector);
            let element = json!({"kind": kind});
            let list = if vector { json!({"kind":"vector","element":element,"size":n}) } else { json!({"kind":"list","element":element}) };
            let root = json!({"kind":"record","name":"R","fields":[{"name":"xs","type":list},{"name":"tail","type":{"kind":"u32"}}]});
            let schema = Schema::new(root, Default::default()).unwrap();
            let xs: Vec<Value> = (0..n).map(|_| value(kind, next())).collect();
            let mut bytes = schema.encode(&json!({"xs": xs, "tail": 7}), Limits::default()).unwrap();
            // Corrupt one header, length, value or the tail, or truncate/extend.
            match next() % 6 {
                0 => {}
                1 => { let i = 40 + (next() as usize) % (bytes.len() - 40); bytes[i] ^= 1 << (next() % 8); }
                2 => { let i = 40 + (next() as usize) % (bytes.len() - 40); bytes[i] = (next() % 9) as u8; }
                3 => { let k = (next() as usize) % (bytes.len() - 40); bytes.truncate(40 + k); }
                4 => bytes.extend([0, 1, 2, 3]),
                _ => { let i = 40 + (next() as usize) % (bytes.len() - 40); bytes[i] = 2; }
            }
            let limits = Limits { depth: [128, 1, 2][(next() % 3) as usize], nodes: [1_000_000, 3, 8][(next() % 3) as usize], ..Limits::default() };
            let run = |materialize: bool| {
                let mut pos = 40;
                let result = walk(&schema, schema.root(), &bytes, &mut pos, &mut limits.nodes.clone(), 0, limits, materialize);
                result.map(|_| pos)
            };
            if bytes.len() < 40 { continue; }
            assert_eq!(run(false), run(true), "{kind} n={n} vector={vector} {bytes:?}");
            checked += 1;
        }
        assert!(checked > 2500);
    }
    #[test]
    fn bulk_scalar_frames_match_one_frame_per_value() {
        let schema = Schema::new(json!({"kind":"list","element":{"kind":"u64"}}), Default::default()).unwrap();
        for n in 0..40usize {
            for (bytes, nodes, depth) in [(16 * 1024 * 1024, 1_000_000, 128), (44 + 8 + 12 * 5, 1_000_000, 128), (44 + 8 + 12 * 5 + 3, 1_000_000, 128),
                (1 << 20, 6, 128), (1 << 20, 1_000_000, 0), (44 + 8 + 12 * 4, 5, 128), (44 + 8 + 12 * 5, 5, 128)] {
                let limits = Limits { bytes, nodes, depth };
                let values: Vec<u64> = (0..n as u64).map(|i| i.wrapping_mul(0x9e3779b97f4a7c15)).collect();
                let frames = schema.encode_with(limits, |w| w.frame(|w| { w.word(n as u32)?; for v in &values { w.frame(|w| w.bytes(&v.to_le_bytes()))?; } Ok(()) }));
                let bulk = schema.encode_with(limits, |w| w.frame(|w| { w.word(n as u32)?; w.scalars(values.iter().map(|v| v.to_le_bytes())) }));
                assert_eq!(frames, bulk, "n={n} limits={limits:?}");
            }
        }
    }
}

