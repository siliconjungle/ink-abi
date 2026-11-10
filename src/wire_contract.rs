use crate::value_wire::{Schema, FORMAT};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
/// The named descriptors a schema rooted at `root` can reach. `Schema::new`
/// keeps exactly the references its check visits, and that check follows a
/// subset of these edges, so handing it this closure instead of every record
/// in the program yields the same schema or the same error.
fn reachable(root: &Value, named: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    fn visit<'a>(t: &'a Value, named: &'a BTreeMap<String, Value>, found: &mut BTreeSet<&'a str>) {
        let Some(object) = t.as_object() else { return };
        if object.get("kind").and_then(Value::as_str) == Some("ref") {
            if let Some((name, target)) = object
                .get("name")
                .and_then(Value::as_str)
                .and_then(|n| named.get_key_value(n))
            {
                if found.insert(name) {
                    visit(target, named, found);
                }
            }
        }
        for key in ["element", "ok", "error"] {
            if let Some(inner) = object.get(key) {
                visit(inner, named, found);
            }
        }
        for field in object.get("fields").and_then(Value::as_array).into_iter().flatten() {
            if let Some(inner) = field.get("type") {
                visit(inner, named, found);
            }
        }
    }
    let mut found = BTreeSet::new();
    visit(root, named, &mut found);
    found
        .into_iter()
        .map(|name| (name.to_owned(), named[name].clone()))
        .collect()
}
/// Build portable signatures from a serialized checked source program. Generic
/// descriptors are shared across all backends; temporary owners stay internal.
pub fn source_contract<P: serde::Serialize>(program: &P) -> Result<Value, String> {
    let p = serde_json::to_value(program).map_err(|e| e.to_string())?;
    let describe = |t: &Value| {
        crate::describe_json(t, &p["records"], &p["enums"], &p["ids"], &mut BTreeSet::new(), 0)
    };
    let named = p["records"]
        .as_object()
        .ok_or("wire program records")?
        .keys()
        .map(|n| Ok((n.clone(), describe(&json!({"Named":n}))?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let schema = |root: Value| {
        let named = reachable(&root, &named);
        Schema::new(root, named)
    };
    let mut signatures = serde_json::Map::new();
    for f in p["functions"].as_array().ok_or("wire functions")? {
        let name = f["name"].as_str().ok_or("wire function name")?;
        let fields = f["params"]
            .as_array()
            .ok_or("wire parameters")?
            .iter()
            .map(|param| Ok(json!({"name":param[0],"type":describe(&param[1])?})))
            .collect::<Result<Vec<_>, String>>()?;
        let args = schema(json!({"kind":"record","name":"arguments","fields":fields}));
        let result = schema(describe(&f["result"])?);
        let entry = match (args, result) {
            (Ok(a), Ok(r)) => json!({"arguments":a.descriptor(),"result":r.descriptor()}),
            (a, r) => json!({"unsupported":a.err().or_else(||r.err()).unwrap()}),
        };
        signatures.insert(name.into(), entry);
    }
    Ok(json!({"format":FORMAT,"functions":signatures}))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The contract as first built: every schema checked against the table of
    /// every record in the program, each description from the public entry.
    fn whole_table_contract(p: &Value) -> Result<Value, String> {
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
    fn program(functions: Value) -> Value {
        json!({
            "records": {
                "Node": [["value","U64"],["next",{"Option":{"Named":"Node"}}],["tag",{"Named":"Tag"}]],
                "Tag": [["name","String"],["weights",{"Vector":["F32",3]}],["key",{"Named":"Key"}]],
                "Pair": [["left",{"Named":"Node"}],["right",{"Result":[{"Named":"Tag"},{"Named":"Kind"}]}]],
                "Ring": [["peer",{"Named":"Loop"}]],
                "Loop": [["ring",{"List":{"Named":"Ring"}}],["pair",{"Named":"Pair"}]],
                "Scratch": [["cells",{"Buffer":"U32"}]],
                "Holder": [["scratch",{"Named":"Scratch"}],["node",{"Named":"Node"}]],
                "Unused": [["flag","Bool"]],
            },
            "enums": {"Kind":["A","B"]},
            "ids": ["Key"],
            "functions": functions,
        })
    }
    #[test]
    fn reachable_tables_give_the_whole_table_contract() {
        let p = program(json!([
            {"name":"scalar","params":[["x","F32"],["y","U32"]],"result":"F64"},
            {"name":"node","params":[["n",{"Named":"Node"}]],"result":{"Named":"Tag"}},
            {"name":"pairs","params":[["ps",{"List":{"Named":"Pair"}}],["k",{"Named":"Kind"}]],"result":{"Option":{"Named":"Pair"}}},
            {"name":"mutual","params":[["r",{"Named":"Ring"}]],"result":{"Named":"Loop"}},
            {"name":"owner","params":[["s",{"Named":"Scratch"}]],"result":"Unit"},
            {"name":"nested_owner","params":[["n",{"Named":"Node"}]],"result":{"Named":"Holder"}},
            {"name":"temporary","params":[["m",{"Map":["U32","Bool"]}]],"result":{"Named":"Key"}},
            {"name":"none","params":[],"result":{"Result":[{"Named":"Unused"},"String"]}},
        ]));
        let contract = source_contract(&p).unwrap();
        assert_eq!(contract, whole_table_contract(&p).unwrap());
        let functions = &contract["functions"];
        // A schema names only what it reaches, recursion included.
        let named = |f: &str, side: &str| {
            functions[f][side]["named"].as_object().unwrap().keys().cloned().collect::<Vec<_>>()
        };
        assert!(named("scalar", "arguments").is_empty());
        assert_eq!(named("node", "arguments"), ["Node"]);
        assert_eq!(named("mutual", "result"), ["Loop", "Node"]);
        assert!(functions["owner"]["unsupported"].is_string());
        assert!(functions["nested_owner"]["unsupported"].is_string());
        assert!(functions["temporary"]["unsupported"].is_string());
    }
    #[test]
    fn undeclared_types_fail_as_before() {
        let p = program(json!([
            {"name":"missing","params":[["x",{"Named":"Absent"}]],"result":"Unit"},
        ]));
        assert_eq!(source_contract(&p), whole_table_contract(&p));
        assert!(source_contract(&p).is_err());
    }
}
