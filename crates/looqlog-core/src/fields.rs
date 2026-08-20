//! Field inventory with a per-field cardinality cap (design.md D7, field-extraction
//! spec "Field inventory with a cardinality cap"). Filters need to know which fields
//! exist and which values they take; tracking every distinct value is unbounded for
//! any field carrying a request/trace id, so each field is capped independently and
//! flagged `high_cardinality` once it overflows.

use std::collections::BTreeMap;

/// Per-field statistics: total occurrences, and up to `cap` distinct values with
/// their counts.
#[derive(Debug, Clone, Default)]
pub struct FieldStats {
    pub count: usize,
    pub values: BTreeMap<String, usize>,
    pub high_cardinality: bool,
}

/// Inventory of every field seen across the parsed input.
#[derive(Debug, Clone)]
pub struct FieldInventory {
    cap: usize,
    fields: BTreeMap<String, FieldStats>,
}

impl FieldInventory {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            fields: BTreeMap::new(),
        }
    }

    /// Record one occurrence of `name` with display value `value`. Once a field's
    /// distinct-value count reaches `cap`, it is flagged high-cardinality and no
    /// further distinct values are tracked — `count` keeps incrementing (O(1)
    /// memory), but the value list stops growing.
    pub fn record(&mut self, name: &str, value: &str) {
        let stats = self.fields.entry(name.to_string()).or_default();
        stats.count += 1;
        if stats.high_cardinality {
            return;
        }
        if let Some(existing) = stats.values.get_mut(value) {
            *existing += 1;
        } else if stats.values.len() < self.cap {
            stats.values.insert(value.to_string(), 1);
        } else {
            stats.high_cardinality = true;
        }
    }

    pub fn fields(&self) -> &BTreeMap<String, FieldStats> {
        &self.fields
    }

    pub fn get(&self, name: &str) -> Option<&FieldStats> {
        self.fields.get(name)
    }

    pub fn cap(&self) -> usize {
        self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_cardinality_field_lists_all_values() {
        let mut inv = FieldInventory::new(10);
        inv.record("service", "api");
        inv.record("service", "db");
        inv.record("service", "api");
        let stats = inv.get("service").unwrap();
        assert_eq!(stats.count, 3);
        assert!(!stats.high_cardinality);
        assert_eq!(stats.values.get("api"), Some(&2));
        assert_eq!(stats.values.get("db"), Some(&1));
    }

    #[test]
    fn high_cardinality_field_is_capped_and_flagged() {
        let mut inv = FieldInventory::new(5);
        for i in 0..50_000 {
            inv.record("request_id", &format!("req-{i}"));
        }
        let stats = inv.get("request_id").unwrap();
        assert_eq!(stats.count, 50_000);
        assert!(stats.high_cardinality);
        assert!(stats.values.len() <= 5);
    }
}
