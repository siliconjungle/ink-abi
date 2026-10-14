//! Optional owned JSON read view. Sidecar offsets never change source values.
use crate::{
    json_value::{self, Json},
    value_wire::Limits,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Part {
    Key(String),
    Index(usize),
}
#[derive(Clone, Debug)]
pub struct Budget {
    pub step_entries: usize,
    pub min_reads: u64,
    pub max_entries: usize,
    pub max_objects: usize,
    pub max_index_bytes: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            step_entries: 16,
            min_reads: 4,
            max_entries: 4096,
            max_objects: 64,
            max_index_bytes: 262144,
        }
    }
}
#[derive(Default, Clone, Debug, Serialize)]
pub struct Stats {
    pub reads: u64,
    pub writes: u64,
    pub scanned_members: u64,
    pub index_hits: u64,
    pub built_entries: u64,
    pub invalidations: u64,
    pub index_entries: usize,
    pub index_objects: usize,
    pub index_bytes: usize,
}
struct Index {
    reads: u64,
    cursor: usize,
    keys: BTreeMap<String, usize>,
    bytes: usize,
}
pub struct Lookup {
    root: Json,
    limits: Limits,
    budget: Budget,
    indexes: BTreeMap<Vec<Part>, Index>,
    stats: Stats,
}
fn path_valid(path: &[Part]) -> bool {
    path.len() <= 128
        && path.iter().all(|p| match p {
            Part::Key(k) => k.encode_utf16().count() <= 4096,
            Part::Index(i) => (*i as u64) <= 9_007_199_254_740_991,
        })
}
fn resolve<'a>(root: &'a Json, path: &[Part]) -> Result<Option<&'a Json>, String> {
    let mut node = root;
    for part in path {
        node = match (node, part) {
            (Json::Object(members), Part::Key(key)) => match members.iter().find(|(k, _)| k == key)
            {
                Some((_, v)) => v,
                None => return Ok(None),
            },
            (Json::Array(items), Part::Index(i)) => match items.get(*i) {
                Some(v) => v,
                None => return Ok(None),
            },
            (_, Part::Key(_)) => return Err("expected JSON object".into()),
            (_, Part::Index(_)) => return Err("expected JSON array".into()),
        };
    }
    Ok(Some(node))
}
fn resolve_mut<'a>(root: &'a mut Json, path: &[Part]) -> Result<&'a mut Json, String> {
    let mut node = root;
    for part in path {
        node = match (node, part) {
            (Json::Object(members), Part::Key(key)) => {
                &mut members
                    .iter_mut()
                    .find(|(k, _)| k == key)
                    .ok_or("JSON update path missing")?
                    .1
            }
            (Json::Array(items), Part::Index(i)) => {
                items.get_mut(*i).ok_or("JSON update path missing")?
            }
            (_, Part::Key(_)) => return Err("expected JSON object".into()),
            (_, Part::Index(_)) => return Err("expected JSON array".into()),
        };
    }
    Ok(node)
}
impl Lookup {
    pub fn new(text: &str, limits: Limits, budget: Budget) -> Result<Self, String> {
        if budget.step_entries > 4096
            || budget.min_reads > 1_000_000
            || budget.max_entries > 1_000_000
            || budget.max_objects > 256
            || budget.max_index_bytes > 16_777_216
        {
            return Err("JSON index budget".into());
        }
        Ok(Self {
            root: json_value::parse(text, limits)?,
            limits,
            budget,
            indexes: BTreeMap::new(),
            stats: Stats::default(),
        })
    }
    pub fn lookup(&mut self, path: &[Part]) -> Result<Option<Json>, String> {
        if !path_valid(path) {
            return Err("JSON lookup path".into());
        }
        self.stats.reads += 1;
        let mut node = &self.root;
        let mut remaining = self.budget.step_entries;
        for (depth, part) in path.iter().enumerate() {
            match (node, part) {
                (Json::Array(items), Part::Index(i)) => {
                    let Some(v) = items.get(*i) else {
                        return Ok(None);
                    };
                    node = v;
                }
                (Json::Object(members), Part::Key(key)) => {
                    let prefix = &path[..depth];
                    let overhead = 128
                        + prefix
                            .iter()
                            .map(|p| match p {
                                Part::Key(k) => 16 + k.encode_utf16().count() * 2,
                                Part::Index(_) => 8,
                            })
                            .sum::<usize>();
                    if !self.indexes.contains_key(prefix)
                        && self.indexes.len() < self.budget.max_objects
                        && self.stats.index_bytes + overhead <= self.budget.max_index_bytes
                    {
                        self.indexes.insert(
                            prefix.to_vec(),
                            Index {
                                reads: 0,
                                cursor: 0,
                                keys: BTreeMap::new(),
                                bytes: overhead,
                            },
                        );
                        self.stats.index_bytes += overhead;
                    }
                    let mut start = 0;
                    if let Some(index) = self.indexes.get_mut(prefix) {
                        index.reads += 1;
                        if index.reads >= self.budget.min_reads {
                            while remaining > 0
                                && index.cursor < members.len()
                                && self.stats.index_entries < self.budget.max_entries
                            {
                                let (key, _) = &members[index.cursor];
                                let bytes = 64 + key.encode_utf16().count() * 2;
                                if self.stats.index_bytes + bytes > self.budget.max_index_bytes {
                                    break;
                                }
                                index.keys.insert(key.clone(), index.cursor);
                                index.cursor += 1;
                                index.bytes += bytes;
                                self.stats.index_bytes += bytes;
                                self.stats.index_entries += 1;
                                self.stats.built_entries += 1;
                                remaining -= 1;
                            }
                        }
                        if let Some(i) = index.keys.get(key) {
                            self.stats.index_hits += 1;
                            node = &members[*i].1;
                            continue;
                        }
                        start = index.cursor;
                    }
                    let mut found = None;
                    for (k, v) in &members[start..] {
                        self.stats.scanned_members += 1;
                        if k == key {
                            found = Some(v);
                            break;
                        }
                    }
                    let Some(v) = found else { return Ok(None) };
                    node = v;
                }
                (_, Part::Key(_)) => return Err("expected JSON object".into()),
                (_, Part::Index(_)) => return Err("expected JSON array".into()),
            }
        }
        Ok(Some(node.clone()))
    }
    pub fn set(&mut self, path: &[Part], key: String, text: &str) -> Result<(), String> {
        if !path_valid(path) {
            return Err("JSON lookup path".into());
        }
        let replacement = json_value::parse(text, self.limits)?;
        let mut next = self.root.clone();
        // Resolve first so a failed update never invalidates or changes the live view.
        resolve(&next, path)?.ok_or("JSON update path missing")?;
        let Json::Object(members) = resolve_mut(&mut next, path)? else {
            return Err("expected JSON object".into());
        };
        members.retain(|(k, _)| k != &key);
        members.insert(0, (key, replacement));
        next.stringify(self.limits)?;
        self.root = next;
        self.stats.writes += 1;
        self.indexes.retain(|p, index| {
            if p.starts_with(path) {
                self.stats.index_entries -= index.keys.len();
                self.stats.index_bytes -= index.bytes;
                self.stats.invalidations += 1;
                false
            } else {
                true
            }
        });
        Ok(())
    }
    pub fn replace(&mut self, text: &str) -> Result<(), String> {
        let next = json_value::parse(text, self.limits)?;
        self.root = next;
        self.stats.writes += 1;
        self.stats.invalidations += self.indexes.len() as u64;
        self.indexes.clear();
        self.stats.index_entries = 0;
        self.stats.index_bytes = 0;
        Ok(())
    }
    pub fn stringify(&self) -> Result<String, String> {
        self.root.stringify(self.limits)
    }
    pub fn stats(&self) -> Stats {
        let mut s = self.stats.clone();
        s.index_objects = self.indexes.len();
        s
    }
}
