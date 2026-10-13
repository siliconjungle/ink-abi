use crate::value_wire::{Schema, FORMAT};
use serde_json::{json, Value};
use std::collections::BTreeMap;
/// Build portable signatures from a serialized checked source program. Generic
/// descriptors are shared across all backends; temporary owners stay internal.
pub fn source_contract<P: serde::Serialize>(program: &P) -> Result<Value, String> {
    let p = serde_json::to_value(program).map_err(|e| e.to_string())?;
    let describe = |t: &Value| crate::describe(t, &p["records"], &p["enums"], &p["ids"]);
    let named = p["records"]
        .as_object()
        .ok_or("wire program records")?
        .keys()
        .map(|n| Ok((n.clone(), describe(&json!({"Named":n}))?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut signatures = serde_json::Map::new();
    for f in p["functions"].as_array().ok_or("wire functions")? {
        let name = f["name"].as_str().ok_or("wire function name")?;
        let fields = f["params"]
            .as_array()
            .ok_or("wire parameters")?
            .iter()
            .map(|param| Ok(json!({"name":param[0],"type":describe(&param[1])?})))
            .collect::<Result<Vec<_>, String>>()?;
        let args = Schema::new(
            json!({"kind":"record","name":"arguments","fields":fields}),
            named.clone(),
        );
        let result = Schema::new(describe(&f["result"])?, named.clone());
        let entry = match (args, result) {
            (Ok(a), Ok(r)) => json!({"arguments":a.descriptor(),"result":r.descriptor()}),
            (a, r) => json!({"unsupported":a.err().or_else(||r.err()).unwrap()}),
        };
        signatures.insert(name.into(), entry);
    }
    Ok(json!({"format":FORMAT,"functions":signatures}))
}
