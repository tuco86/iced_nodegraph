//! Host-configurable, platform-aware keymap for graph-level actions.
//!
//! This module is pure data plus a resolver: it decides which [`KeyAction`] a
//! key press maps to, given the current [`Keymap`]. It performs no widget
//! wiring; the widget owns the event loop and calls [`Keymap::key_action`].

use iced::keyboard::key::{Named, Physical};
use iced::keyboard::{Key, Modifiers};
use iced::mouse;

/// A graph-level action triggered by a keyboard shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Select every node in the graph.
    SelectAll,
    /// Clear the current selection.
    ClearSelection,
    /// Duplicate the selected nodes.
    CloneSelection,
    /// Remove the selected nodes (and their incident edges).
    DeleteSelection,
}

/// The logical key half of a [`KeyCombo`].
///
/// `Char` resolves layout-independently (see [`KeyCombo::matches`]); `Named`
/// matches an [`iced::keyboard::key::Named`] key directly, since named keys
/// (e.g. `Escape`, `Delete`) carry no layout ambiguity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboKey {
    /// A printable character, matched case-insensitively and independent of
    /// keyboard layout.
    Char(char),
    /// A named key, e.g. `Escape` or `Delete`.
    Named(Named),
}

/// A key plus the exact modifier state required to trigger it.
///
/// Build one with [`KeyCombo::new`] or the platform-flavored shorthands
/// [`KeyCombo::command`], [`KeyCombo::alt`], [`KeyCombo::bare`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCombo {
    /// The key that must be pressed.
    pub key: ComboKey,
    /// The modifiers that must be held, compared by exact equality.
    pub modifiers: Modifiers,
}

impl KeyCombo {
    /// Builds a combo from an explicit key and modifier state.
    pub fn new(key: ComboKey, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Builds a combo for `c` held with the platform "command" modifier
    /// (`Cmd` on macOS, `Ctrl` elsewhere).
    pub fn command(c: char) -> Self {
        Self::new(ComboKey::Char(c), Modifiers::COMMAND)
    }

    /// Builds a combo for `c` held with `Alt`.
    pub fn alt(c: char) -> Self {
        Self::new(ComboKey::Char(c), Modifiers::ALT)
    }

    /// Builds a combo for `key` with no modifiers held.
    pub fn bare(key: ComboKey) -> Self {
        Self::new(key, Modifiers::empty())
    }

    /// Reports whether a key press satisfies this combo.
    ///
    /// Modifiers are compared by exact equality, not [`Modifiers::contains`]:
    /// a combo for `Cmd+A` does not match a `Cmd+Shift+A` press. This differs
    /// from the pointer modifier fields on [`Keymap`], which the widget tests
    /// with `contains` semantics.
    ///
    /// [`ComboKey::Char`] resolves the physical key to its latin equivalent
    /// via [`Key::to_latin`] so the shortcut fires regardless of keyboard
    /// layout (e.g. a Dvorak or Cyrillic layout), then compares
    /// case-insensitively. When `to_latin` cannot resolve a latin character
    /// (the key has no established Latin-alphabet code, e.g. punctuation
    /// outside the mapped set), this falls back to comparing the logical
    /// [`Key::Character`] text case-insensitively.
    pub fn matches(&self, key: &Key, physical: Physical, modifiers: Modifiers) -> bool {
        if self.modifiers != modifiers {
            return false;
        }

        match self.key {
            ComboKey::Char(target) => match key.to_latin(physical) {
                Some(latin) => latin.eq_ignore_ascii_case(&target),
                None => match key {
                    Key::Character(s) => character_matches(s, target),
                    _ => false,
                },
            },
            ComboKey::Named(named) => matches!(key, Key::Named(n) if *n == named),
        }
    }
}

/// Case-insensitively compares a logical `Key::Character` payload to a single
/// target character. Only single-character payloads can match, since
/// [`ComboKey::Char`] represents one key, not a composed sequence.
fn character_matches(s: &str, target: char) -> bool {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.eq_ignore_ascii_case(&target),
        _ => false,
    }
}

