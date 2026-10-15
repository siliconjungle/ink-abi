//! Streaming text-to-wire adapters. No Json tree or serde_json value carrier.
use crate::{
    json_value::{Json, Parser},
    value_wire::{Encoder, Limits, Schema},
};
use serde_json::Value;
use std::collections::BTreeSet;

fn sequence(
    p: &mut Parser<'_>,
    w: &mut Encoder,
    open: u8,
    close: u8,
    mut item: impl FnMut(&mut Parser<'_>, &mut Encoder) -> Result<(), String>,
) -> Result<(), String> {
    if !p.eat(open) {
        return Err("JSON container shape".into());
    }
    let at = w.position();
    w.word(0)?;
    let mut count = 0u32;
    if !p.eat(close) {
        loop {
            item(p, w)?;
            count = count.checked_add(1).ok_or("JSON sequence count")?;
            if p.eat(close) {
                break;
            }
            if !p.eat(b',') {
                return Err("JSON container separator".into());
            }
        }
    }
    w.patch_word(at, count)
}
fn scalar(p: &mut Parser<'_>, d: usize) -> Result<Json, String> {
    if matches!(p.peek(), Some(b'[' | b'{')) {
        return Err("expected JSON scalar".into());
    }
    p.value(d)
}
fn json_variant(
    w: &mut Encoder,
    index: usize,
    level: usize,
    emit: &mut impl FnMut(&mut Encoder) -> Result<(), String>,
) -> Result<(), String> {
    if level < index {
        return w.frame(|w| {
            w.bytes(&[1])?;
            json_variant(w, index, level + 1, emit)
        });
    }
    if index < 5 {
        w.frame(|w| {
            w.bytes(&[0])?;
            w.frame(|w| emit(w))
        })
    } else {
        w.frame(|w| emit(w))
    }
}
fn json_node(p: &mut Parser<'_>, w: &mut Encoder, d: usize) -> Result<(), String> {
    let ch = p.peek();
    let index = match ch {
        Some(b'n') => 0,
        Some(b't' | b'f') => 1,
        Some(b'-' | b'0'..=b'9') => 2,
        Some(b'"') => 3,
        Some(b'[') => 4,
        Some(b'{') => 5,
        _ => return Err("invalid JSON value".into()),
    };
    // Only scalar parsing constructs a scalar token. Containers stream directly.
    let leaf = if index < 4 {
        Some(scalar(p, d)?)
    } else {
        p.node(d)?;
        None
    };
    w.frame(|w| {
        json_variant(w, index, 0, &mut |w| match &leaf {
            Some(Json::Null) => Ok(()),
            Some(Json::Boolean(b)) => w.frame(|w| w.bytes(&[u8::from(*b)])),
            Some(Json::Number(s) | Json::Text(s)) => w.frame(|w| w.bytes(s.as_bytes())),
            None if index == 4 => {
                w.frame(|w| sequence(p, w, b'[', b']', |p, w| json_node(p, w, d + 1)))
            }
            None => w.frame(|w| {
                let mut seen = BTreeSet::new();
                sequence(p, w, b'{', b'}', |p, w| {
                    let key = p.string()?;
                    if !seen.insert(key.clone()) {
                        return Err("duplicate JSON object key".into());
                    }
                    if !p.eat(b':') {
                        return Err("JSON object colon".into());
                    }
                    w.frame(|w| {
                        w.frame(|w| w.bytes(key.as_bytes()))?;
                        json_node(p, w, d + 1)
                    })
                })
            }),
            _ => Err("invalid JSON scalar".into()),
        })
    })
}
pub(crate) fn json_binary(schema: &Schema, text: &str, l: Limits) -> Result<Vec<u8>, String> {
    let mut p = Parser::new(text, l)?;
    schema.encode_with(l, |w| {
        json_node(&mut p, w, 0)?;
        p.finish()
    })
}
fn typed_node(
    s: &Schema,
    t: &Value,
    p: &mut Parser<'_>,
    w: &mut Encoder,
    d: usize,
) -> Result<(), String> {
    let t = s.resolve(t)?;
    w.frame(|w| {
        match t["kind"].as_str().unwrap() {
            "list" | "vector" => {
                p.node(d)?;
                sequence(p, w, b'[', b']', |p, w| {
                    typed_node(s, &t["element"], p, w, d + 1)
                })?;
                // The final schema validation checks fixed vector size.
            }
            "record" => {
                p.node(d)?;
                if !p.eat(b'{') {
                    return Err("expected JSON record".into());
                }
                let fs = t["fields"].as_array().unwrap();
                let start = w.position();
                let mut spans = vec![None; fs.len()];
                if !p.eat(b'}') {
                    loop {
                        let key = p.string()?;
                        let i = fs
                            .iter()
                            .position(|f| f["name"] == key)
                            .ok_or("unknown JSON record field")?;
                        if spans[i].is_some() {
                            return Err("duplicate JSON record field".into());
                        }
                        if !p.eat(b':') {
                            return Err("JSON object colon".into());
                        }
                        let a = w.position();
                        typed_node(s, &fs[i]["type"], p, w, d + 1)?;
                        spans[i] = Some((a, w.position()));
                        if p.eat(b'}') {
                            break;
                        }
                        if !p.eat(b',') {
                            return Err("JSON record separator".into());
                        }
                    }
                }
                let spans = spans
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or("missing JSON record field")?;
                w.reorder(start, &spans)?;
            }
            "option" | "result" => {
                p.node(d)?;
                if !p.eat(b'{') {
                    return Err("expected JSON tagged value".into());
                }
                let key = p.string()?;
                if !p.eat(b':') {
                    return Err("JSON tag colon".into());
                }
                match (t["kind"].as_str().unwrap(), key.as_str()) {
                    ("option", "None") => {
                        if scalar(p, d + 1)? != Json::Null {
                            return Err("JSON None payload".into());
                        }
                        w.bytes(&[0])?;
                    }
                    ("option", "Some") => {
                        w.bytes(&[1])?;
                        typed_node(s, &t["element"], p, w, d + 1)?;
                    }
                    ("result", "Ok") => {
                        w.bytes(&[0])?;
                        typed_node(s, &t["ok"], p, w, d + 1)?;
                    }
                    ("result", "Err") => {
                        w.bytes(&[1])?;
                        typed_node(s, &t["error"], p, w, d + 1)?;
                    }
                    _ => return Err("invalid JSON tag".into()),
                }
                if !p.eat(b'}') {
                    return Err("expected exactly one JSON tag".into());
                }
            }
            kind => match (kind, scalar(p, d)?) {
                ("unit", Json::Null) => {}
                ("bool", Json::Boolean(b)) => w.bytes(&[u8::from(b)])?,
                ("string" | "id" | "enum", Json::Text(v)) => w.bytes(v.as_bytes())?,
                ("u32" | "u64" | "i32" | "int", Json::Number(v)) => {
                    let token = crate::typed_json::integer(
                        &v,
                        if kind == "int" {
                            4096
                        } else if kind == "u64" {
                            20
                        } else {
                            10
                        },
                    )?;
                    match kind {
                        "u32" => w.bytes(
                            &token
                                .parse::<u32>()
                                .map_err(|_| "JSON u32 range")?
                                .to_le_bytes(),
                        )?,
                        "u64" => w.bytes(
                            &token
                                .parse::<u64>()
                                .map_err(|_| "JSON u64 range")?
                                .to_le_bytes(),
                        )?,
                        "i32" => w.bytes(
                            &token
                                .parse::<i32>()
                                .map_err(|_| "JSON i32 range")?
                                .to_le_bytes(),
                        )?,
                        _ => w.bytes(token.as_bytes())?,
                    }
                }
                ("f64", Json::Number(v)) => {
                    let n=v.parse::<f64>().map_err(|_|"JSON f64")?;
                    if !n.is_finite(){return Err("JSON f64 overflow".into());}
                    w.bytes(&n.to_bits().to_le_bytes())?;
                }
                ("f32", Json::Number(v)) => {
                    let f = v.parse::<f32>().map_err(|_| "JSON f32")?;
                    if !f.is_finite() {
                        return Err("JSON f32 overflow".into());
                    }
                    w.bytes(&f.to_bits().to_le_bytes())?;
                }
                _ => return Err("JSON does not match typed schema".into()),
            },
        }
        Ok(())
    })
}
pub(crate) fn typed_binary(schema: &Schema, text: &str, l: Limits) -> Result<Vec<u8>, String> {
    let mut p = Parser::new(text, l)?;
    schema.encode_with(l, |w| {
        typed_node(schema, schema.root(), &mut p, w, 0)?;
        p.finish()
    })
}

struct Text {
    out: String,
    limits: Limits,
    left: usize,
}
impl Text {
    fn emit(&mut self, s: &str) -> Result<(), String> {
        if s.len() > self.limits.bytes.saturating_sub(self.out.len()) {
            return Err("JSON byte limit".into());
        }
        self.out.push_str(s);
        Ok(())
    }
    fn string(&mut self, s: &str) -> Result<(), String> {
        self.emit(&serde_json::to_string(s).map_err(|e| e.to_string())?)
    }
    fn node(&mut self, d: usize) -> Result<(), String> {
        if d > self.limits.depth || self.left == 0 {
            return Err("JSON node or depth limit".into());
        }
        self.left -= 1;
        Ok(())
    }
}
fn json_write(v: crate::value_wire::Validated<'_>, out: &mut Text, d: usize) -> Result<(), String> {
    out.node(d)?;
    let mut p = v.field("payload")?.ok_or("JSON payload")?;
    let mut index = 0;
    while index < 5 {
        let (err, child) = p.variant()?;
        p = child.ok_or("JSON branch")?;
        if !err {
            break;
        }
        index += 1;
    }
    if index == 0 {
        return out.emit("null");
    }
    let item = p.field("item0")?.ok_or("JSON item")?;
    match index {
        1 => out.emit(if item.body()?[0] == 0 {
            "false"
        } else {
            "true"
        }),
        2 => {
            let token = item.text()?;
            if !crate::json_value::number_token(token) {
                return Err("invalid JSON number token".into());
            }
            out.emit(token)
        }
        3 => out.string(item.text()?),
        4 => {
            out.emit("[")?;
            for (i, x) in item.items()?.enumerate() {
                if i > 0 {
                    out.emit(",")?;
                }
                json_write(x?, out, d + 1)?;
            }
            out.emit("]")
        }
        _ => {
            out.emit("{")?;
            let mut seen = BTreeSet::new();
            for (i, m) in item.items()?.enumerate() {
                let m = m?;
                let key = m.field("key")?.ok_or("JSON member key")?.text()?;
                if !seen.insert(key) {
                    return Err("duplicate JSON object key".into());
                }
                if i > 0 {
                    out.emit(",")?;
                }
                out.string(key)?;
                out.emit(":")?;
                json_write(m.field("value")?.ok_or("JSON member value")?, out, d + 1)?;
            }
            out.emit("}")
        }
    }
}
pub(crate) fn json_stringify(s: &Schema, bytes: &[u8], limits: Limits) -> Result<String, String> {
    let limits = limits.checked()?;
    let view = s.validate(bytes, limits)?;
    let mut out = Text {
        out: String::new(),
        limits,
        left: limits.nodes,
    };
    json_write(view, &mut out, 0)?;
    Ok(out.out)
}
fn typed_write(
    v: crate::value_wire::Validated<'_>,
    out: &mut Text,
    d: usize,
) -> Result<(), String> {
    out.node(d)?;
    match v.kind()? {
        "unit" => out.emit("null"),
        "bool" => out.emit(if v.body()?[0] == 0 { "false" } else { "true" }),
        "u32" => out.emit(&u32::from_le_bytes(v.body()?.try_into().unwrap()).to_string()),
        "u64" => out.emit(&u64::from_le_bytes(v.body()?.try_into().unwrap()).to_string()),
        "i32" => out.emit(&i32::from_le_bytes(v.body()?.try_into().unwrap()).to_string()),
        "int" => out.emit(v.text()?),
        "f64" => {
            let n=f64::from_le_bytes(v.body()?.try_into().unwrap());
            if !n.is_finite(){return Err("non-finite f64 cannot encode as JSON".into());}
            out.emit(if n==0.0&&n.is_sign_negative(){"-0".into()}else{n.to_string()}.as_str())
        }
        "f32" => {
            let f = f32::from_le_bytes(v.body()?.try_into().unwrap());
            if !f.is_finite() {
                return Err("non-finite f32 cannot encode as JSON".into());
            }
            out.emit(
                if f == 0. && f.is_sign_negative() {
                    "-0".into()
                } else {
                    f.to_string()
                }
                .as_str(),
            )
        }
        "string" | "enum" | "id" => out.string(v.text()?),
        "record" => {
            out.emit("{")?;
            for (i, (field, child)) in v.descriptor()?["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(v.fields()?)
                .enumerate()
            {
                if i > 0 {
                    out.emit(",")?;
                }
                out.string(field["name"].as_str().unwrap())?;
                out.emit(":")?;
                typed_write(child?, out, d + 1)?;
            }
            out.emit("}")
        }
        "list" | "vector" => {
            out.emit("[")?;
            for (i, x) in v.items()?.enumerate() {
                if i > 0 {
                    out.emit(",")?;
                }
                typed_write(x?, out, d + 1)?;
            }
            out.emit("]")
        }
        "option" | "result" => {
            let (tag, child) = v.variant()?;
            let key = if v.kind()? == "option" {
                if tag {
                    "Some"
                } else {
                    "None"
                }
            } else if tag {
                "Err"
            } else {
                "Ok"
            };
            out.emit("{")?;
            out.string(key)?;
            out.emit(":")?;
            if let Some(x) = child {
                typed_write(x, out, d + 1)?;
            } else {
                out.node(d + 1)?;
                out.emit("null")?;
            }
            out.emit("}")
        }
        _ => Err("unsupported typed JSON descriptor".into()),
    }
}
pub(crate) fn typed_stringify(s: &Schema, bytes: &[u8], limits: Limits) -> Result<String, String> {
    let limits = limits.checked()?;
    let view = s.validate(bytes, limits)?;
    let mut out = Text {
        out: String::new(),
        limits,
        left: limits.nodes,
    };
    typed_write(view, &mut out, 0)?;
    Ok(out.out)
}
