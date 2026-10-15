//! Versioned, authored objectives over complete eligible candidate forecasts.
//! Scores are predictions, never semantic, applicability or migration authority.
//! Hosts evaluate ordinary compiled goal code; this module only binds and ranks.
use crate::adaptive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub source: String,
    pub entry: String,
    pub revision: u64,
    pub context: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Minimize,
    Maximize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: u32,
    pub adaptive: adaptive::Request,
    pub goal: Goal,
    pub direction: Direction,
    pub minimum_improvement: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_goal: Option<Goal>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub variant: String,
    pub artifact: String,
    pub current: bool,
    pub workload: Vec<Cell>,
    pub window_calls: u64,
    pub total_calls: u64,
    pub state_elements: u64,
    pub horizon_calls: u64,
    pub execution_low_ns: f64,
    pub execution_high_ns: f64,
    pub migration_high_ns: f64,
    pub migration_margin: f64,
    pub total_low_ns: f64,
    pub total_high_ns: f64,
    pub peak_bytes: u64,
    pub migration_peak_bytes: u64,
    pub migration_pause_ns: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub name: String,
    pub calls: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Preparation {
    pub subject: String,
    pub goal: Goal,
    pub candidates: Vec<Candidate>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub low: f64,
    pub high: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub subject: String,
    pub scores: BTreeMap<String, Score>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Decision {
    pub selected: String,
    pub reason: String,
    pub state: adaptive::State,
    pub goal: Goal,
    pub predicted_improvement: f64,
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn valid_goal(g: &Goal) -> bool {
    hash(&g.source)
        && hash(&g.context)
        && g.revision <= 9_007_199_254_740_991
        && !g.entry.is_empty()
        && g.entry.len() <= 256
        && g.entry
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.".contains(&c))
}
pub fn validate_goal(g: &Goal, minimum_improvement: f64) -> Result<(), String> {
    if !valid_goal(g)
        || !minimum_improvement.is_finite()
        || !(0.0..=1e100).contains(&minimum_improvement)
    {
        return Err("objective identity/policy budget".into());
    }
    Ok(())
}
pub fn prepare(r: &Request) -> Result<Preparation, String> {
    adaptive::validate(&r.adaptive)?;
    validate_goal(&r.goal, r.minimum_improvement)?;
    if r.schema != 1 || r.previous_goal.as_ref().is_some_and(|g| !valid_goal(g)) {
        return Err("objective identity/policy budget".into());
    }
    let a = &r.adaptive;
    let mut candidates = vec![];
    for (id, variant) in &a.profile.variants {
        let current = id == &a.current;
        let migration = if current {
            None
        } else {
            let Some(migration) = adaptive::eligible_migration(a, id) else {
                continue;
            };
            Some(migration)
        };
        let (Some(low), Some(high)) = (
            adaptive::horizon_cost(a, id, false),
            adaptive::horizon_cost(a, id, true),
        ) else {
            continue;
        };
        let migration_high_ns = migration.map_or(0.0, |m| m.high_ns);
        let total_high_ns = high + migration_high_ns * a.policy.migration_margin;
        if !low.is_finite() || !high.is_finite() || !total_high_ns.is_finite() {
            return Err("objective forecast overflow".into());
        }
        candidates.push(Candidate {
            variant: id.clone(),
            artifact: a.binding.variants[id].clone(),
            current,
            workload: a
                .window
                .counts
                .iter()
                .map(|(name, calls)| Cell {
                    name: name.clone(),
                    calls: *calls,
                })
                .collect(),
            window_calls: a.window.calls,
            total_calls: a.window.total_calls,
            state_elements: a.window.state_elements,
            horizon_calls: a.policy.horizon_calls,
            execution_low_ns: low,
            execution_high_ns: high,
            migration_high_ns,
            migration_margin: a.policy.migration_margin,
            total_low_ns: low,
            total_high_ns,
            peak_bytes: variant.peak_bytes,
            migration_peak_bytes: migration.map_or(0, |m| m.peak_bytes),
            migration_pause_ns: migration.map_or(0.0, |m| m.max_pause_ns),
        });
    }
    // Bind the entire policy, workload, evidence, current implementation,
    // controller state and goal context. This hash binds bytes, not correctness.
    let bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?;
    if bytes.len() > 4_000_000 {
        return Err("objective request byte budget".into());
    }
    let subject = format!("{:x}", Sha256::digest(bytes));
    Ok(Preparation {
        subject,
        goal: r.goal.clone(),
        candidates,
    })
}
pub fn decide(r: &Request, assessment: &Assessment) -> Result<Decision, String> {
    let prepared = prepare(r)?;
    if assessment.subject != prepared.subject || assessment.scores.len() > 16 {
        return Err("stale objective assessment or score budget".into());
    }
    for (id, score) in &assessment.scores {
        if !prepared.candidates.iter().any(|c| &c.variant == id)
            || !score.low.is_finite()
            || !score.high.is_finite()
            || score.low.abs() > 1e100
            || score.high.abs() > 1e100
            || score.low > score.high
        {
            return Err("invalid or ineligible objective score".into());
        }
    }
    let a = &r.adaptive;
    let mut state = a.state.clone();
    if r.previous_goal.as_ref() != Some(&r.goal) {
        state.pending.clear();
        state.confirmations = 0;
    }
    let mut best = None;
    let mut improvement = 0.0;
    if !a.window.saturated {
        if let Some(baseline) = assessment.scores.get(&a.current) {
            for (id, score) in &assessment.scores {
                if id == &a.current {
                    continue;
                }
                let gain = match r.direction {
                    Direction::Minimize => baseline.low - score.high,
                    Direction::Maximize => score.low - baseline.high,
                };
                // Strict improvement preserves the current implementation on
                // ties, uncertainty overlap, absent scores and marginal gains.
                if gain > r.minimum_improvement && gain > improvement {
                    best = Some(id.as_str());
                    improvement = gain;
                }
            }
        }
    }
    let (state, ready) = adaptive::confirm(a, best, state);
    Ok(Decision {
        selected: if ready {
            best.unwrap().into()
        } else {
            a.current.clone()
        },
        reason: if ready {
            "authored objective improvement; resource and migration gates retained"
        } else if best.is_some() {
            "waiting for objective stability or residence interval"
        } else {
            "missing comparable objective improvement; current retained"
        }
        .into(),
        state,
        goal: r.goal.clone(),
        predicted_improvement: if ready { improvement } else { 0.0 },
    })
}
