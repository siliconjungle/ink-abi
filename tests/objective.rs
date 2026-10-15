use ink_abi::{objective, selection};
use serde_json::json;
use std::{collections::BTreeMap, fs};

fn fixture() -> objective::Request {
    let binding = json!({"source":"a".repeat(64),"compiler":"b".repeat(64),"target":"fixture",
        "boundary":"owned-serial-state-v1","variants":{"direct":"c".repeat(64),"compact":"d".repeat(64)}});
    serde_json::from_value(json!({
        "schema":1,"goal":{"source":"e".repeat(64),"entry":"rank","revision":1,"context":"f".repeat(64)},
        "direction":"minimize","minimum_improvement":0,
        "previous_goal":{"source":"e".repeat(64),"entry":"rank","revision":1,"context":"f".repeat(64)},
        "adaptive":{"binding":binding,"profile":{"binding":binding,"variants":{
            "direct":{"cells":{"read@s":{"low_ns":100,"high_ns":110}},"peak_bytes":400},
            "compact":{"cells":{"read@s":{"low_ns":200,"high_ns":220}},"peak_bytes":80}
        },"migrations":[{"from":"direct","to":"compact","high_ns":5,"peak_bytes":500,
            "max_pause_ns":5,"min_state_elements":0,"max_state_elements":100}]},
        "policy":{"horizon_calls":100,"min_residence_calls":10,"stable_windows":2,"min_gain":0.15,
            "migration_margin":2,"memory_limit_bytes":1000,"max_pause_ns":500,"max_switches":4},
        "state":{"pending":"compact","confirmations":1,"last_switch_call":0,"switches":0},
        "window":{"counts":{"read@s":20},"calls":20,"total_calls":20,"state_elements":10,"saturated":false},
        "current":"direct"}
    })).unwrap()
}
fn scores(r: &objective::Request) -> objective::Assessment {
    let p = objective::prepare(r).unwrap();
    objective::Assessment {
        subject: p.subject,
        scores: p
            .candidates
            .into_iter()
            .map(|c| {
                (
                    c.variant,
                    objective::Score {
                        low: c.peak_bytes as f64,
                        high: c.peak_bytes as f64,
                    },
                )
            })
            .collect(),
    }
}
#[test]
fn objective_can_prefer_a_slower_variant_without_waiving_resource_gates() {
    let r = fixture();
    assert_eq!(
        ink_abi::adaptive::decide(&r.adaptive).unwrap().selected,
        "direct"
    );
    let prepared = objective::prepare(&r).unwrap();
    assert_eq!(prepared.candidates[0].workload[0].calls, 20);
    assert_eq!(prepared.candidates[0].total_high_ns, 22010.0);
    assert_eq!(prepared.candidates[0].artifact, "d".repeat(64));
    let d = objective::decide(&r, &scores(&r)).unwrap();
    assert_eq!(d.selected, "compact");
    assert_eq!(d.predicted_improvement, 320.0);
    for field in [
        "variant-memory",
        "migration-memory",
        "pause",
        "state-size",
        "saturation",
        "switches",
    ] {
        let mut r = fixture();
        match field {
            "variant-memory" => {
                r.adaptive
                    .profile
                    .variants
                    .get_mut("compact")
                    .unwrap()
                    .peak_bytes = 1001
            }
            "migration-memory" => r.adaptive.profile.migrations[0].peak_bytes = 1001,
            "pause" => r.adaptive.profile.migrations[0].max_pause_ns = 501.0,
            "state-size" => r.adaptive.window.state_elements = 101,
            "saturation" => r.adaptive.window.saturated = true,
            "switches" => r.adaptive.state.switches = 4,
            _ => unreachable!(),
        }
        assert_eq!(
            objective::prepare(&r).unwrap().candidates.len(),
            1,
            "{field}"
        );
        assert_eq!(
            objective::decide(&r, &scores(&r)).unwrap().selected,
            "direct",
            "{field}"
        );
    }
}
#[test]
fn changed_goals_restart_confirmations_but_preserve_residence_and_switch_budgets() {
    let mut r = fixture();
    r.goal.revision += 1;
    let d = objective::decide(&r, &scores(&r)).unwrap();
    assert_eq!(d.selected, "direct");
    assert_eq!(d.state.confirmations, 1);
    assert_eq!(d.state.last_switch_call, 0);
    assert_eq!(d.state.switches, 0);
    r.adaptive.state = d.state;
    r.previous_goal = Some(d.goal);
    assert_eq!(
        objective::decide(&r, &scores(&r)).unwrap().selected,
        "compact"
    );
    r.adaptive.state.last_switch_call = 19;
    assert_eq!(
        objective::decide(&r, &scores(&r)).unwrap().selected,
        "direct"
    );
}
#[test]
fn signed_scores_maximization_missing_data_ties_and_uncertainty_are_explicit() {
    let mut r = fixture();
    let mut a = scores(&r);
    a.scores = BTreeMap::from([
        (
            "direct".into(),
            objective::Score {
                low: -10.0,
                high: -10.0,
            },
        ),
        (
            "compact".into(),
            objective::Score {
                low: -3.0,
                high: -3.0,
            },
        ),
    ]);
    assert_eq!(objective::decide(&r, &a).unwrap().selected, "direct");
    r.direction = objective::Direction::Maximize;
    a.subject = objective::prepare(&r).unwrap().subject;
    assert_eq!(objective::decide(&r, &a).unwrap().selected, "compact");
    r.minimum_improvement = 8.0;
    a.subject = objective::prepare(&r).unwrap().subject;
    assert_eq!(objective::decide(&r, &a).unwrap().selected, "direct");
    r = fixture();
    a = scores(&r);
    for values in [(0.0, 0.0, 0.0, 0.0), (100.0, 200.0, 90.0, 150.0)] {
        a.scores.insert(
            "direct".into(),
            objective::Score {
                low: values.0,
                high: values.1,
            },
        );
        a.scores.insert(
            "compact".into(),
            objective::Score {
                low: values.2,
                high: values.3,
            },
        );
        assert_eq!(objective::decide(&r, &a).unwrap().selected, "direct");
    }
    a = scores(&r);
    a.scores.remove("direct");
    assert_eq!(objective::decide(&r, &a).unwrap().selected, "direct");
    r.adaptive
        .profile
        .variants
        .get_mut("compact")
        .unwrap()
        .cells
        .clear();
    assert_eq!(objective::prepare(&r).unwrap().candidates.len(), 1);
    assert!(
        objective::decide(&r, &a).is_err(),
        "stale assessment cannot survive evidence changes"
    );
}
#[test]
fn entire_request_is_bound_and_malformed_scores_never_bypass_eligibility() {
    let r = fixture();
    let a = scores(&r);
    for field in [
        "goal", "context", "policy", "profile", "window", "state", "current",
    ] {
        let mut other = fixture();
        match field {
            "goal" => other.goal.revision += 1,
            "context" => other.goal.context = "0".repeat(64),
            "policy" => other.minimum_improvement = 1.0,
            "profile" => {
                other
                    .adaptive
                    .profile
                    .variants
                    .get_mut("compact")
                    .unwrap()
                    .peak_bytes += 1
            }
            "window" => other.adaptive.window.total_calls += 1,
            "state" => other.adaptive.state.confirmations = 0,
            "current" => other.adaptive.current = "compact".into(),
            _ => unreachable!(),
        }
        assert!(
            objective::decide(&other, &a).unwrap_err().contains("stale"),
            "{field}"
        );
    }
    for (low, high) in [
        (f64::NAN, 1.0),
        (0.0, f64::INFINITY),
        (2.0, 1.0),
        (-1e101, 1.0),
    ] {
        let mut bad = a.clone();
        bad.scores
            .insert("compact".into(), objective::Score { low, high });
        assert!(objective::decide(&r, &bad).is_err());
    }
    let mut bad = a;
    bad.scores.insert(
        "uncompiled".into(),
        objective::Score {
            low: -1e99,
            high: -1e99,
        },
    );
    assert!(objective::decide(&r, &bad).is_err());
}
#[test]
fn native_and_wasm_execute_the_same_versioned_objective_contract() {
    let r = fixture();
    let mut cases = vec![];
    for variant in 0..8 {
        let mut r = r.clone();
        match variant {
            1 => r.goal.revision += 1,
            2 => r.adaptive.policy.memory_limit_bytes = 450,
            3 => r.adaptive.state.last_switch_call = 19,
            4 => r.minimum_improvement = 500.0,
            5 => r.direction = objective::Direction::Maximize,
            6 => r.previous_goal = None,
            7 => r
                .adaptive
                .profile
                .variants
                .get_mut("compact")
                .unwrap()
                .cells
                .clear(),
            _ => (),
        }
        let prepared = json!({"operation":"objective-prepare","request":r});
        let decided = json!({"operation":"objective-decide","request":r,"assessment":scores(&r)});
        for request in [prepared, decided] {
            cases.push(json!({"request":request,"expected":selection::request(&request).unwrap()}));
        }
    }
    let root = std::env::temp_dir().join(format!("ink-objective-parity-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("ink-selection.wasm"), ink_abi::SELECTION_WASM).unwrap();
    fs::write(root.join("selection.mjs"), ink_abi::SELECTION_JS_SOURCE).unwrap();
    fs::write(root.join("cases.json"), serde_json::to_vec(&cases).unwrap()).unwrap();
    fs::write(root.join("check.mjs"),"import fs from 'node:fs';import assert from 'node:assert/strict';import{selectionRequest}from './selection.mjs';for(const row of JSON.parse(fs.readFileSync(new URL('./cases.json',import.meta.url))))assert.deepEqual(await selectionRequest(row.request),row.expected);").unwrap();
    let result = std::process::Command::new("node")
        .arg(root.join("check.mjs"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    fs::remove_dir_all(root).unwrap();
}
