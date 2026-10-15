//! Exact binary64 carriers at JSON boundaries. Decimal bit strings avoid the
//! precision loss of transporting a u64 payload through a JavaScript Number.
use serde_json::{json,Value};
pub fn bits(value:&Value)->Result<u64,String>{
    crate::value_wire::f64_bits(value)
}
pub fn raw(bits:u64)->Value{crate::value_wire::f64_raw(bits)}
pub fn value(bits:u64)->Value{
    let n=f64::from_bits(bits);
    if n.is_finite(){json!(n)}else{raw(bits)}
}
