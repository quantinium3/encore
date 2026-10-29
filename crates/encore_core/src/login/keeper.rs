use super::Session;
use crate::AccountId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Refreshed {
        account: AccountId,
        session: Session,
    },
    SignedOut {
        account: AccountId,
    },
}

pub trait SessionKeeper {
    fn start(&self, account: &AccountId, session: &Session);
    fn stop(&self);
}

impl SessionKeeper for () {
    fn start(&self, _: &AccountId, _: &Session) {}

    fn stop(&self) {}
}
