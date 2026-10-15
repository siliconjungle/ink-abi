//! Direct borrowed-wire bridge to the existing flat numeric compute ABI.
use crate::value_wire::Validated;
fn word(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("packed scalar width")?
            .try_into()
            .unwrap(),
    ))
}
/// Bridge directly to the existing flat packed compute ABI. Raw f32 bits
/// survive packing; finite is a separate GPU eligibility observation.
pub fn pack_flat(value: Validated<'_>) -> Result<crate::packed::Data, String> {
    let t = value.descriptor()?;
    let stride = crate::packed_width(t)?;
    let count = if t["kind"] == "list" { value.len()? } else { 1 };
    let word_count = count.checked_mul(stride).ok_or("packed size overflow")?;
    if word_count
        .checked_mul(4)
        .is_none_or(|n| n > value.limits().bytes)
    {
        return Err("packed wire byte limit".into());
    }
    fn push(v: Validated<'_>, words: &mut Vec<u32>, finite: &mut bool) -> Result<(), String> {
        match v.kind()? {
            "record" => {
                for f in v.fields()? {
                    push(f?, words, finite)?;
                }
            }
            "list" | "vector" => {
                for x in v.items()? {
                    push(x?, words, finite)?;
                }
            }
            "bool" => words.push(u32::from(v.body()?[0])),
            "u64" | "f64" => {
                let body = v.body()?;
                if v.kind()?=="f64"{*finite &= f64::from_le_bytes(body.try_into().map_err(|_|"binary64 wire width")?).is_finite();}
                words.push(word(body, 0)?);
                words.push(word(body, 4)?);
            }
            "u32" | "i32" | "f32" => {
                let bits = word(v.body()?, 0)?;
                if v.kind()? == "f32" {
                    *finite &= f32::from_bits(bits).is_finite();
                }
                words.push(bits);
            }
            _ => return Err("unsupported packed wire value".into()),
        }
        Ok(())
    }
    let mut words = Vec::with_capacity(word_count);
    let mut finite = true;
    push(value, &mut words, &mut finite)?;
    Ok(crate::packed::Data {
        words: std::sync::Arc::new(words),
        count,
        finite,
    })
}
