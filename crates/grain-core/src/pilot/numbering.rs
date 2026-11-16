//! Pilot's own control numbers.
//!
//! The driver numbers rows by position in each read, and every read replaces
//! the previous snapshot: after the page changes, `[106]` can name a different
//! control. A model that clicks a number from an older read could then press
//! the wrong button. So Pilot gives each control its own number, keyed by
//! role, name and which occurrence of that role and name it is, and never
//! reuses a number for a different control while the window lives. A click
//! resolves the number against the latest read; if the control is gone, the
//! click is refused instead of guessed.

use std::collections::HashMap;

/// What identifies a control across reads.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ControlKey {
    pub role: String,
    pub name: String,
    /// 0 for the first control with this role and name in a read, 1 for the
    /// second, and so on.
    pub occurrence: u32,
}

/// Most numbers one window keeps before starting over. A fresh start only
/// means older numbers stop resolving; it never re-points one.
const MAX_NUMBERS: usize = 4_000;

/// The numbers handed out for one window.
#[derive(Debug, Default, Clone)]
pub struct Numbering {
    by_key: HashMap<ControlKey, u32>,
    by_number: HashMap<u32, ControlKey>,
    next: u32,
    /// Numbers below this were handed out before the last reset and no
    /// longer resolve.
    floor: u32,
}

impl Numbering {
    pub fn new() -> Self {
        Self {
            next: 1,
            floor: 1,
            ..Self::default()
        }
    }

    /// The number for `key`, handing out a new one the first time.
    pub fn number(&mut self, key: &ControlKey) -> u32 {
        if let Some(number) = self.by_key.get(key) {
            return *number;
        }
        if self.by_key.len() >= MAX_NUMBERS {
            // Start over above every number already shown, so an old number
            // can never come back pointing at something else.
            self.by_key.clear();
            self.by_number.clear();
            self.floor = self.next;
        }
        let number = self.next.max(1);
        self.next = number + 1;
        self.by_key.insert(key.clone(), number);
        self.by_number.insert(number, key.clone());
        number
    }

    /// The control a number was given to, if it is still known.
    pub fn key(&self, number: u32) -> Option<&ControlKey> {
        if number < self.floor {
            return None;
        }
        self.by_number.get(&number)
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

/// Where a number points in the latest read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The driver's row index in the latest read.
    Row(u32),
    /// Pilot never gave out this number for this window.
    Unknown,
    /// The control the number named is not in the latest read.
    Gone(ControlKey),
}

/// Resolve `number` against the latest read, given as (driver row index, key)
/// pairs for every control in it.
pub fn resolve(numbering: &Numbering, number: u32, latest: &[(u32, ControlKey)]) -> Resolved {
    let Some(key) = numbering.key(number) else {
        return Resolved::Unknown;
    };
    latest
        .iter()
        .find(|(_, candidate)| candidate == key)
        .map(|(row, _)| Resolved::Row(*row))
        .unwrap_or_else(|| Resolved::Gone(key.clone()))
}

/// Give every control in a read its occurrence count, in read order.
pub fn keys_in_order<'a>(
    controls: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<ControlKey> {
    let mut seen: HashMap<(String, String), u32> = HashMap::new();
    controls
        .into_iter()
        .map(|(role, name)| {
            let slot = seen
                .entry((role.to_string(), name.to_string()))
                .or_insert(0);
            let key = ControlKey {
                role: role.to_string(),
                name: name.to_string(),
                occurrence: *slot,
            };
            *slot += 1;
            key
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(role: &str, name: &str, occurrence: u32) -> ControlKey {
        ControlKey {
            role: role.into(),
            name: name.into(),
            occurrence,
        }
    }

    #[test]
    fn same_control_keeps_its_number_across_reads() {
        let mut numbering = Numbering::new();
        let book = key("button", "Book 10:15 PM", 0);
        let first = numbering.number(&book);
        numbering.number(&key("link", "Movies", 0));
        assert_eq!(numbering.number(&book), first);
        assert_eq!(first, 1);
    }

    #[test]
    fn a_number_follows_its_control_when_driver_rows_move() {
        let mut numbering = Numbering::new();
        let book = key("button", "Book 10:15 PM", 0);
        let number = numbering.number(&book);
        // The page changed: the driver's row index moved from 208 to 151.
        let latest = vec![(150, key("button", "Go back", 0)), (151, book.clone())];
        assert_eq!(resolve(&numbering, number, &latest), Resolved::Row(151));
    }

    #[test]
    fn a_vanished_control_is_refused_not_guessed() {
        let mut numbering = Numbering::new();
        let pay = key("button", "Pay ₹ 340", 0);
        let number = numbering.number(&pay);
        let latest = vec![(10, key("button", "Accept", 0))];
        assert_eq!(resolve(&numbering, number, &latest), Resolved::Gone(pay));
        assert_eq!(resolve(&numbering, 999, &latest), Resolved::Unknown);
    }

    #[test]
    fn duplicates_are_told_apart_by_occurrence() {
        let keys = keys_in_order([("link", "Hindi"), ("button", "Book"), ("link", "Hindi")]);
        assert_eq!(keys[0], key("link", "Hindi", 0));
        assert_eq!(keys[2], key("link", "Hindi", 1));
        let mut numbering = Numbering::new();
        assert_ne!(numbering.number(&keys[0]), numbering.number(&keys[2]));
    }

    #[test]
    fn a_reset_never_repoints_an_old_number() {
        let mut numbering = Numbering::new();
        let first = numbering.number(&key("button", "first", 0));
        for i in 0..MAX_NUMBERS {
            numbering.number(&key("button", &format!("b{i}"), 0));
        }
        // The cap was reached and the map started over above every old number.
        assert!(numbering.key(first).is_none());
        let fresh = numbering.number(&key("button", "first", 0));
        assert!(fresh > first);
    }
}