/// Host-configurable bindings for graph-level keyboard and pointer actions.
///
/// Every key binding is optional (or, for [`Keymap::delete_selection`], a
/// list of alternatives) so a host can disable or remap any action; see
/// [`Keymap::none`] for the disable-everything escape hatch. [`Keymap::default`]
/// provides a sensible platform-appropriate starting point.
#[derive(Debug, Clone, PartialEq)]
pub struct Keymap {
    /// Selects every node. `None` disables the shortcut.
    pub select_all: Option<KeyCombo>,
    /// Clears the current selection. `None` disables the shortcut.
    pub clear_selection: Option<KeyCombo>,
    /// Duplicates the selected nodes. `None` disables the shortcut.
    pub clone_selection: Option<KeyCombo>,
    /// Removes the selected nodes. Any combo in this list triggers the
    /// action; an empty list disables the shortcut.
    pub delete_selection: Vec<KeyCombo>,
    /// The pointer button that pans the graph.
    pub pan_button: mouse::Button,
    /// The modifier state that starts an edge-cutting drag.
    ///
    /// Unlike [`KeyCombo`] matching, the widget tests this field with
    /// [`Modifiers::contains`], not exact equality, and checks it before
    /// [`Keymap::multi_select_modifiers`] when both could apply to the same
    /// chord.
    pub edge_cut_modifiers: Modifiers,
    /// The modifier state that extends the current selection instead of
    /// replacing it.
    ///
    /// Unlike [`KeyCombo`] matching, the widget tests this field with
    /// [`Modifiers::contains`], not exact equality, and is checked after
    /// [`Keymap::edge_cut_modifiers`] when both could apply to the same
    /// chord.
    pub multi_select_modifiers: Modifiers,
}

impl Default for Keymap {
    /// Platform-appropriate default bindings.
    ///
    /// On `wasm32`, `clone_selection` uses `Alt+D` instead of `Cmd/Ctrl+D`
    /// because browsers intercept `Cmd/Ctrl+D` at the chrome level (bookmark
    /// the page) before it ever reaches the canvas. `delete_selection` drops
    /// the `Backspace` alternative on `wasm32` because some browsers treat it
    /// as legacy back-navigation outside a text field. Both differences are
    /// unreachable on native, where the shortcuts work as expected.
    fn default() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let clone_selection = Some(KeyCombo::command('d'));
        #[cfg(target_arch = "wasm32")]
        let clone_selection = Some(KeyCombo::alt('d'));

        #[cfg(not(target_arch = "wasm32"))]
        let delete_selection = vec![
            KeyCombo::bare(ComboKey::Named(Named::Delete)),
            KeyCombo::bare(ComboKey::Named(Named::Backspace)),
        ];
        #[cfg(target_arch = "wasm32")]
        let delete_selection = vec![KeyCombo::bare(ComboKey::Named(Named::Delete))];

        Self {
            select_all: Some(KeyCombo::command('a')),
            clear_selection: Some(KeyCombo::bare(ComboKey::Named(Named::Escape))),
            clone_selection,
            delete_selection,
            pan_button: mouse::Button::Right,
            edge_cut_modifiers: Modifiers::COMMAND,
            multi_select_modifiers: Modifiers::SHIFT,
        }
    }
}

impl Keymap {
    /// A keymap with every key binding disabled and pointer fields left at
    /// their platform [`Default`] values.
    ///
    /// Use this as a base when a host wants to opt into shortcuts
    /// individually instead of overriding [`Keymap::default`] piecemeal.
    pub fn none() -> Self {
        Self {
            select_all: None,
            clear_selection: None,
            clone_selection: None,
            delete_selection: Vec::new(),
            ..Self::default()
        }
    }

