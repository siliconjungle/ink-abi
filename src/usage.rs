//! Bounded usage features shared unchanged by native and Wasm hosts. Opaque tokens
//! are application-scoped IDs, never proof premises or raw keys/documents.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Read,
    Write,
    Scan,
    Invalidate,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub kind: Kind,
    #[serde(default)]
    pub key: Option<u64>,
    #[serde(default)]
    pub path: Option<u64>,
    #[serde(default)]
    pub shape: Option<u64>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub scanned: Option<u64>,
    #[serde(default)]
    pub touched: Option<u64>,
    #[serde(default)]
    pub invalidated: Option<u64>,
    #[serde(default)]
    pub failed: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub samples: u64,
    pub failed: u64,
    pub kinds: [u64; 4],
    pub size_log2: Vec<u64>,
    pub scanned: u64,
    pub touched: u64,
    pub invalidated: u64,
    pub key_samples: u64,
    pub distinct_keys: u64,
    pub repeated_keys: u64,
    pub hottest_key_count: u64,
    pub distinct_paths: u64,
    pub distinct_shapes: u64,
    pub consecutive_same_key: u64,
    pub kind_transitions: u64,
    pub longest_kind_run: u64,
    pub saturated: bool,
}
fn bounded(n: u64) -> bool {
    n <= 9_007_199_254_740_991
}
pub fn summarize(events: &[Event], overflow: bool) -> Result<Summary, String> {
    if events.len() > 256 {
        return Err("usage sample budget".into());
    }
    let mut s = Summary {
        samples: events.len() as u64,
        size_log2: vec![0; 55],
        saturated: overflow,
        ..Summary::default()
    };
    let (mut keys, mut paths, mut shapes) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let (mut previous_key, mut previous_kind, mut run) = (None, None, 0);
    fn count(map: &mut BTreeMap<u64, u64>, key: Option<u64>, saturated: &mut bool) {
        if let Some(key) = key {
            if map.contains_key(&key) || map.len() < 64 {
                *map.entry(key).or_default() += 1;
            } else {
                *saturated = true;
            }
        }
    }
    for e in events {
        if [
            e.key,
            e.path,
            e.shape,
            e.size,
            e.scanned,
            e.touched,
            e.invalidated,
        ]
        .into_iter()
        .flatten()
        .any(|n| !bounded(n))
        {
            return Err("usage integer budget".into());
        }
        let k = match e.kind {
            Kind::Read => 0,
            Kind::Write => 1,
            Kind::Scan => 2,
            Kind::Invalidate => 3,
        };
        s.kinds[k] += 1;
        s.failed += u64::from(e.failed);
        if previous_kind == Some(k) {
            run += 1;
        } else {
            if previous_kind.is_some() {
                s.kind_transitions += 1;
            }
            run = 1;
        }
        s.longest_kind_run = s.longest_kind_run.max(run);
        previous_kind = Some(k);
        if let Some(size) = e.size {
            let bucket = if size == 0 {
                0
            } else {
                64 - size.leading_zeros() as usize
            };
            s.size_log2[bucket] += 1;
        }
        for (sum, n) in [
            (&mut s.scanned, e.scanned),
            (&mut s.touched, e.touched),
            (&mut s.invalidated, e.invalidated),
        ] {
            let next = sum.checked_add(n.unwrap_or(0)).filter(|n| bounded(*n));
            if let Some(n) = next {
                *sum = n;
            } else {
                s.saturated = true;
            }
        }
        if e.key.is_some() {
            s.key_samples += 1;
            if previous_key == e.key {
                s.consecutive_same_key += 1;
            }
        }
        previous_key = e.key;
        count(&mut keys, e.key, &mut s.saturated);
        count(&mut paths, e.path, &mut s.saturated);
        count(&mut shapes, e.shape, &mut s.saturated);
    }
    s.distinct_keys = keys.len() as u64;
    s.distinct_paths = paths.len() as u64;
    s.distinct_shapes = shapes.len() as u64;
    s.repeated_keys = keys.values().map(|n| n - 1).sum();
    s.hottest_key_count = keys.values().copied().max().unwrap_or(0);
    Ok(s)
}
/// Total variation in the sampled operation mix. No threshold or prediction is
/// inferred here. Saturated/empty windows have no comparable drift estimate.
pub fn drift(a: &Summary, b: &Summary) -> Option<f64> {
    if a.saturated || b.saturated || a.samples == 0 || b.samples == 0 {
        return None;
    }
    Some(
        a.kinds
            .iter()
            .zip(b.kinds)
            .map(|(x, y)| (*x as f64 / a.samples as f64 - y as f64 / b.samples as f64).abs())
            .sum::<f64>()
            / 2.,
    )
}
