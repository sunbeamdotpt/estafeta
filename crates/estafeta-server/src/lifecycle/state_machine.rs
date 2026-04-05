use std::fmt;

/// Strongly-typed notification lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationState {
    Unseen,
    Unread,
    Read,
    Snoozed,
    Archived,
    Expired,
}

/// Strongly-typed state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    MarkSeen,
    MarkRead,
    MarkUnread,
    Snooze,
    Wake,
    Archive,
    Unarchive,
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
            Self::Unseen => &[Transition::MarkSeen, Transition::Snooze, Transition::Archive, Transition::Expire],
            Self::Unread => &[Transition::MarkRead, Transition::Snooze, Transition::Archive, Transition::Expire],
            Self::Read => &[Transition::MarkUnread, Transition::Archive, Transition::Expire],
            Self::Snoozed => &[Transition::Wake, Transition::Expire],
            Self::Archived => &[Transition::Unarchive],
            Self::Expired => &[Transition::Archive],
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
            Self::Unseen => "unseen",
            Self::Unread => "unread",
            Self::Read => "read",
            Self::Snoozed => "snoozed",
            Self::Archived => "archived",
            Self::Expired => "expired",
        }
    }

    /// Parse from database string.
    pub fn from_db_str(s: &str) -> Result<Self, UnknownState> {
        match s {
            "unseen" => Ok(Self::Unseen),
            "unread" => Ok(Self::Unread),
            "read" => Ok(Self::Read),
            "snoozed" => Ok(Self::Snoozed),
            "archived" => Ok(Self::Archived),
            "expired" => Ok(Self::Expired),
            other => Err(UnknownState(other.to_string())),
        }
    }

    /// Convert to proto enum value.
    pub fn to_proto(self) -> i32 {
        match self {
            Self::Unseen => 1,
            Self::Unread => 2,
            Self::Read => 3,
            Self::Snoozed => 4,
            Self::Archived => 5,
            Self::Expired => 6,
        }
    }

    /// Parse from proto enum value.
    pub fn from_proto(v: i32) -> Result<Self, UnknownState> {
        match v {
            1 => Ok(Self::Unseen),
            2 => Ok(Self::Unread),
            3 => Ok(Self::Read),
            4 => Ok(Self::Snoozed),
            5 => Ok(Self::Archived),
            6 => Ok(Self::Expired),
            other => Err(UnknownState(other.to_string())),
        }
    }
}

impl Transition {
    /// The target state this transition leads to.
    pub fn target(self) -> NotificationState {
        match self {
            Self::MarkSeen => NotificationState::Unread,
            Self::MarkRead => NotificationState::Read,
            Self::MarkUnread => NotificationState::Unread,
            Self::Snooze => NotificationState::Snoozed,
            Self::Wake => NotificationState::Unread,
            Self::Archive => NotificationState::Archived,
            Self::Unarchive => NotificationState::Read,
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
            NotificationState::Unseen.apply(Transition::MarkSeen).unwrap(),
            NotificationState::Unread,
        );
        assert_eq!(
            NotificationState::Unseen.apply(Transition::Snooze).unwrap(),
            NotificationState::Snoozed,
        );
        assert_eq!(
            NotificationState::Unseen.apply(Transition::Archive).unwrap(),
            NotificationState::Archived,
        );
        assert_eq!(
            NotificationState::Unread.apply(Transition::MarkRead).unwrap(),
            NotificationState::Read,
        );
        assert_eq!(
            NotificationState::Unread.apply(Transition::Snooze).unwrap(),
            NotificationState::Snoozed,
        );
        assert_eq!(
            NotificationState::Unread.apply(Transition::Archive).unwrap(),
            NotificationState::Archived,
        );
        assert_eq!(
            NotificationState::Read.apply(Transition::MarkUnread).unwrap(),
            NotificationState::Unread,
        );
        assert_eq!(
            NotificationState::Read.apply(Transition::Archive).unwrap(),
            NotificationState::Archived,
        );
        assert_eq!(
            NotificationState::Snoozed.apply(Transition::Wake).unwrap(),
            NotificationState::Unread,
        );
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(NotificationState::Archived.apply(Transition::MarkRead).is_err());
        assert!(NotificationState::Expired.apply(Transition::MarkUnread).is_err());
        assert!(NotificationState::Read.apply(Transition::Snooze).is_err());
        assert!(NotificationState::Archived.apply(Transition::Wake).is_err());
        assert!(NotificationState::Unseen.apply(Transition::MarkRead).is_err());
        assert!(NotificationState::Unseen.apply(Transition::MarkUnread).is_err());
    }

    #[test]
    fn test_unarchive() {
        assert_eq!(
            NotificationState::Archived.apply(Transition::Unarchive).unwrap(),
            NotificationState::Read,
        );
        // Can't unarchive from non-archived states
        assert!(NotificationState::Read.apply(Transition::Unarchive).is_err());
        assert!(NotificationState::Unseen.apply(Transition::Unarchive).is_err());
    }

    #[test]
    fn test_expired_can_archive() {
        assert_eq!(
            NotificationState::Expired.apply(Transition::Archive).unwrap(),
            NotificationState::Archived,
        );
        // But expired cannot do other transitions
        assert!(NotificationState::Expired.apply(Transition::MarkRead).is_err());
        assert!(NotificationState::Expired.apply(Transition::Expire).is_err());
    }

    #[test]
    fn test_expire_from_non_terminal() {
        assert!(NotificationState::Unseen.apply(Transition::Expire).is_ok());
        assert!(NotificationState::Unread.apply(Transition::Expire).is_ok());
        assert!(NotificationState::Read.apply(Transition::Expire).is_ok());
        assert!(NotificationState::Snoozed.apply(Transition::Expire).is_ok());
        // Archived is no longer terminal, but expire from archived is not valid
        assert!(NotificationState::Archived.apply(Transition::Expire).is_err());
        assert!(NotificationState::Expired.apply(Transition::Expire).is_err());
    }

    #[test]
    fn test_db_roundtrip() {
        for state in [
            NotificationState::Unseen,
            NotificationState::Unread,
            NotificationState::Read,
            NotificationState::Snoozed,
            NotificationState::Archived,
            NotificationState::Expired,
        ] {
            let s = state.as_db_str();
            assert_eq!(NotificationState::from_db_str(s).unwrap(), state);
        }
    }

    #[test]
    fn test_proto_roundtrip() {
        for state in [
            NotificationState::Unseen,
            NotificationState::Unread,
            NotificationState::Read,
            NotificationState::Snoozed,
            NotificationState::Archived,
            NotificationState::Expired,
        ] {
            let v = state.to_proto();
            assert_eq!(NotificationState::from_proto(v).unwrap(), state);
        }
    }

    #[test]
    fn test_can_transition_to() {
        assert!(NotificationState::Unseen.can_transition_to(NotificationState::Unread));
        assert!(NotificationState::Unread.can_transition_to(NotificationState::Read));
        assert!(!NotificationState::Archived.can_transition_to(NotificationState::Archived));
        assert!(NotificationState::Archived.can_transition_to(NotificationState::Read));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", NotificationState::Unseen), "unseen");
        assert_eq!(format!("{}", NotificationState::Unread), "unread");
        assert_eq!(format!("{}", NotificationState::Expired), "expired");
    }
}
