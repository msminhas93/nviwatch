//! Closed-set keybind aggregates: several physical keys map to one intent.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::fmt;

/// Navigation intents that can be bound to more than one physical key.
///
/// Matching lives here so main only dispatches on the aggregate, not on every
/// KeyCode synonym (arrows, hjkl, Ctrl+n/p, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindAggregate {
    Up,
    Down,
    Left,
    Right,
}

/// Key event did not map to a navigation aggregate (normal for chords / modes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnboundKey {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl fmt::Display for UnboundKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "key code {:?} with modifiers {:?} is not a KeybindAggregate navigation binding",
            self.code, self.modifiers
        )
    }
}

impl std::error::Error for UnboundKey {}

impl TryFrom<&KeyEvent> for KeybindAggregate {
    type Error = UnboundKey;

    fn try_from(key: &KeyEvent) -> Result<Self, Self::Error> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // Ignore other modifiers (shift/alt) for these aliases so e.g. Shift+j
        // does not steal Down from plain j; capital G is handled separately.
        match key.code {
            KeyCode::Up => Ok(Self::Up),
            KeyCode::Char('k') if !ctrl => Ok(Self::Up),
            KeyCode::Char('p') if ctrl => Ok(Self::Up),

            KeyCode::Down => Ok(Self::Down),
            KeyCode::Char('j') if !ctrl => Ok(Self::Down),
            KeyCode::Char('n') if ctrl => Ok(Self::Down),

            KeyCode::Left => Ok(Self::Left),
            KeyCode::Char('h') if !ctrl => Ok(Self::Left),

            KeyCode::Right => Ok(Self::Right),
            KeyCode::Char('l') if !ctrl => Ok(Self::Right),

            _ => Err(UnboundKey {
                code: key.code,
                modifiers: key.modifiers,
            }),
        }
    }
}

/// In-progress multi-key chords (`gg`, `dd`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PendingOp {
    #[default]
    None,
    /// First `g` seen; next `g` jumps to top of process list.
    GoTop,
    /// First `d` seen; next `d` kills the selected process.
    Kill,
}

impl PendingOp {
    pub fn is_pending(self) -> bool {
        self != Self::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::empty(),
        }
    }

    #[test]
    fn up_set_matches_arrow_k_and_ctrl_p() {
        assert_eq!(
            KeybindAggregate::try_from(&key(KeyCode::Up, KeyModifiers::NONE)),
            Ok(KeybindAggregate::Up)
        );
        assert_eq!(
            KeybindAggregate::try_from(&key(KeyCode::Char('k'), KeyModifiers::NONE)),
            Ok(KeybindAggregate::Up)
        );
        assert_eq!(
            KeybindAggregate::try_from(&key(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            Ok(KeybindAggregate::Up)
        );
    }

    #[test]
    fn down_set_matches_arrow_j_and_ctrl_n() {
        assert_eq!(
            KeybindAggregate::try_from(&key(KeyCode::Down, KeyModifiers::NONE)),
            Ok(KeybindAggregate::Down)
        );
        assert_eq!(
            KeybindAggregate::try_from(&key(KeyCode::Char('j'), KeyModifiers::NONE)),
            Ok(KeybindAggregate::Down)
        );
        assert_eq!(
            KeybindAggregate::try_from(&key(KeyCode::Char('n'), KeyModifiers::CONTROL)),
            Ok(KeybindAggregate::Down)
        );
    }
}
