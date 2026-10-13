//! Bounded, schema-bound portable values. Networking is deliberately external.
//! The descriptor and codecs are operationally trusted, not proof certificates.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub const FORMAT: &str = "ink-value-wire-v1";
pub const MAGIC: &[u8; 8] = b"INKVAL\0\x01";
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub nodes: usize,
    pub depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 16 * 1024 * 1024,
            nodes: 1_000_000,
            depth: 128,
        }
    }
}
impl Limits {
    pub fn checked(self) -> Result<Self, String> {
        let max = Self::default();
        if self.bytes > max.bytes || self.nodes > max.nodes || self.depth > max.depth {
            return Err("wire hard resource limit".into());
        }
        Ok(self)
    }
}
#[derive(Clone, Debug)]
pub struct Schema {
    root: Value,
    named: BTreeMap<String, Value>,
    identity: [u8; 32],
}
impl Schema {
    pub fn new(root: Value, named: BTreeMap<String, Value>) -> Result<Self, String> {
        fn check(
            t: &Value,
            named: &BTreeMap<String, Value>,
            used: &mut BTreeSet<String>,
            depth: usize,
            budget: &mut usize,
        ) -> Result<(), String> {
            if depth > 64 || *budget == 0 {
                return Err("wire descriptor limit".into());
            }
            *budget -= 1;
            let obj = t.as_object().ok_or("invalid wire descriptor")?;
            let kind = t["kind"].as_str().ok_or("missing wire kind")?;
            let keys: &[&str] = match kind {
                "u32" | "u64" | "i32" | "f32" | "bool" | "unit" | "string" | "int" => &["kind"],
                "id" | "ref" => &["kind", "name"],
                "enum" => &["kind", "name", "variants"],
                "record" => &["kind", "name", "fields"],
                "list" | "option" => &["kind", "element"],
                "vector" => &["kind", "element", "size"],
                "result" => &["kind", "ok", "error"],
                _ => return Err("unsupported wire descriptor".into()),
            };
            if obj.len() != keys.len() || keys.iter().any(|k| !obj.contains_key(*k)) {
                return Err("invalid wire descriptor fields".into());
            }
            match kind {
                "id" | "ref" | "enum" | "record" => {
                    t["name"].as_str().ok_or("missing descriptor name")?;
                }
                _ => {}
            }
            match kind {
                "ref" => {
                    let n = t["name"].as_str().unwrap();
                    if used.insert(n.into()) {
                        let d = named.get(n).ok_or("unresolved wire reference")?;
                        if d["kind"] != "record" || d["name"] != n {
                            return Err("invalid named wire reference".into());
                        }
                        check(d, named, used, depth + 1, budget)?;
                    }
                }
                "record" => {
                    let mut seen = BTreeSet::new();
                    for f in t["fields"].as_array().ok_or("missing descriptor fields")? {
                        if f.as_object().is_none_or(|o| o.len() != 2)
                            || !seen.insert(f["name"].as_str().ok_or("invalid field name")?)
                        {
                            return Err("invalid or duplicate wire field".into());
                        }
                        check(&f["type"], named, used, depth + 1, budget)?;
                    }
                }
                "enum" => {
                    let variants = t["variants"].as_array().ok_or("missing variants")?;
                    let mut seen = BTreeSet::new();
                    if variants.is_empty() || variants.len() > 64 {
                        return Err("wire enum limit".into());
                    }
                    for v in variants {
                        if !seen.insert(v.as_str().ok_or("invalid variant")?) {
                            return Err("duplicate variant".into());
                        }
                    }
                }
                "vector" => {
                    if t["size"].as_u64().is_none_or(|n| n == 0 || n > 64) {
                        return Err("invalid vector size".into());
                    }
                    check(&t["element"], named, used, depth + 1, budget)?;
                }
                "list" | "option" => check(&t["element"], named, used, depth + 1, budget)?,
                "result" => {
                    check(&t["ok"], named, used, depth + 1, budget)?;
                    check(&t["error"], named, used, depth + 1, budget)?;
                }
                _ => {}
            }
            Ok(())
        }
        let mut used = BTreeSet::new();
        check(&root, &named, &mut used, 0, &mut 100_000)?;
        let named = named
            .into_iter()
            .filter(|(k, _)| used.contains(k))
            .collect::<BTreeMap<_, _>>();
        let identity = Sha256::digest(
            serde_json::to_vec(&json!({"format":FORMAT,"root":root,"named":named}))
                .map_err(|e| e.to_string())?,
        )
        .into();
        Ok(Self {
            root,
            named,
            identity,
        })
    }
    pub fn root(&self) -> &Value {
        &self.root
    }
    pub fn named(&self) -> &BTreeMap<String, Value> {
        &self.named
    }
    pub fn identity(&self) -> String {
        self.identity.iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn descriptor(&self) -> Value {
        json!({"format":FORMAT,"root":self.root,"named":self.named,"identity":self.identity()})
    }
    pub fn from_descriptor(v: &Value) -> Result<Self, String> {
        if v["format"] != FORMAT || v.as_object().is_none_or(|o| o.len() != 4) {
            return Err("invalid wire schema envelope".into());
        }
        let s = Self::new(
            v["root"].clone(),
            serde_json::from_value(v["named"].clone()).map_err(|e| e.to_string())?,
        )?;
        if v["identity"] != s.identity() {
            return Err("wire schema identity mismatch".into());
        }
        Ok(s)
    }
    pub(crate) fn resolve<'a>(&'a self, mut t: &'a Value) -> Result<&'a Value, String> {
        for _ in 0..64 {
            if t["kind"] != "ref" {
                return Ok(t);
            }
            t = self
                .named
                .get(t["name"].as_str().ok_or("wire reference name")?)
                .ok_or("wire reference")?;
        }
        Err("wire reference cycle".into())
    }
    pub fn encode(&self, value: &Value, limits: Limits) -> Result<Vec<u8>, String> {
        let limits = limits.checked()?;
        if limits.bytes < 44 {
            return Err("wire byte limit".into());
        }
        let mut out = Vec::new();
        out.extend(MAGIC);
        out.extend(self.identity);
        encode(
            self,
            &self.root,
            value,
            &mut out,
            &mut limits.nodes.clone(),
            0,
            limits,
        )?;
        // Encoding validation is also a check on public numeric/string carriers.
        self.validate(&out, limits)?;
        Ok(out)
    }
    pub fn validate<'a>(
        &'a self,
        bytes: &'a [u8],
        limits: Limits,
    ) -> Result<Validated<'a>, String> {
        let limits = limits.checked()?;
        if bytes.len() > limits.bytes || bytes.len() < 44 {
            return Err("wire byte limit or truncated header".into());
        }
        if &bytes[..8] != MAGIC || bytes[8..40] != self.identity {
            return Err("wire format or schema mismatch".into());
        }
        let mut pos = 40;
        walk(
            self,
            &self.root,
            bytes,
            &mut pos,
            &mut limits.nodes.clone(),
            0,
            limits,
            false,
        )?;
        if pos != bytes.len() {
            return Err("trailing wire bytes".into());
        }
        Ok(Validated {
            schema: self,
            ty: &self.root,
            bytes: &bytes[40..],
            limits,
        })
    }
    pub fn decode(&self, bytes: &[u8], limits: Limits) -> Result<Value, String> {
        self.validate(bytes, limits)?.materialize()
    }
}
fn count(budget: &mut usize, depth: usize, limits: Limits) -> Result<(), String> {
    if *budget == 0 || depth > limits.depth {
        return Err("wire node or depth limit".into());
    }
    *budget -= 1;
    Ok(())
}
fn decimal(s: &str) -> bool {
    let b = s.strip_prefix('-').unwrap_or(s);
    !b.is_empty()
        && b.bytes().all(|b| b.is_ascii_digit())
        && (b == "0" || !b.starts_with('0'))
        && s != "-0"
}
fn tag<'a>(v: &'a Value, n: &str) -> Result<&'a Value, String> {
    if v.as_object().is_some_and(|o| o.len() == 1) {
        v.get(n).ok_or("invalid wire value tag".into())
    } else {
        Err("invalid wire value tag".into())
    }
}
fn encode(
    s: &Schema,
    t: &Value,
    v: &Value,
    out: &mut Vec<u8>,
    budget: &mut usize,
    d: usize,
    limits: Limits,
) -> Result<(), String> {
    count(budget, d, limits)?;
    let t = s.resolve(t)?;
    let at = out.len();
    out.extend([0; 4]);
    let error = || "invalid value for wire schema".to_string();
    match t["kind"].as_str().ok_or_else(error)? {
        "unit" => {
            if !v.is_null() {
                return Err(error());
            }
        }
        "bool" => out.push(u8::from(v.as_bool().ok_or_else(error)?)),
        "u32" => out.extend(
            u32::try_from(v.as_u64().ok_or_else(error)?)
                .map_err(|_| error())?
                .to_le_bytes(),
        ),
        "i32" => out.extend(
            i32::try_from(v.as_i64().ok_or_else(error)?)
                .map_err(|_| error())?
                .to_le_bytes(),
        ),
        "u64" => {
            let n = if let Some(n) = v.as_u64() {
                n
            } else {
                let text = v.as_str().ok_or_else(error)?;
                if !decimal(text) || text.starts_with('-') {
                    return Err(error());
                }
                text.parse::<u64>().map_err(|_| error())?
            };
            out.extend(n.to_le_bytes());
        }
        "f32" => {
            let bits = if let Some(n) = v.as_f64() {
                let f = n as f32;
                if !f.is_finite() {
                    return Err(error());
                }
                f.to_bits()
            } else {
                u32::try_from(tag(v, "F32Bits")?.as_u64().ok_or_else(error)?)
                    .map_err(|_| error())?
            };
            out.extend(bits.to_le_bytes());
        }
        "string" | "int" | "id" | "enum" => {
            let text = if t["kind"] == "int" {
                tag(v, "Int")?.as_str().ok_or_else(error)?
            } else {
                v.as_str().ok_or_else(error)?
            };
            if text.len() > limits.bytes.saturating_sub(out.len()) {
                return Err("wire byte limit".into());
            }
            if t["kind"] == "int" && !decimal(text) {
                return Err(error());
            }
            if t["kind"] == "enum" && !t["variants"].as_array().unwrap().contains(v) {
                return Err(error());
            }
            if t["kind"] == "id"
                && (text.len() != 32
                    || !text
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
            {
                return Err(error());
            }
            out.extend(text.as_bytes());
        }
        "list" | "vector" => {
            let xs = v.as_array().ok_or_else(error)?;
            if xs.len() > *budget
                || t["kind"] == "vector" && Some(xs.len() as u64) != t["size"].as_u64()
            {
                return Err(error());
            }
            out.extend(u32::try_from(xs.len()).map_err(|_| error())?.to_le_bytes());
            for x in xs {
                encode(s, &t["element"], x, out, budget, d + 1, limits)?;
            }
        }
        "record" => {
            let o = v.as_object().ok_or_else(error)?;
            let fs = t["fields"].as_array().unwrap();
            if o.len() != fs.len() {
                return Err(error());
            }
            for f in fs {
                encode(
                    s,
                    &f["type"],
                    o.get(f["name"].as_str().unwrap()).ok_or_else(error)?,
                    out,
                    budget,
                    d + 1,
                    limits,
                )?;
            }
        }
        "option" => {
            if let Ok(x) = tag(v, "Some") {
                out.push(1);
                encode(s, &t["element"], x, out, budget, d + 1, limits)?;
            } else {
                if !tag(v, "None")?.is_null() {
                    return Err(error());
                }
                out.push(0);
            }
        }
        "result" => {
            if let Ok(x) = tag(v, "Ok") {
                out.push(0);
                encode(s, &t["ok"], x, out, budget, d + 1, limits)?;
            } else {
                out.push(1);
                encode(s, &t["error"], tag(v, "Err")?, out, budget, d + 1, limits)?;
            }
        }
        _ => return Err(error()),
    }
    if out.len() > limits.bytes {
        return Err("wire byte limit".into());
    }
    let len = u32::try_from(out.len() - at - 4).map_err(|_| "wire frame overflow")?;
    out[at..at + 4].copy_from_slice(&len.to_le_bytes());
    Ok(())
}
fn u32_at(bytes: &[u8], pos: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(pos..pos.checked_add(4).ok_or("wire offset overflow")?)
            .ok_or("truncated wire length")?
            .try_into()
            .unwrap(),
    ))
}
fn frame(bytes: &[u8], pos: usize) -> Result<(usize, usize), String> {
    let start = pos.checked_add(4).ok_or("wire offset overflow")?;
    let end = start
        .checked_add(u32_at(bytes, pos)? as usize)
        .ok_or("wire length overflow")?;
    if end > bytes.len() {
        return Err("truncated wire frame".into());
    }
    Ok((start, end))
}
fn walk(
    s: &Schema,
    t: &Value,
    bytes: &[u8],
    pos: &mut usize,
    budget: &mut usize,
    d: usize,
    limits: Limits,
    materialize: bool,
) -> Result<Option<Value>, String> {
    count(budget, d, limits)?;
    let t = s.resolve(t)?;
    let (start, end) = frame(bytes, *pos)?;
    *pos = end;
    let body = &bytes[start..end];
    let mut p = 0;
    let error = || "malformed wire value".to_string();
    let value = match t["kind"].as_str().ok_or_else(error)? {
        "unit" => {
            if !body.is_empty() {
                return Err(error());
            }
            Value::Null
        }
        "bool" => {
            if body.len() != 1 || body[0] > 1 {
                return Err(error());
            }
            Value::Bool(body[0] == 1)
        }
        "u32" | "i32" | "f32" => {
            if body.len() != 4 {
                return Err(error());
            }
            let bits = u32_at(body, 0)?;
            match t["kind"].as_str().unwrap() {
                "u32" => json!(bits),
                "i32" => json!(bits as i32),
                _ => {
                    if materialize {
                        json!({"F32Bits":bits})
                    } else {
                        Value::Null
                    }
                }
            }
        }
        "u64" => {
            if body.len() != 8 {
                return Err(error());
            }
            json!(u64::from_le_bytes(body.try_into().unwrap()))
        }
        "string" | "int" | "id" | "enum" => {
            let text = std::str::from_utf8(body).map_err(|_| error())?;
            if t["kind"] == "int" && !decimal(text)
                || t["kind"] == "enum"
                    && !t["variants"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|v| v.as_str() == Some(text))
                || t["kind"] == "id"
                    && (text.len() != 32
                        || !text
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
            {
                return Err(error());
            }
            if materialize {
                if t["kind"] == "int" {
                    json!({"Int":text})
                } else {
                    json!(text)
                }
            } else {
                Value::Null
            }
        }
        "list" | "vector" => {
            let n = u32_at(body, 0)? as usize;
            p = 4;
            if n > *budget
                || n > body.len().saturating_sub(4) / 4
                || t["kind"] == "vector" && Some(n as u64) != t["size"].as_u64()
            {
                return Err(error());
            }
            let mut xs = vec![];
            for _ in 0..n {
                if let Some(x) = walk(
                    s,
                    &t["element"],
                    body,
                    &mut p,
                    budget,
                    d + 1,
                    limits,
                    materialize,
                )? {
                    xs.push(x);
                }
            }
            if p != body.len() {
                return Err(error());
            }
            Value::Array(xs)
        }
        "record" => {
            let mut fields = serde_json::Map::new();
            for f in t["fields"].as_array().unwrap() {
                if let Some(x) = walk(
                    s,
                    &f["type"],
                    body,
                    &mut p,
                    budget,
                    d + 1,
                    limits,
                    materialize,
                )? {
                    fields.insert(f["name"].as_str().unwrap().into(), x);
                }
            }
            if p != body.len() {
                return Err(error());
            }
            Value::Object(fields)
        }
        "option" | "result" => {
            let b = *body.first().ok_or_else(error)?;
            if b > 1 {
                return Err(error());
            }
            p = 1;
            if t["kind"] == "option" && b == 0 {
                if body.len() != 1 {
                    return Err(error());
                }
                if materialize {
                    json!({"None":null})
                } else {
                    Value::Null
                }
            } else {
                let (tag, child) = if t["kind"] == "option" {
                    ("Some", &t["element"])
                } else if b == 0 {
                    ("Ok", &t["ok"])
                } else {
                    ("Err", &t["error"])
                };
                let x = walk(s, child, body, &mut p, budget, d + 1, limits, materialize)?;
                if p != body.len() {
                    return Err(error());
                }
                if let Some(x) = x {
                    json!({tag:x})
                } else {
                    Value::Null
                }
            }
        }
        _ => return Err(error()),
    };
    Ok(materialize.then_some(value))
}
/// Borrowed access to already structurally validated bytes. Rust borrowing
/// prevents mutation while this validation witness exists. No JSON tree is built.
#[derive(Clone, Copy)]
pub struct Validated<'a> {
    schema: &'a Schema,
    ty: &'a Value,
    bytes: &'a [u8],
    limits: Limits,
}
impl<'a> Validated<'a> {
    pub fn payload(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn materialize(&self) -> Result<Value, String> {
        let mut p = 0;
        walk(
            self.schema,
            self.ty,
            self.bytes,
            &mut p,
            &mut self.limits.nodes.clone(),
            0,
            self.limits,
            true,
        )?
        .ok_or("wire decode".into())
    }
    pub fn field(&self, name: &str) -> Result<Option<Self>, String> {
        let t = self.schema.resolve(self.ty)?;
        if t["kind"] != "record" {
            return Err("wire view is not a record".into());
        }
        let (start, end) = frame(self.bytes, 0)?;
        let body = &self.bytes[start..end];
        let mut p = 0;
        for f in t["fields"].as_array().unwrap() {
            let (_, next) = frame(body, p)?;
            if f["name"] == name {
                return Ok(Some(Self {
                    schema: self.schema,
                    ty: &f["type"],
                    bytes: &body[p..next],
                    limits: self.limits,
                }));
            }
            p = next;
        }
        Ok(None)
    }
    pub fn at(&self, index: usize) -> Result<Option<Self>, String> {
        let t = self.schema.resolve(self.ty)?;
        if t["kind"] != "list" && t["kind"] != "vector" {
            return Err("wire view is not a sequence".into());
        }
        let (start, end) = frame(self.bytes, 0)?;
        let body = &self.bytes[start..end];
        let n = u32_at(body, 0)? as usize;
        if index >= n {
            return Ok(None);
        }
        let mut p = 4;
        for i in 0..n {
            let (_, next) = frame(body, p)?;
            if i == index {
                return Ok(Some(Self {
                    schema: self.schema,
                    ty: &t["element"],
                    bytes: &body[p..next],
                    limits: self.limits,
                }));
            }
            p = next;
        }
        Ok(None)
    }
}

impl<'a> Validated<'a> {
    pub fn text(&self) -> Result<&'a str, String> {
        let t = self.schema.resolve(self.ty)?;
        if !["string", "int", "id", "enum"].contains(&t["kind"].as_str().unwrap_or("")) {
            return Err("wire view is not text".into());
        }
        let (a, b) = frame(self.bytes, 0)?;
        std::str::from_utf8(&self.bytes[a..b]).map_err(|e| e.to_string())
    }
    pub fn len(&self) -> Result<usize, String> {
        let t = self.schema.resolve(self.ty)?;
        if t["kind"] != "list" && t["kind"] != "vector" {
            return Err("wire view is not a sequence".into());
        }
        let (a, _) = frame(self.bytes, 0)?;
        Ok(u32_at(self.bytes, a)? as usize)
    }
    pub fn is_empty(&self) -> Result<bool, String> {
        Ok(self.len()? == 0)
    }
    pub fn variant(&self) -> Result<(bool, Option<Self>), String> {
        let t = self.schema.resolve(self.ty)?;
        if t["kind"] != "option" && t["kind"] != "result" {
            return Err("wire view is not a sum".into());
        }
        let (a, b) = frame(self.bytes, 0)?;
        let tag = self.bytes[a] == 1;
        if t["kind"] == "option" && !tag {
            return Ok((false, None));
        }
        let ty = if t["kind"] == "option" {
            &t["element"]
        } else if tag {
            &t["error"]
        } else {
            &t["ok"]
        };
        Ok((
            tag,
            Some(Self {
                schema: self.schema,
                ty,
                bytes: &self.bytes[a + 1..b],
                limits: self.limits,
            }),
        ))
    }
}

/// A single forward traversal over a validated sequence, without repeated scans.
pub struct Items<'a> {
    schema: &'a Schema,
    ty: &'a Value,
    body: &'a [u8],
    position: usize,
    left: usize,
    limits: Limits,
}
impl<'a> Iterator for Items<'a> {
    type Item = Result<Validated<'a>, String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let at = self.position;
        Some(frame(self.body, at).map(|(_, end)| {
            self.position = end;
            Validated {
                schema: self.schema,
                ty: self.ty,
                bytes: &self.body[at..end],
                limits: self.limits,
            }
        }))
    }
}
impl<'a> Validated<'a> {
    pub fn items(&self) -> Result<Items<'a>, String> {
        let t = self.schema.resolve(self.ty)?;
        if t["kind"] != "list" && t["kind"] != "vector" {
            return Err("wire view is not a sequence".into());
        }
        let (a, b) = frame(self.bytes, 0)?;
        let body = &self.bytes[a..b];
        Ok(Items {
            schema: self.schema,
            ty: &t["element"],
            body,
            position: 4,
            left: u32_at(body, 0)? as usize,
            limits: self.limits,
        })
    }
}
