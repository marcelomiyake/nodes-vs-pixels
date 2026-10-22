//! The page's undo history. Chrome keeps one for the whole page rather than
//! one per input: each edit of any input is a step, and typing in a row is
//! one step.

use form_core::Field;

/// An input's value and selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub field: Field,
    pub text: String,
    pub anchor: usize,
    pub head: usize,
}

/// What an edit did, which decides whether it joins the step that typing
/// left open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Typing joins the open step.
    Type,
    /// Backspace joins it only after another Backspace.
    Backspace,
    /// Delete joins it.
    Delete,
    /// Pasting or cutting is a step of its own.
    Replace,
}

#[derive(Debug)]
struct Step {
    before: Snapshot,
    after: Snapshot,
    last: Edit,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Whether the last step takes more typing.
    open: bool,
}

impl History {
    /// Records an edit from `before` to `after`, which puts the steps undone
    /// out of reach of redo.
    pub fn record(&mut self, edit: Edit, before: Snapshot, after: Snapshot) {
        self.redo.clear();
        let open = self.undo.last_mut().filter(|step| {
            self.open
                && match edit {
                    Edit::Type | Edit::Delete => true,
                    Edit::Backspace => step.last == Edit::Backspace,
                    Edit::Replace => false,
                }
        });
        if let Some(step) = open {
            step.after = after;
            step.last = edit;
        } else {
            self.undo.push(Step {
                before,
                after,
                last: edit,
            });
        }
        self.open = edit != Edit::Replace;
    }

    /// Ends the typing in a row, as moving the selection or focus does.
    pub const fn close(&mut self) {
        self.open = false;
    }

    /// Takes the last step back, to the value and selection it started from.
    pub fn undo(&mut self) -> Option<Snapshot> {
        self.open = false;
        let step = self.undo.pop()?;
        let before = step.before.clone();
        self.redo.push(step);
        Some(before)
    }

    /// Makes the last step undone again, with the value and selection it
    /// ended with.
    pub fn redo(&mut self) -> Option<Snapshot> {
        self.open = false;
        let step = self.redo.pop()?;
        let after = step.after.clone();
        self.undo.push(step);
        Some(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, caret: usize) -> Snapshot {
        Snapshot {
            field: Field::FullName,
            text: text.to_owned(),
            anchor: caret,
            head: caret,
        }
    }

    /// Records each edit of `edits` from the value before it to the next.
    fn history(start: &str, edits: &[(Edit, &str)]) -> History {
        let mut history = History::default();
        let mut text = start.to_owned();
        for &(edit, next) in edits {
            history.record(edit, at(&text, text.len()), at(next, next.len()));
            next.clone_into(&mut text);
        }
        history
    }

    fn undone(history: &mut History) -> Vec<String> {
        std::iter::from_fn(|| history.undo().map(|snapshot| snapshot.text)).collect()
    }

    #[test]
    fn typing_in_a_row_is_one_step() {
        let mut typed = history("", &[(Edit::Type, "a"), (Edit::Type, "ab")]);
        assert_eq!(undone(&mut typed), [""]);
        // Moving the selection ends it.
        let mut moved = history("", &[(Edit::Type, "a")]);
        moved.close();
        moved.record(Edit::Type, at("a", 0), at("ba", 1));
        assert_eq!(undone(&mut moved), ["a", ""]);
    }

    #[test]
    fn deleting_joins_typing_as_chrome_does() {
        // Backspace starts a step after typing, which typing then joins.
        let mut back = history(
            "ab",
            &[
                (Edit::Type, "abc"),
                (Edit::Backspace, "ab"),
                (Edit::Backspace, "a"),
                (Edit::Type, "ad"),
                (Edit::Backspace, "a"),
            ],
        );
        assert_eq!(undone(&mut back), ["ad", "abc", "ab"]);
        // Delete joins any of them.
        let mut forward = history(
            "abc",
            &[
                (Edit::Backspace, "ab"),
                (Edit::Delete, "a"),
                (Edit::Type, "ax"),
                (Edit::Delete, "x"),
            ],
        );
        assert_eq!(undone(&mut forward), ["abc"]);
        // Pasting and cutting are steps of their own.
        let mut pasted = history(
            "",
            &[
                (Edit::Type, "a"),
                (Edit::Replace, "aXY"),
                (Edit::Type, "aXYz"),
            ],
        );
        assert_eq!(undone(&mut pasted), ["aXY", "a", ""]);
    }

    #[test]
    fn redo_takes_back_an_undo_until_the_next_edit() {
        let mut history = history("", &[(Edit::Type, "a"), (Edit::Replace, "ab")]);
        assert_eq!(history.undo(), Some(at("a", 1)));
        assert_eq!(history.undo(), Some(at("", 0)));
        assert_eq!(history.undo(), None);
        assert_eq!(history.redo(), Some(at("a", 1)));
        // Typing after an undo starts a step, and nothing is left to redo.
        history.record(Edit::Type, at("a", 1), at("ac", 2));
        assert_eq!(history.redo(), None);
        assert_eq!(undone(&mut history), ["a", ""]);
    }
}
