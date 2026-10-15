//! Bounded, workload-window selection. Measurements never confer semantic authority.
//! Hosts supply only previously checked/compiled variants. No code runs here.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub source: String,
    pub compiler: String,
    pub target: String,
    pub boundary: String,
    pub variants: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Price {
    pub low_ns: f64,
    pub high_ns: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub cells: BTreeMap<String, Price>,
    pub peak_bytes: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    pub from: String,
    pub to: String,
    pub high_ns: f64,
    pub peak_bytes: u64,
    pub max_pause_ns: f64,
    pub min_state_elements: u64,
    pub max_state_elements: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub binding: Binding,
    pub variants: BTreeMap<String, Variant>,
    pub migrations: Vec<Migration>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub horizon_calls: u64,
    pub min_residence_calls: u64,
    pub stable_windows: u64,
    pub min_gain: f64,
    pub migration_margin: f64,
    pub memory_limit_bytes: u64,
    pub max_pause_ns: f64,
    pub max_switches: u64,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub pending: String,
    pub confirmations: u64,
    pub last_switch_call: u64,
    pub switches: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub counts: BTreeMap<String, u64>,
    pub calls: u64,
    pub total_calls: u64,
    pub state_elements: u64,
    pub saturated: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub binding: Binding,
    pub profile: Profile,
    pub policy: Policy,
    pub state: State,
    pub window: Window,
    pub current: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Decision {
    pub selected: String,
    pub reason: String,
    pub state: State,
    pub predicted_saving_ns: f64,
}
fn price(n: f64) -> bool {
    n.is_finite() && (0.0..=1e15).contains(&n)
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
fn name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.is_ascii()
}
pub fn validate(r: &Request) -> Result<(), String> {
    let b = &r.binding;
    let p = &r.policy;
    let w = &r.window;
    if b != &r.profile.binding
        || !hash(&b.source)
        || !hash(&b.compiler)
        || !name(&b.target)
        || !name(&b.boundary)
        || b.variants.is_empty()
        || b.variants.len() > 16
        || b.variants.iter().any(|(k, v)| !name(k) || !hash(v))
        || !b.variants.contains_key(&r.current)
        || r.profile.variants.keys().ne(b.variants.keys())
    {
        return Err("adaptive binding/variant mismatch".into());
    }
    if !(1..=1_000_000).contains(&p.horizon_calls)
        || p.min_residence_calls > 1_000_000_000
        || !(1..=64).contains(&p.stable_windows)
        || !p.min_gain.is_finite()
        || !(0.0..1.0).contains(&p.min_gain)
        || !p.migration_margin.is_finite()
        || !(1.0..=100.0).contains(&p.migration_margin)
        || p.memory_limit_bytes == 0
        || p.memory_limit_bytes > 9_007_199_254_740_991
        || !price(p.max_pause_ns)
        || p.max_switches > 1_000_000
        || w.counts.len() > 64
        || w.counts.keys().any(|s| !name(s))
        || w.calls == 0
        || w.calls > 1_000_000
        || w.total_calls > 9_007_199_254_740_991
        || w.total_calls < w.calls
        || w.counts.values().try_fold(0u64, |a, b| a.checked_add(*b)) != Some(w.calls)
        || w.total_calls < r.state.last_switch_call
        || w.state_elements > 9_007_199_254_740_991
        || r.state.confirmations > 64
        || (!r.state.pending.is_empty() && !b.variants.contains_key(&r.state.pending))
    {
        return Err("adaptive policy/window budget".into());
    }
    for v in r.profile.variants.values() {
        if v.cells.len() > 256
            || v.peak_bytes > 9_007_199_254_740_991
            || v.cells.iter().any(|(k, c)| {
                !name(k) || !price(c.low_ns) || !price(c.high_ns) || c.low_ns > c.high_ns
            })
        {
            return Err("adaptive cost cell".into());
        }
    }
    let mut edges = BTreeSet::new();
    if r.profile.migrations.len() > 240 {
        return Err("adaptive migration budget".into());
    }
    for m in &r.profile.migrations {
        if !b.variants.contains_key(&m.from)
            || !b.variants.contains_key(&m.to)
            || m.from == m.to
            || !edges.insert((&m.from, &m.to))
            || !price(m.high_ns)
            || !price(m.max_pause_ns)
            || m.peak_bytes > 9_007_199_254_740_991
            || m.min_state_elements > m.max_state_elements
            || m.max_state_elements > 9_007_199_254_740_991
        {
            return Err("adaptive migration evidence".into());
        }
    }
    Ok(())
}
pub(crate) fn horizon_cost(r: &Request, id: &str, upper: bool) -> Option<f64> {
    let variant = r.profile.variants.get(id)?;
    r.window
        .counts
        .iter()
        .try_fold(0.0, |total, (cell, n)| {
            variant
                .cells
                .get(cell)
                .map(|price| total + *n as f64 * if upper { price.high_ns } else { price.low_ns })
        })
        .map(|n| n / r.window.calls as f64 * r.policy.horizon_calls as f64)
}

/// Shared hard resource/evidence gate. A different objective cannot waive it.
pub(crate) fn eligible_migration<'a>(r: &'a Request, id: &str) -> Option<&'a Migration> {
    let variant = r.profile.variants.get(id)?;
    let policy = &r.policy;
    if id == r.current
        || r.window.saturated
        || r.state.switches >= policy.max_switches
        || variant.peak_bytes > policy.memory_limit_bytes
    {
        return None;
    }
    let migration = r
        .profile
        .migrations
        .iter()
        .find(|m| m.from == r.current && m.to == id)?;
    (migration.peak_bytes <= policy.memory_limit_bytes
        && migration.max_pause_ns <= policy.max_pause_ns
        && r.window.state_elements >= migration.min_state_elements
        && r.window.state_elements <= migration.max_state_elements)
        .then_some(migration)
}

