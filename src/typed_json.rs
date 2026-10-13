//! Schema-directed standard JSON boundary. Conversion is fallible; exact words
//! and Int never pass through floating-point arithmetic. Unknown fields reject.
use crate::{
    json_value::{parse, Json},
    value_wire::{Limits, Schema},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn integer(token: &str, limit: usize) -> Result<String, String> {
    if !crate::json_value::number_token(token) {
        return Err("invalid JSON number".into());
    }
    let negative = token.starts_with('-');
    let token = token.strip_prefix('-').unwrap_or(token);
    let (mantissa, exp) = token.split_once(['e', 'E']).unwrap_or((token, "0"));
    let mut digits = mantissa.replace('.', "");
    if digits.bytes().all(|b| b == b'0') {
        return Ok("0".into());
    }
    let exponent = exp
        .parse::<i64>()
        .map_err(|_| "JSON integer exponent limit")?;
    let fraction = mantissa.split_once('.').map_or(0, |(_, s)| s.len());
    let shift = exponent
        .checked_sub(fraction as i64)
        .ok_or("JSON integer exponent limit")?;
    if shift < 0 {
        let cut =
            usize::try_from(shift.unsigned_abs()).map_err(|_| "JSON integer exponent limit")?;
        if cut >= digits.len() || !digits[digits.len() - cut..].bytes().all(|b| b == b'0') {
            return Err("JSON number is not an integer".into());
        }
        digits.truncate(digits.len() - cut);
    } else {
        let shift = usize::try_from(shift).map_err(|_| "JSON integer exponent limit")?;
        if digits.len().checked_add(shift).is_none_or(|n| n > limit) {
            return Err("JSON integer magnitude limit".into());
        }
        digits.extend(std::iter::repeat_n('0', shift));
    }
    let digits = digits.trim_start_matches('0');
    if digits.len() > limit {
        return Err("JSON integer magnitude limit".into());
    }
    Ok(format!("{}{digits}", if negative { "-" } else { "" }))
}
#[derive(Clone)]
pub struct TypedJsonCodec {
    schema: Schema,
}
impl TypedJsonCodec {
    pub fn new(schema: Schema) -> Self {
        Self { schema }
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn decode(&self, document: &Json, limits: Limits) -> Result<Value, String> {
        document.stringify(limits)?;
        fn read(
            s: &Schema,
            t: &Value,
            j: &Json,
            budget: &mut usize,
            d: usize,
            l: Limits,
        ) -> Result<Value, String> {
            if d > l.depth || *budget == 0 {
                return Err("typed JSON resource limit".into());
            }
            *budget -= 1;
            let t = s.resolve(t)?;
            let error = || format!("JSON does not match {}", t["kind"]);
            Ok(match (t["kind"].as_str().unwrap(), j) {
                ("unit", Json::Null) => Value::Null,
                ("bool", Json::Boolean(v)) => json!(v),
                ("u32", Json::Number(v)) => {
                    json!(integer(v, 10)?.parse::<u32>().map_err(|_| error())?)
                }
                ("u64", Json::Number(v)) => {
                    json!(integer(v, 20)?.parse::<u64>().map_err(|_| error())?)
                }
                ("i32", Json::Number(v)) => {
                    json!(integer(v, 10)?.parse::<i32>().map_err(|_| error())?)
                }
                ("int", Json::Number(v)) => json!({"Int":integer(v,4096)?}),
                ("f32", Json::Number(v)) => {
                    let f = v.parse::<f32>().map_err(|_| error())?;
                    if !f.is_finite() {
                        return Err("JSON f32 overflow".into());
                    }
                    json!({"F32Bits":f.to_bits()})
                }
                ("string" | "id" | "enum", Json::Text(v)) => json!(v),
                ("list" | "vector", Json::Array(xs)) => Value::Array(
                    xs.iter()
                        .map(|x| read(s, &t["element"], x, budget, d + 1, l))
                        .collect::<Result<_, _>>()?,
                ),
                ("record", Json::Object(xs)) => {
                    let values = xs
                        .iter()
                        .map(|(k, v)| (k.as_str(), v))
                        .collect::<BTreeMap<_, _>>();
                    let fs = t["fields"].as_array().unwrap();
                    if values.len() != xs.len() || values.len() != fs.len() {
                        return Err("unknown, missing or duplicate JSON record field".into());
                    }
                    let mut out = serde_json::Map::new();
                    for f in fs {
                        let key = f["name"].as_str().unwrap();
                        let j = values.get(key).ok_or("missing JSON record field")?;
                        out.insert(key.into(), read(s, &f["type"], j, budget, d + 1, l)?);
                    }
                    Value::Object(out)
                }
                ("option" | "result", Json::Object(xs)) => {
                    if xs.len() != 1 {
                        return Err(error());
                    }
                    let (k, v) = &xs[0];
                    match (t["kind"].as_str().unwrap(), k.as_str(), v) {
                        ("option", "None", Json::Null) => json!({"None":null}),
                        ("option", "Some", _) => {
                            json!({"Some":read(s,&t["element"],v,budget,d+1,l)?})
                        }
                        ("result", "Ok", _) => json!({"Ok":read(s,&t["ok"],v,budget,d+1,l)?}),
                        ("result", "Err", _) => json!({"Err":read(s,&t["error"],v,budget,d+1,l)?}),
                        _ => return Err(error()),
                    }
                }
                _ => return Err(error()),
            })
        }
        let value = read(
            &self.schema,
            self.schema.root(),
            document,
            &mut limits.nodes.clone(),
            0,
            limits,
        )?;
        self.schema.encode(&value, limits)?;
        Ok(value)
    }
    pub fn encode(&self, value: &Value, limits: Limits) -> Result<Json, String> {
        self.schema.encode(value, limits)?;
        fn write(
            s: &Schema,
            t: &Value,
            v: &Value,
            budget: &mut usize,
            d: usize,
            l: Limits,
        ) -> Result<Json, String> {
            if d > l.depth || *budget == 0 {
                return Err("typed JSON resource limit".into());
            }
            *budget -= 1;
            let t = s.resolve(t)?;
            Ok(match t["kind"].as_str().unwrap() {
                "unit" => Json::Null,
                "bool" => Json::Boolean(v.as_bool().unwrap()),
                "u32" | "i32" => Json::Number(v.to_string()),
                "u64" => Json::Number(
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string()),
                ),
                "int" => Json::Number(v["Int"].as_str().unwrap().into()),
                "f32" => {
                    let f = if let Some(n) = v.as_f64() {
                        n as f32
                    } else {
                        f32::from_bits(v["F32Bits"].as_u64().unwrap() as u32)
                    };
                    if !f.is_finite() {
                        return Err("non-finite f32 cannot encode as JSON".into());
                    }
                    Json::Number(if f == 0.0 && f.is_sign_negative() {
                        "-0".into()
                    } else {
                        f.to_string()
                    })
                }
                "string" | "enum" | "id" => Json::Text(v.as_str().unwrap().into()),
                "list" | "vector" => Json::Array(
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|x| write(s, &t["element"], x, budget, d + 1, l))
                        .collect::<Result<_, _>>()?,
                ),
                "record" => Json::Object(
                    t["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|f| {
                            let key = f["name"].as_str().unwrap();
                            Ok((key.into(), write(s, &f["type"], &v[key], budget, d + 1, l)?))
                        })
                        .collect::<Result<_, String>>()?,
                ),
                "option" => {
                    if let Some(x) = v.get("Some") {
                        Json::Object(vec![(
                            "Some".into(),
                            write(s, &t["element"], x, budget, d + 1, l)?,
                        )])
                    } else {
                        Json::Object(vec![("None".into(), Json::Null)])
                    }
                }
                "result" => {
                    if let Some(x) = v.get("Ok") {
                        Json::Object(vec![(
                            "Ok".into(),
                            write(s, &t["ok"], x, budget, d + 1, l)?,
                        )])
                    } else {
                        Json::Object(vec![(
                            "Err".into(),
                            write(s, &t["error"], &v["Err"], budget, d + 1, l)?,
                        )])
                    }
                }
                _ => return Err("unsupported typed JSON descriptor".into()),
            })
        }
        write(
            &self.schema,
            self.schema.root(),
            value,
            &mut limits.nodes.clone(),
            0,
            limits,
        )
    }
    pub fn parse(&self, text: &str, limits: Limits) -> Result<Value, String> {
        self.decode(&parse(text, limits)?, limits)
    }
    pub fn stringify(&self, value: &Value, limits: Limits) -> Result<String, String> {
        self.encode(value, limits)?.stringify(limits)
    }
    pub fn parse_binary(&self, text: &str, limits: Limits) -> Result<Vec<u8>, String> {
        self.schema.encode(&self.parse(text, limits)?, limits)
    }
    pub fn stringify_binary(&self, bytes: &[u8], limits: Limits) -> Result<String, String> {
        self.stringify(&self.schema.decode(bytes, limits)?, limits)
    }
}
