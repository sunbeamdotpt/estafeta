use std::fmt;

/// Strongly-typed notification lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationState {
    Unread,
    Read,
    Snoozed,
    Dismissed,
    Expired,
}

/// Strongly-typed state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    MarkRead,
    MarkUnread,
    Snooze,
    Wake,
    Dismiss,
    Expire,
}

/// Error returned when attempting a disallowed state transition.
#[derive(Debug, thiserror::Error)]
#[error("invalid state transition: {from} -> {to}")]
pub struct InvalidTransition {
    pub from: NotificationState,
    pub to: NotificationState,
}

/// Error returned when parsing an unrecognized state string or proto value.
#[derive(Debug, thiserror::Error)]
#[error("unknown notification state: '{0}'")]
pub struct UnknownState(pub String);

impl NotificationState {
    /// Returns which transitions are valid from this state.
    pub fn valid_transitions(self) -> &'static [Transition] {
        match self {
            Self::Unread => &[Transition::MarkRead, Transition::Snooze, Transition::Dismiss, Transition::Expire],
            Self::Read => &[Transition::MarkUnread, Transition::Dismiss, Transition::Expire],
            Self::Snoozed => &[Transition::Wake, Transition::Expire],
            Self::Dismissed => &[],
            Self::Expired => &[],
        }
    }

    /// Apply a transition, returning the new state or an error.
    pub fn apply(self, transition: Transition) -> Result<Self, InvalidTransition> {
        let target = transition.target();
        if self.valid_transitions().contains(&transition) {
            Ok(target)
        } else {
            Err(InvalidTransition { from: self, to: target })
        }
    }

    /// Check if transitioning directly to a target state is valid.
    pub fn can_transition_to(self, target: Self) -> bool {
        self.valid_transitions()
            .iter()
            .any(|t| t.target() == target)
    }

    /// The database string representation.
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Read => "read",
            Self::Snoozed => "snoozed",
            Self::Dismissed => "dismissed",
            Self::Expired => "expired",
        }
    }

    /// Parse from database string.
    pub fn from_db_str(s: &str) -> Result<Self, UnknownState> {
        match s {
            "unread" => Ok(Self::Unread),
            "read" => Ok(Self::Read),
            "snoozed" => Ok(Self::Snoozed),
            "dismissed" => Ok(Self::Dismissed),
            "expired" => Ok(Self::Expired),
            other => Err(UnknownState(other.to_string())),
        }
    }

    /// Convert to proto enum value.
    pub fn to_proto(self) -> i32 {
        match self {
            Self::Unread => 1,
            Self::Read => 2,
            Self::Snoozed => 3,
            Self::Dismissed => 4,
            Self::Expired => 5,
        }
    }

    /// Parse from proto enum value.
    pub fn from_proto(v: i32) -> Result<Self, UnknownState> {
        match v {
            1 => Ok(Self::Unread),
            2 => Ok(Self::Read),
            3 => Ok(Self::Snoozed),
            4 => Ok(Self::Dismissed),
            5 => Ok(Self::Expired),
            other => Err(UnknownState(other.to_string())),
        }
    }
}

impl Transition {
    /// The target state this transition leads to.
    pub fn target(self) -> NotificationState {
        match self {
            Self::MarkRead => NotificationState::Read,
            Self::MarkUnread => NotificationState::Unread,
            Self::Snooze => NotificationState::Snoozed,
            Self::Wake => NotificationState::Unread,
            Self::Dismiss => NotificationState::Dismissed,
            Self::Expire => NotificationState::Expired,
        }
    }
}

impl fmt::Display for NotificationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_db_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        assert_eq!(
            NotificationState::Unread.apply(Transition::MarkRead).unwrap(),
            NotificationState::Read,
        );
        assert_eq!(
            NotificationState::Unread.apply(Transition::Snooze).unwrap(),
            NotificationState::Snoozed,
        );
        assert_eq!(
            NotificationState::Unread.apply(Transition::Dismiss).unwrap(),
            NotificationState::Dismissed,
        );
        assert_eq!(
            NotificationState::Read.apply(Transition::MarkUnread).unwrap(),
            NotificationState::Unread,
        );
        assert_eq!(
            NotificationState::Read.apply(Transition::Dismiss).unwrap(),
            NotificationState::Dismissed,
        );
        assert_eq!(
            NotificationState::Snoozed.apply(Transition::Wake).unwrap(),
            NotificationState::Unread,
        );
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(NotificationState::Dismissed.apply(Transition::MarkRead).is_err());
        assert!(NotificationState::Expired.apply(Transition::MarkUnread).is_err());
        assert!(NotificationState::Read.apply(Transition::Snooze).is_err());
        assert!(NotificationState::Dismissed.apply(Transition::Wake).is_err());
    }

    #[test]
    fn test_expire_from_all_non_terminal() {
        assert!(NotificationState::Unread.apply(Transition::Expire).is_ok());
        assert!(NotificationState::Read.apply(Transition::Expire).is_ok());
        assert!(NotificationState::Snoozed.apply(Transition::Expire).is_ok());
        assert!(NotificationState::Dismissed.apply(Transition::Expire).is_err());
        assert!(NotificationState::Expired.apply(Transition::Expire).is_err());
    }

    #[test]
    fn test_db_roundtrip() {
        for state in [
            NotificationState::Unread,
            NotificationState::Read,
            NotificationState::Snoozed,
            NotificationState::Dismissed,
            NotificationState::Expired,
        ] {
            let s = state.as_db_str();
            assert_eq!(NotificationState::from_db_str(s).unwrap(), state);
        }
    }

    #[test]
    fn test_proto_roundtrip() {
        for state in [
            NotificationState::Unread,
            NotificationState::Read,
            NotificationState::Snoozed,
            NotificationState::Dismissed,
            NotificationState::Expired,
        ] {
            let v = state.to_proto();
            assert_eq!(NotificationState::from_proto(v).unwrap(), state);
        }
    }

    #[test]
    fn test_can_transition_to() {
        assert!(NotificationState::Unread.can_transition_to(NotificationState::Read));
        assert!(!NotificationState::Dismissed.can_transition_to(NotificationState::Read));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", NotificationState::Unread), "unread");
        assert_eq!(format!("{}", NotificationState::Expired), "expired");
    }
}