    /// Resolves a key press to the [`KeyAction`] it triggers, if any.
    ///
    /// Checks bindings in field order (`select_all`, `clear_selection`,
    /// `clone_selection`, then `delete_selection`) and returns the first
    /// match.
    pub fn key_action(
        &self,
        key: &Key,
        physical: Physical,
        modifiers: Modifiers,
    ) -> Option<KeyAction> {
        let hit =
            |combo: Option<KeyCombo>| combo.is_some_and(|c| c.matches(key, physical, modifiers));

        if hit(self.select_all) {
            return Some(KeyAction::SelectAll);
        }
        if hit(self.clear_selection) {
            return Some(KeyAction::ClearSelection);
        }
        if hit(self.clone_selection) {
            return Some(KeyAction::CloneSelection);
        }
        if self
            .delete_selection
            .iter()
            .any(|combo| combo.matches(key, physical, modifiers))
        {
            return Some(KeyAction::DeleteSelection);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::Code;

    #[test]
    fn select_all_resolves_from_default_combo() {
        let keymap = Keymap::default();
        let key = Key::Character("a".into());
        let physical = Physical::Code(Code::KeyA);

        assert_eq!(
            keymap.key_action(&key, physical, Modifiers::COMMAND),
            Some(KeyAction::SelectAll)
        );
    }

    #[test]
    fn clone_selection_resolves_from_default_combo() {
        let keymap = Keymap::default();
        let key = Key::Character("d".into());
        let physical = Physical::Code(Code::KeyD);

        assert_eq!(
            keymap.key_action(&key, physical, Modifiers::COMMAND),
            Some(KeyAction::CloneSelection)
        );
    }

    #[test]
    fn exact_modifier_match_rejects_extra_modifiers() {
        let keymap = Keymap::default();
        let key = Key::Character("a".into());
        let physical = Physical::Code(Code::KeyA);

        // Cmd+Shift+A must not resolve the Cmd+A binding.
        assert_eq!(
            keymap.key_action(&key, physical, Modifiers::COMMAND | Modifiers::SHIFT),
            None
        );
    }

    #[test]
    fn char_combo_is_layout_independent_via_to_latin() {
        // `Key::to_latin` only consults the physical code when the logical
        // character is outside the Latin range (below U+0370 short-circuits
        // to the logical char itself, unchanged) - this is exactly the case
        // for a non-Latin layout (e.g. a Cyrillic keymap) typing over a
        // US/QWERTY physical layout. Simulate that: the logical key is a
        // Cyrillic letter, but the physical key is where "D" lives on a
        // QWERTY board, so `to_latin` resolves it to 'd' regardless of what
        // the Cyrillic letter actually was.
        let keymap = Keymap::default();
        let key = Key::Character("д".into());
        let physical = Physical::Code(Code::KeyD);

        assert_eq!(
            keymap.key_action(&key, physical, Modifiers::COMMAND),
            Some(KeyAction::CloneSelection)
        );
    }

    #[test]
    fn escape_resolves_clear_selection() {
        let keymap = Keymap::default();
        let key = Key::Named(Named::Escape);
        let physical = Physical::Code(Code::Escape);

        assert_eq!(
            keymap.key_action(&key, physical, Modifiers::empty()),
            Some(KeyAction::ClearSelection)
        );
    }

    #[test]
    fn delete_and_backspace_both_resolve_delete_selection() {
        let keymap = Keymap::default();

        let delete = Key::Named(Named::Delete);
        assert_eq!(
            keymap.key_action(&delete, Physical::Code(Code::Delete), Modifiers::empty()),
            Some(KeyAction::DeleteSelection)
        );

        let backspace = Key::Named(Named::Backspace);
        assert_eq!(
            keymap.key_action(
                &backspace,
                Physical::Code(Code::Backspace),
                Modifiers::empty()
            ),
            Some(KeyAction::DeleteSelection)
        );
    }

    #[test]
    fn none_resolves_nothing() {
        let keymap = Keymap::none();

        let a = Key::Character("a".into());
        assert_eq!(
            keymap.key_action(&a, Physical::Code(Code::KeyA), Modifiers::COMMAND),
            None
        );

        let escape = Key::Named(Named::Escape);
        assert_eq!(
            keymap.key_action(&escape, Physical::Code(Code::Escape), Modifiers::empty()),
            None
        );

        let delete = Key::Named(Named::Delete);
        assert_eq!(
            keymap.key_action(&delete, Physical::Code(Code::Delete), Modifiers::empty()),
            None
        );
    }

    #[test]
    fn disabling_one_binding_leaves_the_others_working() {
        let keymap = Keymap {
            select_all: None,
            ..Keymap::default()
        };

        let a = Key::Character("a".into());
        assert_eq!(
            keymap.key_action(&a, Physical::Code(Code::KeyA), Modifiers::COMMAND),
            None
        );

        let escape = Key::Named(Named::Escape);
        assert_eq!(
            keymap.key_action(&escape, Physical::Code(Code::Escape), Modifiers::empty()),
            Some(KeyAction::ClearSelection)
        );
    }
}
