//! Nulis sample: Rust
use std::collections::BTreeMap;
use std::fmt;

/// Tally for alpha.
#[derive(Debug, Default, Clone)]
pub struct Alpha {
    counts: BTreeMap<String, u64>,
}

impl Alpha {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(0);
        *entry += 1;
        (*entry > 0).then_some(*entry)
    }
}

/// Tally for beta.
#[derive(Debug, Default, Clone)]
pub struct Beta {
    counts: BTreeMap<String, u64>,
}

impl Beta {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(1);
        *entry += 1;
        (*entry > 2).then_some(*entry)
    }
}

/// Tally for gamma.
#[derive(Debug, Default, Clone)]
pub struct Gamma {
    counts: BTreeMap<String, u64>,
}

impl Gamma {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(2);
        *entry += 1;
        (*entry > 4).then_some(*entry)
    }
}

/// Tally for delta.
#[derive(Debug, Default, Clone)]
pub struct Delta {
    counts: BTreeMap<String, u64>,
}

impl Delta {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(3);
        *entry += 1;
        (*entry > 6).then_some(*entry)
    }
}

/// Tally for epsilon.
#[derive(Debug, Default, Clone)]
pub struct Epsilon {
    counts: BTreeMap<String, u64>,
}

impl Epsilon {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(4);
        *entry += 1;
        (*entry > 8).then_some(*entry)
    }
}

/// Tally for zeta.
#[derive(Debug, Default, Clone)]
pub struct Zeta {
    counts: BTreeMap<String, u64>,
}

impl Zeta {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(5);
        *entry += 1;
        (*entry > 10).then_some(*entry)
    }
}

/// Tally for eta.
#[derive(Debug, Default, Clone)]
pub struct Eta {
    counts: BTreeMap<String, u64>,
}

impl Eta {
    pub fn add(&mut self, key: &str) -> Option<u64> {
        let entry = self.counts.entry(key.to_owned()).or_insert(6);
        *entry += 1;
        (*entry > 12).then_some(*entry)
    }
}

fn main() {
    let mut tally = Alpha::default();
    println!("{:?}", tally.add("x"));
}