/// A proposal is acknowledged by the host only after successful migration.
pub(crate) fn confirm(r: &Request, candidate: Option<&str>, mut state: State) -> (State, bool) {
    let mut ready = false;
    if let Some(id) = candidate {
        state.confirmations = if state.pending == id {
            (state.confirmations + 1).min(64)
        } else {
            1
        };
        state.pending = id.into();
        ready = state.confirmations >= r.policy.stable_windows
            && r.window.total_calls - state.last_switch_call >= r.policy.min_residence_calls;
    } else {
        state.pending.clear();
        state.confirmations = 0;
    }
    (state, ready)
}

pub fn decide(r: &Request) -> Result<Decision, String> {
    validate(r)?;
    let p = &r.policy;
    let w = &r.window;
    let mut result = Decision {
        selected: r.current.clone(),
        reason: "no comparable profitable alternative".into(),
        state: r.state.clone(),
        predicted_saving_ns: 0.0,
    };
    let baseline = horizon_cost(r, &r.current, false);
    let mut best = None;
    let mut best_saving = 0.0;
    if !w.saturated && r.state.switches < p.max_switches {
        if let Some(base) = baseline {
            for id in r.profile.variants.keys() {
                let Some(m) = eligible_migration(r, id) else {
                    continue;
                };
                let Some(c) = horizon_cost(r, id, true) else {
                    continue;
                };
                let saving = base - c - m.high_ns * p.migration_margin;
                if saving > base * p.min_gain && saving > best_saving {
                    best = Some(id.clone());
                    best_saving = saving;
                }
            }
        }
    }
    let (state, ready) = confirm(r, best.as_deref(), r.state.clone());
    if let Some(id) = best {
        result.reason = "waiting for sustained savings or residence interval".into();
        if ready {
            result.selected = id;
            result.reason = "measured savings cover migration and uncertainty margin".into();
            result.predicted_saving_ns = best_saving;
            // Host acknowledges only after a successful migration; proposal is not a commit.
        }
    }
    result.state = state;
    Ok(result)
}
