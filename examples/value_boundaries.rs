//! Whole-boundary timings; operational codec tests are run separately.
use ink_abi::{
    typed_json::TypedJsonCodec,
    value_wire::{Limits, Schema},
    wire_packed,
};
use serde_json::json;
use std::{collections::BTreeMap, hint::black_box, time::Instant};
fn samples(mut f: impl FnMut(), rounds: usize) -> Vec<String> {
    for _ in 0..3 {
        f();
    }
    (0..9)
        .map(|_| {
            let at = Instant::now();
            for _ in 0..rounds {
                f();
            }
            at.elapsed()
                .as_nanos()
                .checked_div(rounds as u128)
                .unwrap()
                .to_string()
        })
        .collect()
}
fn word(v: ink_abi::value_wire::Validated<'_>) -> u32 {
    u32::from_le_bytes(v.body().unwrap().try_into().unwrap())
}
fn main() {
    let l = Limits::default();
    let mut rows = vec![];
    for n in [32usize, 512, 4096] {
        let schema = Schema::new(
            json!({"kind":"list","element":{"kind":"u32"}}),
            BTreeMap::new(),
        )
        .unwrap();
        let codec = TypedJsonCodec::new(schema.clone());
        let text = serde_json::to_string(&(0..n as u32).collect::<Vec<_>>()).unwrap();
        let bytes = codec.parse_binary(&text, l).unwrap();
        let materialized = codec.parse(&text, l).unwrap();
        assert_eq!(bytes, schema.encode(&materialized, l).unwrap());
        let rounds = (10000 / n).max(8);
        for (label, timing) in [
            (
                "carrier-json-to-wire",
                samples(
                    || {
                        black_box(
                            schema
                                .encode(&codec.parse(black_box(&text), l).unwrap(), l)
                                .unwrap(),
                        );
                    },
                    rounds,
                ),
            ),
            (
                "stream-json-to-wire",
                samples(
                    || {
                        black_box(codec.parse_binary(black_box(&text), l).unwrap());
                    },
                    rounds,
                ),
            ),
            (
                "materialize-wire",
                samples(
                    || {
                        black_box(schema.decode(black_box(&bytes), l).unwrap());
                    },
                    rounds,
                ),
            ),
            (
                "borrow-wire",
                samples(
                    || {
                        black_box(schema.validate(black_box(&bytes), l).unwrap());
                    },
                    rounds,
                ),
            ),
            (
                "index-wire",
                samples(
                    || {
                        black_box(
                            schema
                                .validate(black_box(&bytes), l)
                                .unwrap()
                                .index()
                                .unwrap(),
                        );
                    },
                    rounds,
                ),
            ),
            (
                "pack-wire",
                samples(
                    || {
                        black_box(
                            wire_packed::pack_flat(schema.validate(black_box(&bytes), l).unwrap())
                                .unwrap(),
                        );
                    },
                    rounds,
                ),
            ),
        ] {
            rows.push(json!({"candidate":label,"rows":n,"phase":"construct","samples_ns":timing,"input_bytes":bytes.len()}));
        }
        for reads in [4usize, 64] {
            let positions = (0..reads)
                .map(|i| (i.wrapping_mul(2654435761usize) ^ 17) % n)
                .collect::<Vec<_>>();
            let expected = positions.iter().map(|&i| i as u64).sum::<u64>();
            let query_rounds = (30000 / (n * reads)).max(4);
            macro_rules! row {($label:expr,$body:expr)=>{rows.push(json!({"candidate":$label,"rows":n,"reads":reads,"phase":"lifecycle","samples_ns":samples(||{let got:u64=$body;assert_eq!(black_box(got),expected);},query_rounds),"input_bytes":bytes.len()}));};}
            row!("materialized", {
                let v = schema.decode(black_box(&bytes), l).unwrap();
                positions.iter().map(|&i| v[i].as_u64().unwrap()).sum()
            });
            row!("borrowed", {
                let v = schema.validate(black_box(&bytes), l).unwrap();
                positions
                    .iter()
                    .map(|&i| word(v.at(i).unwrap().unwrap()) as u64)
                    .sum()
            });
            row!("indexed", {
                let v = schema
                    .validate(black_box(&bytes), l)
                    .unwrap()
                    .index()
                    .unwrap();
                positions
                    .iter()
                    .map(|&i| word(v.at(i).unwrap().unwrap()) as u64)
                    .sum()
            });
            row!("packed", {
                let v =
                    wire_packed::pack_flat(schema.validate(black_box(&bytes), l).unwrap()).unwrap();
                positions.iter().map(|&i| v.words[i] as u64).sum()
            });
            let view = schema.validate(&bytes, l).unwrap();
            let indexed = view.index().unwrap();
            let packed = wire_packed::pack_flat(view).unwrap();
            let owned = schema.decode(&bytes, l).unwrap();
            for (label, timing, auxiliary_bytes) in [
                (
                    "materialized",
                    samples(
                        || {
                            black_box(
                                positions
                                    .iter()
                                    .map(|&i| owned[i].as_u64().unwrap())
                                    .sum::<u64>(),
                            );
                        },
                        1000,
                    ),
                    None,
                ),
                (
                    "borrowed",
                    samples(
                        || {
                            black_box(
                                positions
                                    .iter()
                                    .map(|&i| word(view.at(i).unwrap().unwrap()) as u64)
                                    .sum::<u64>(),
                            );
                        },
                        query_rounds,
                    ),
                    Some(0),
                ),
                (
                    "indexed",
                    samples(
                        || {
                            black_box(
                                positions
                                    .iter()
                                    .map(|&i| word(indexed.at(i).unwrap().unwrap()) as u64)
                                    .sum::<u64>(),
                            );
                        },
                        1000,
                    ),
                    Some(indexed.retained_bytes()),
                ),
                (
                    "packed",
                    samples(
                        || {
                            black_box(
                                positions
                                    .iter()
                                    .map(|&i| packed.words[i] as u64)
                                    .sum::<u64>(),
                            );
                        },
                        1000,
                    ),
                    Some(packed.words.capacity() * 4),
                ),
            ] {
                rows.push(json!({"candidate":label,"rows":n,"reads":reads,"phase":"read","samples_ns":timing,"auxiliary_bytes":auxiliary_bytes,"input_bytes":bytes.len()}));
            }
        }
    }
    println!(
        "{}",
        json!({"schema":1,"instrumented":false,"fixture":"List<u32>","measurements":rows,"scope":"local uninstrumented whole-boundary and whole-read-batch timings; no compilation/migration or peak-memory measurements"})
    );
}
