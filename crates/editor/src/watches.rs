//! The debugger's watch expressions (roadmap G6): kept across pauses and
//! Play sessions, worked out again in each new pause's innermost frame.

/// One watch expression and its value in the current pause.
#[derive(Clone, Debug, PartialEq)]
pub struct Watch {
    pub expression: String,
    /// `None` until worked out in a pause; then the value as text, or why
    /// there's none.
    pub value: Option<Result<String, String>>,
}

#[derive(Debug, Default)]
pub struct Watches {
    pub list: Vec<Watch>,
    /// What's being typed in the box for a new watch.
    pub draft: String,
    /// Set when one is added, so the host works it out.
    stale: bool,
}

impl Watches {
    /// Adds the draft as a watch, if it isn't blank.
    pub fn add_draft(&mut self) {
        let expression = self.draft.trim().to_owned();
        self.draft.clear();
        if expression.is_empty() {
            return;
        }
        self.list.push(Watch {
            expression,
            value: None,
        });
        self.stale = true;
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.list.len() {
            self.list.remove(index);
        }
    }

    /// Whether a watch has no value yet for this pause.
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// Works each one out with `evaluate` (in the paused frame).
    pub fn evaluate(&mut self, mut evaluate: impl FnMut(&str) -> Result<String, String>) {
        for watch in &mut self.list {
            watch.value = Some(evaluate(&watch.expression));
        }
        self.stale = false;
    }

    /// Forgets the values: the game ran on.
    pub fn clear_values(&mut self) {
        for watch in &mut self.list {
            watch.value = None;
        }
        self.stale = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watch_is_added_from_the_draft_and_worked_out_in_a_pause() {
        let mut watches = Watches {
            draft: "  ".to_owned(),
            ..Watches::default()
        };
        watches.add_draft();
        assert!(watches.list.is_empty(), "a blank draft adds nothing");

        watches.draft = " count * 2 ".to_owned();
        watches.add_draft();
        assert_eq!(watches.draft, "");
        assert!(watches.is_stale());
        watches.evaluate(|expression| Ok(format!("<{expression}>")));
        assert_eq!(watches.list[0].value, Some(Ok("<count * 2>".to_owned())));
        assert!(!watches.is_stale());

        watches.clear_values();
        assert_eq!(watches.list[0].value, None);
        watches.remove(0);
        watches.remove(3);
        assert!(watches.list.is_empty());
    }
}
