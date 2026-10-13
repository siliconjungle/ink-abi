//! JSON text adapters for the ordinary six-variant knowledge-library datatype.
//! Tokens retain exact numeric spelling. Duplicate object names are rejected.
use crate::value_wire::{Limits, Schema};
use serde_json::{json, Value};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    Null,
    Boolean(bool),
    Number(String),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}
pub fn number_token(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    if b.get(i) == Some(&b'0') {
        i += 1;
    } else {
        if !b.get(i).is_some_and(|b| (b'1'..=b'9').contains(b)) {
            return false;
        }
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        let at = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == at {
            return false;
        }
    }
    if b.get(i).is_some_and(|b| *b == b'e' || *b == b'E') {
        i += 1;
        if b.get(i).is_some_and(|b| *b == b'+' || *b == b'-') {
            i += 1;
        }
        let at = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == at {
            return false;
        }
    }
    i == b.len()
}
struct Parser<'a> {
    text: &'a str,
    at: usize,
    nodes: usize,
    limits: Limits,
}
impl Parser<'_> {
    fn ws(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| b" \t\r\n".contains(b))
        {
            self.at += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.text.as_bytes().get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.ws();
        let start = self.at;
        if !self.eat(b'"') {
            return Err("expected JSON string".into());
        }
        loop {
            match self.text.as_bytes().get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return serde_json::from_str(&self.text[start..self.at])
                        .map_err(|e| e.to_string());
                }
                Some(b'\\') => {
                    self.at += 2;
                }
                Some(_) => self.at += 1,
                None => return Err("unterminated JSON string".into()),
            }
            if self.at > self.text.len() {
                return Err("unterminated JSON escape".into());
            }
        }
    }
    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > self.limits.depth || self.nodes == 0 {
            return Err("JSON node or depth limit".into());
        }
        self.nodes -= 1;
        self.ws();
        Ok(match self.text.as_bytes().get(self.at) {
            Some(b'n') => {
                if !self.text[self.at..].starts_with("null") {
                    return Err("invalid JSON literal".into());
                }
                self.at += 4;
                Json::Null
            }
            Some(b't') => {
                if !self.text[self.at..].starts_with("true") {
                    return Err("invalid JSON literal".into());
                }
                self.at += 4;
                Json::Boolean(true)
            }
            Some(b'f') => {
                if !self.text[self.at..].starts_with("false") {
                    return Err("invalid JSON literal".into());
                }
                self.at += 5;
                Json::Boolean(false)
            }
            Some(b'"') => Json::Text(self.string()?),
            Some(b'[') => {
                self.at += 1;
                let mut xs = vec![];
                if !self.eat(b']') {
                    loop {
                        xs.push(self.value(depth + 1)?);
                        if self.eat(b']') {
                            break;
                        }
                        if !self.eat(b',') {
                            return Err("expected JSON array separator".into());
                        }
                    }
                }
                Json::Array(xs)
            }
            Some(b'{') => {
                self.at += 1;
                let mut xs = vec![];
                let mut seen = BTreeSet::new();
                if !self.eat(b'}') {
                    loop {
                        let key = self.string()?;
                        if !seen.insert(key.clone()) {
                            return Err("duplicate JSON object key".into());
                        }
                        if !self.eat(b':') {
                            return Err("expected JSON object colon".into());
                        }
                        xs.push((key, self.value(depth + 1)?));
                        if self.eat(b'}') {
                            break;
                        }
                        if !self.eat(b',') {
                            return Err("expected JSON object separator".into());
                        }
                    }
                }
                Json::Object(xs)
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self
                    .text
                    .as_bytes()
                    .get(self.at)
                    .is_some_and(|b| b"-+0123456789.eE".contains(b))
                {
                    self.at += 1;
                }
                let token = &self.text[start..self.at];
                if !number_token(token) {
                    return Err("invalid JSON number".into());
                }
                Json::Number(token.into())
            }
            _ => return Err("invalid JSON value".into()),
        })
    }
}
pub fn parse(text: &str, limits: Limits) -> Result<Json, String> {
    let limits = limits.checked()?;
    if text.len() > limits.bytes {
        return Err("JSON byte limit".into());
    }
    let mut p = Parser {
        text,
        at: 0,
        nodes: limits.nodes,
        limits,
    };
    let v = p.value(0)?;
    p.ws();
    if p.at != text.len() {
        return Err("trailing JSON text".into());
    }
    Ok(v)
}
impl Json {
    pub fn to_ink(&self) -> Value {
        let (index, fields) = match self {
            Self::Null => (0, json!({})),
            Self::Boolean(v) => (1, json!({"item0":v})),
            Self::Number(v) => (2, json!({"item0":v})),
            Self::Text(v) => (3, json!({"item0":v})),
            Self::Array(v) => (
                4,
                json!({"item0":v.iter().map(Self::to_ink).collect::<Vec<_>>()}),
            ),
            Self::Object(v) => (
                5,
                json!({"item0":v.iter().map(|(key,value)|json!({"key":key,"value":value.to_ink()})).collect::<Vec<_>>()}),
            ),
        };
        let mut payload = if index == 5 {
            fields
        } else {
            json!({"Ok":fields})
        };
        for _ in 0..index {
            payload = json!({"Err":payload});
        }
        json!({"payload":payload})
    }
    pub fn from_ink(v: &Value, limits: Limits) -> Result<Self, String> {
        let limits = limits.checked()?;
        fn decode(v: &Value, nodes: &mut usize, d: usize, l: Limits) -> Result<Json, String> {
            if *nodes == 0 || d > l.depth {
                return Err("JSON node or depth limit".into());
            }
            *nodes -= 1;
            if v.as_object().is_none_or(|o| o.len() != 1) {
                return Err("invalid JSON record".into());
            }
            let mut payload = v.get("payload").ok_or("missing JSON payload")?;
            let mut variant = 0;
            while variant < 5 && payload.get("Err").is_some() {
                if payload.as_object().is_none_or(|o| o.len() != 1) {
                    return Err("invalid JSON sum".into());
                }
                payload = &payload["Err"];
                variant += 1;
            }
            if variant < 5 {
                if payload.as_object().is_none_or(|o| o.len() != 1) {
                    return Err("invalid JSON sum".into());
                }
                payload = payload.get("Ok").ok_or("invalid JSON variant")?;
            }
            if payload
                .as_object()
                .is_none_or(|o| o.len() != usize::from(variant > 0))
                || variant > 0 && payload.get("item0").is_none()
            {
                return Err("invalid JSON payload fields".into());
            }
            let item = &payload["item0"];
            Ok(match variant {
                0 => Json::Null,
                1 => Json::Boolean(item.as_bool().ok_or("invalid JSON Boolean")?),
                2 => {
                    let token = item.as_str().ok_or("invalid JSON number token")?;
                    if !number_token(token) {
                        return Err("invalid JSON number token".into());
                    }
                    Json::Number(token.into())
                }
                3 => Json::Text(item.as_str().ok_or("invalid JSON String")?.into()),
                4 => {
                    let items = item.as_array().ok_or("invalid JSON Array")?;
                    if items.len() > *nodes {
                        return Err("JSON node limit".into());
                    }
                    Json::Array(
                        items
                            .iter()
                            .map(|v| decode(v, nodes, d + 1, l))
                            .collect::<Result<_, _>>()?,
                    )
                }
                _ => {
                    let mut seen = BTreeSet::new();
                    let items = item.as_array().ok_or("invalid JSON Object")?;
                    if items.len() > *nodes {
                        return Err("JSON node limit".into());
                    }
                    Json::Object(
                        items
                            .iter()
                            .map(|m| {
                                if m.as_object().is_none_or(|o| o.len() != 2)
                                    || m.get("value").is_none()
                                {
                                    return Err("invalid JSON member fields".into());
                                }
                                let key = m["key"].as_str().ok_or("invalid JSON member key")?;
                                if !seen.insert(key) {
                                    return Err("duplicate JSON object key".into());
                                }
                                Ok((key.into(), decode(&m["value"], nodes, d + 1, l)?))
                            })
                            .collect::<Result<_, String>>()?,
                    )
                }
            })
        }
        decode(v, &mut limits.nodes.clone(), 0, limits)
    }
    pub fn stringify(&self, limits: Limits) -> Result<String, String> {
        let limits = limits.checked()?;
        fn write(
            v: &Json,
            out: &mut String,
            nodes: &mut usize,
            d: usize,
            l: Limits,
        ) -> Result<(), String> {
            if *nodes == 0 || d > l.depth {
                return Err("JSON node or depth limit".into());
            }
            *nodes -= 1;
            match v {
                Json::Null => out.push_str("null"),
                Json::Boolean(v) => out.push_str(if *v { "true" } else { "false" }),
                Json::Number(v) => {
                    if !number_token(v) {
                        return Err("invalid JSON number token".into());
                    }
                    if v.len() > l.bytes.saturating_sub(out.len()) {
                        return Err("JSON byte limit".into());
                    }
                    out.push_str(v);
                }
                Json::Text(v) => {
                    if v.len() > l.bytes.saturating_sub(out.len()) {
                        return Err("JSON byte limit".into());
                    }
                    out.push_str(&serde_json::to_string(v).map_err(|e| e.to_string())?);
                }
                Json::Array(xs) => {
                    out.push('[');
                    for (i, x) in xs.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        write(x, out, nodes, d + 1, l)?;
                    }
                    out.push(']');
                }
                Json::Object(xs) => {
                    let mut seen = BTreeSet::new();
                    out.push('{');
                    for (i, (k, v)) in xs.iter().enumerate() {
                        if !seen.insert(k) {
                            return Err("duplicate JSON object key".into());
                        }
                        if i > 0 {
                            out.push(',');
                        }
                        if k.len() > l.bytes.saturating_sub(out.len()) {
                            return Err("JSON byte limit".into());
                        }
                        out.push_str(&serde_json::to_string(k).map_err(|e| e.to_string())?);
                        out.push(':');
                        write(v, out, nodes, d + 1, l)?;
                    }
                    out.push('}');
                }
            }
            if out.len() > l.bytes {
                return Err("JSON byte limit".into());
            }
            Ok(())
        }
        let mut out = String::new();
        write(self, &mut out, &mut limits.nodes.clone(), 0, limits)?;
        Ok(out)
    }
}
/// The adapter binds a declared ordinary datatype; its name grants no authority.
#[derive(Clone)]
pub struct JsonCodec {
    schema: Schema,
}
impl JsonCodec {
    pub fn new(schema: Schema) -> Result<Self, String> {
        let root = schema.root();
        let name = root["name"].as_str().ok_or("JSON schema root name")?;
        let fields = root["fields"].as_array().ok_or("JSON schema record")?;
        if root["kind"] != "record" || fields.len() != 1 || fields[0]["name"] != "payload" {
            return Err("JSON schema payload".into());
        }
        let mut sum = &fields[0]["type"];
        for index in 0..6 {
            let branch = if index < 5 {
                if sum["kind"] != "result" {
                    return Err("JSON schema sum".into());
                }
                &sum["ok"]
            } else {
                sum
            };
            let fs = branch["fields"].as_array().ok_or("JSON schema branch")?;
            if branch["kind"] != "record"
                || fs.len() != usize::from(index > 0)
                || index > 0 && fs[0]["name"] != "item0"
            {
                return Err("JSON schema branch fields".into());
            }
            if index > 0 {
                let t = &fs[0]["type"];
                match index {
                    1 => {
                        if t["kind"] != "bool" {
                            return Err("JSON schema Boolean".into());
                        }
                    }
                    2 | 3 => {
                        if t["kind"] != "string" {
                            return Err("JSON schema text".into());
                        }
                    }
                    4 => {
                        if t["kind"] != "list"
                            || t["element"]["kind"] != "ref"
                            || t["element"]["name"] != name
                        {
                            return Err("JSON schema array".into());
                        }
                    }
                    5 => {
                        let m = &t["element"];
                        let fs = m["fields"].as_array().ok_or("JSON member schema")?;
                        if t["kind"] != "list"
                            || m["kind"] != "record"
                            || fs.len() != 2
                            || fs[0]["name"] != "key"
                            || fs[0]["type"]["kind"] != "string"
                            || fs[1]["name"] != "value"
                            || fs[1]["type"]["kind"] != "ref"
                            || fs[1]["type"]["name"] != name
                        {
                            return Err("JSON member fields".into());
                        }
                    }
                    _ => {}
                }
            }
            if index < 5 {
                sum = &sum["error"];
            }
        }
        Ok(Self { schema })
    }
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn parse(&self, text: &str, limits: Limits) -> Result<Value, String> {
        let v = parse(text, limits)?.to_ink();
        self.schema.encode(&v, limits)?;
        Ok(v)
    }
    pub fn stringify(&self, value: &Value, limits: Limits) -> Result<String, String> {
        self.schema.encode(value, limits)?;
        Json::from_ink(value, limits)?.stringify(limits)
    }
    pub fn parse_binary(&self, text: &str, limits: Limits) -> Result<Vec<u8>, String> {
        self.schema.encode(&parse(text, limits)?.to_ink(), limits)
    }
    pub fn stringify_binary(&self, bytes: &[u8], limits: Limits) -> Result<String, String> {
        Json::from_ink(&self.schema.decode(bytes, limits)?, limits)?.stringify(limits)
    }
}
impl JsonCodec {
    /// Validate JSON's number grammar and unique member names directly over an
    /// already bounded typed wire tree. No JSON/Ink object tree is allocated.
    pub fn validate_binary<'a>(
        &'a self,
        bytes: &'a [u8],
        limits: Limits,
    ) -> Result<crate::value_wire::Validated<'a>, String> {
        fn check(
            v: crate::value_wire::Validated<'_>,
            depth: usize,
            limits: Limits,
        ) -> Result<(), String> {
            if depth > limits.depth {
                return Err("JSON depth limit".into());
            }
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
                return Ok(());
            }
            let item = p.field("item0")?.ok_or("JSON item")?;
            match index {
                2 => {
                    if !number_token(item.text()?) {
                        return Err("invalid JSON number token".into());
                    }
                }
                4 => {
                    for child in item.items()? {
                        check(child?, depth + 1, limits)?;
                    }
                }
                5 => {
                    let mut seen = BTreeSet::new();
                    for member in item.items()? {
                        let member = member?;
                        let key = member.field("key")?.ok_or("JSON key")?.text()?;
                        if !seen.insert(key) {
                            return Err("duplicate JSON object key".into());
                        }
                        check(
                            member.field("value")?.ok_or("JSON member value")?,
                            depth + 1,
                            limits,
                        )?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        let view = self.schema.validate(bytes, limits)?;
        check(view, 0, limits)?;
        Ok(view)
    }
}
