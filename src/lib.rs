//! Target-neutral value descriptors and transport contracts. No Ink compiler dependency.
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub const STATE_C_HEADER: &str = include_str!("state-abi.h");
pub const VALUES_C_HEADER: &str = include_str!("values.h");
pub const PACKED_JS_SOURCE: &str = include_str!("packed.mjs");
pub const VALUE_ABI: &str = "ink-values-v1";
pub const MAX_TYPE_DEPTH: usize = 32;

pub fn describe<T: Serialize, R: Serialize, E: Serialize, I: Serialize>(
    ty: &T,
    records: &R,
    enums: &E,
    ids: &I,
) -> Result<Value, String> {
    describe_json(
        &serde_json::to_value(ty).map_err(|e| e.to_string())?,
        &serde_json::to_value(records).map_err(|e| e.to_string())?,
        &serde_json::to_value(enums).map_err(|e| e.to_string())?,
        &serde_json::to_value(ids).map_err(|e| e.to_string())?,
        &mut BTreeSet::new(),
        0,
    )
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
    validate_flat(inner, false, 0)?;
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
        "u64" => Ok(2),
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
        Some("u64") => Ok(()),
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
        _ => {
            Err("packed compute values must be flat numeric components".into())
        }
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
        assert_eq!(flat_width(&d, false).unwrap(), 5);
        assert!(flat_width(&d, true).is_err());
        let recursive = json!({"Node":[["next",{"Option":{"Named":"Node"}}]]});
        let d = describe(&json!({"Named":"Node"}), &recursive, &json!({}), &json!([])).unwrap();
        assert_eq!(d["fields"][0]["type"]["element"]["kind"], "ref");
        assert!(packed_width(&d).is_err());
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

pub mod selection;
pub const SELECTION_WASM: &[u8] = include_bytes!("../selection.wasm");
pub const SELECTION_JS_SOURCE: &str = include_str!("selection.mjs");

mod wire_tags;
pub use wire_tags::InkValueKind as ValueKind;
pub const VALUE_KIND_RUST_SOURCE:&str=include_str!("wire_tags.rs");
