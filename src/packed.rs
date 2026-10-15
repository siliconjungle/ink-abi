//! Shared native transport, used by compiled C/Wasm/GPU hosts.
use serde_json::{json, Value};
use std::sync::Arc;
type Result<T> = std::result::Result<T, String>;
const MAX_BYTES: usize = 256 * 1024 * 1024;
pub fn width(t: &Value) -> usize {
    crate::packed_width(t).expect("checked packed ABI descriptor")
}
#[derive(Clone)]
pub struct Data {
    pub words: Arc<Vec<u32>>,
    pub count: usize,
    pub finite: bool,
}
pub fn pack(v: &Value, t: &Value) -> Result<Data> {
    fn go(v: &Value, t: &Value, w: &mut Vec<u32>, finite: &mut bool) -> Result<()> {
        match t["kind"].as_str().unwrap() {
            "list" => {
                for x in v.as_array().ok_or("expected list")? {
                    go(x, &t["element"], w, finite)?;
                }
            }
            "vector" => {
                let a = v.as_array().ok_or("expected vector")?;
                if a.len() != t["size"].as_u64().unwrap() as usize {
                    return Err("vector dimension mismatch".into());
                }
                for x in a {
                    go(x, &t["element"], w, finite)?
                }
            }
            "record" => {
                let a = v.as_object().ok_or("expected record")?;
                let fields = t["fields"].as_array().unwrap();
                if a.len() != fields.len() {
                    return Err("record fields mismatch".into());
                }
                for f in fields {
                    go(
                        a.get(f["name"].as_str().unwrap())
                            .ok_or("missing record field")?,
                        &f["type"],
                        w,
                        finite,
                    )?
                }
            }
            "bool" => w.push(v.as_bool().ok_or("expected Bool")? as u32),
            "u32" => w.push(
                u32::try_from(v.as_u64().ok_or("expected u32")?)
                    .map_err(|_| "u32 outside range")?,
            ),
            "i32" => w.push(
                i32::try_from(v.as_i64().ok_or("expected i32")?).map_err(|_| "i32 outside range")?
                    as u32,
            ),
            "u64" => {
                let n = if let Some(s) = v.as_str() {
                    s.parse::<u64>().map_err(|_| "u64 outside range")?
                } else {
                    v.as_u64().ok_or("expected u64")?
                };
                w.push(n as u32);
                w.push((n >> 32) as u32);
            }
            "f64" => {
                let bits=crate::float64::bits(v)?;
                *finite &= f64::from_bits(bits).is_finite();
                w.push(bits as u32);w.push((bits>>32)as u32);
            }
            "f32" => {
                let bits = if let Some(n) = v.as_f64() {
                    let f = n as f32;
                    if !f.is_finite() {
                        return Err("f32 input overflow".into());
                    }
                    f.to_bits()
                } else {
                    if v.as_object().is_none_or(|o| o.len() != 1) {
                        return Err("expected finite f32 or explicit F32Bits".into());
                    }
                    u32::try_from(v["F32Bits"].as_u64().ok_or("invalid F32Bits")?)
                        .map_err(|_| "invalid F32Bits")?
                };
                *finite &= f32::from_bits(bits).is_finite();
                w.push(bits);
            }
            _ => return Err("invalid compiled type".into()),
        };
        if w.len() * 4 > MAX_BYTES {
            return Err("input exceeds 256 MiB budget".into());
        }
        Ok(())
    }
    let stride = crate::packed_width(t)?;
    let count = if t["kind"] == "list" {
        v.as_array().ok_or("expected list")?.len()
    } else {
        1
    };
    if count
        .checked_mul(stride)
        .and_then(|n| n.checked_mul(4))
        .is_none_or(|n| n > MAX_BYTES)
    {
        return Err("input exceeds 256 MiB budget".into());
    }
    let mut words = Vec::new();
    let mut finite = true;
    go(v, t, &mut words, &mut finite)?;
    let count = if t["kind"] == "list" {
        v.as_array().unwrap().len()
    } else {
        1
    };
    Ok(Data {
        words: Arc::new(words),
        count,
        finite,
    })
}
pub fn unpack(d: &Data, t: &Value) -> Value {
    fn go(w: &[u32], i: &mut usize, t: &Value) -> Value {
        match t["kind"].as_str().unwrap() {
            "list" => Value::Array(
                (0..w.len() / width(&t["element"]))
                    .map(|_| go(w, i, &t["element"]))
                    .collect(),
            ),
            "vector" => Value::Array(
                (0..t["size"].as_u64().unwrap())
                    .map(|_| go(w, i, &t["element"]))
                    .collect(),
            ),
            "record" => Value::Object(
                t["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (f["name"].as_str().unwrap().into(), go(w, i, &f["type"])))
                    .collect(),
            ),
            "u64" => {
                let n = w[*i] as u64 + ((w[*i + 1] as u64) << 32);
                *i += 2;
                if n > 9007199254740991 {
                    json!(n.to_string())
                } else {
                    json!(n)
                }
            }
            "f64" => {
                let bits=w[*i]as u64|((w[*i+1]as u64)<<32);*i+=2;
                crate::float64::value(bits)
            }
            k => {
                let n = w[*i];
                *i += 1;
                match k {
                    "u32" => json!(n),
                    "i32" => json!(n as i32),
                    "bool" => json!(n != 0),
                    "f32" => {
                        let f = f32::from_bits(n);
                        if f.is_finite() {
                            json!(f)
                        } else {
                            json!({"F32Bits":n})
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    go(&d.words, &mut 0, t)
}
