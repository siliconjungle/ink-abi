//! Saved convex support for profitability models. No search or measurements.
//! Integer halfspaces are evaluated exactly on the represented binary64 inputs.
use num_bigint::BigInt;
use num_traits::Zero;
use serde_json::{Map, Value};

fn integer(v: &Value) -> Result<BigInt, String> {
    let text = v.as_str().ok_or("joint-domain integer string required")?;
    if text.len() > 257 || text.trim_start_matches('-').len() > 256 {
        return Err("joint-domain integer budget".into());
    }
    let n = text.parse::<BigInt>().map_err(|_| "invalid joint-domain integer")?;
    if n.to_string() != text { return Err("noncanonical joint-domain integer".into()); }
    Ok(n)
}

pub fn validate(v: &Value, domains: &Map<String, Value>) -> Result<(), String> {
    let object = v.as_object().ok_or("joint-domain object required")?;
    if object.len() != 3 || !object.contains_key("features") || !object.contains_key("equalities") || !object.contains_key("inequalities") {
        return Err("joint-domain fields".into());
    }
    let names = v["features"].as_array().ok_or("joint-domain features required")?;
    if !(1..=5).contains(&names.len()) || names.len() != domains.len() {
        return Err("joint-domain feature budget".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        let name = name.as_str().ok_or("joint-domain feature name")?;
        if !domains.contains_key(name) || !seen.insert(name) { return Err("joint-domain feature schema".into()); }
    }
    let mut count = 0;
    for field in ["equalities", "inequalities"] {
        let rows = v[field].as_array().ok_or("joint-domain constraints required")?;
        count += rows.len();
        if count > 4096 { return Err("joint-domain constraint budget".into()); }
        for row in rows {
            let row = row.as_array().ok_or("joint-domain constraint row")?;
            if row.len() != names.len()+1 { return Err("joint-domain constraint width".into()); }
            let mut nonzero = false;
            for coefficient in row { nonzero |= !integer(coefficient)?.is_zero(); }
            if !nonzero { return Err("zero joint-domain constraint".into()); }
        }
    }
    if count == 0 { return Err("empty joint domain".into()); }
    Ok(())
}

// Exact nonnegative finite f64 = numerator / 2^shift. Model domains already
// bound every feature; keeping the exact dyadic value avoids epsilon widening.
fn dyadic(value: f64) -> (BigInt, usize) {
    if value == 0.0 { return (BigInt::zero(), 0); }
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52)-1);
    let (mantissa, power) = if exponent == 0 { (fraction, -1074) }
        else { (fraction | (1u64 << 52), exponent-1023-52) };
    if power >= 0 { (BigInt::from(mantissa) << power as usize, 0) }
    else { (BigInt::from(mantissa), (-power) as usize) }
}

/// Called only after validate and the per-feature domain checks.
pub fn contains(v: &Value, features: &Value) -> Result<bool, String> {
    let values = v["features"].as_array().ok_or("joint-domain features")?.iter().map(|name| {
        let value = features[name.as_str().ok_or("joint-domain feature")?].as_f64().ok_or("missing joint-domain feature")?;
        if !value.is_finite() || value < 0.0 { return Err("invalid joint-domain feature".into()); }
        Ok(dyadic(value))
    }).collect::<Result<Vec<_>, String>>()?;
    let shift = values.iter().map(|(_,shift)| *shift).max().unwrap_or(0);
    let values: Vec<BigInt> = values.into_iter().map(|(n,s)| n << (shift-s)).collect();
    for field in ["equalities", "inequalities"] {
        for row in v[field].as_array().ok_or("joint-domain constraints")? {
            let row = row.as_array().ok_or("joint-domain constraint row")?;
            let mut sum = integer(&row[0])? << shift;
            for (coefficient, value) in row[1..].iter().zip(&values) {
                sum += integer(coefficient)? * value;
            }
            if (field == "equalities" && !sum.is_zero()) || (field == "inequalities" && sum > BigInt::zero()) { return Ok(false); }
        }
    }
    Ok(true)
}
