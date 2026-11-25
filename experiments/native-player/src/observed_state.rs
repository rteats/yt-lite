//! Owned playback-state snapshot. No libmpv pointers or borrowed event payloads
//! survive the event drain. Subscription generations reject superseded updates.
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum ObservedValue {
    Double(f64),
    Flag(bool),
    Ranges(Vec<(f64, f64)>),
}

#[derive(Default)]
pub struct ObservedState {
    generation: u64,
    active: bool,
    values: HashMap<&'static str, ObservedValue>,
}

impl ObservedState {
    pub fn active_id(&self) -> Option<u64> {
        self.active.then_some(self.generation)
    }

    /// Each reset invalidates prior subscription IDs and every cached value.
    /// Re-observation requests the initial value even if it has not changed.
    pub fn reset(&mut self, active: bool) -> Option<u64> {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.active = active;
        self.values.clear();
        self.active_id()
    }

    pub fn update(&mut self, id: u64, name: &'static str, value: Option<ObservedValue>) {
        if self.active_id() != Some(id) { return; }
        match value {
            Some(ObservedValue::Double(v)) if !v.is_finite() => { self.values.remove(name); }
            Some(value) => { self.values.insert(name, value); }
            None => { self.values.remove(name); }
        }
    }

    pub fn double(&self, name: &str) -> Option<f64> {
        match self.values.get(name) {
            Some(ObservedValue::Double(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        match self.values.get(name) {
            Some(ObservedValue::Flag(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn ranges(&self) -> Option<Vec<(f64, f64)>> {
        match self.values.get("demuxer-cache-state") {
            Some(ObservedValue::Ranges(v)) => Some(v.clone()),
            _ => None,
        }
    }
}
