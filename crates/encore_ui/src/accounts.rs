use gpui_kit::{Context, EventEmitter, SharedString};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    pub id: SharedString,
    pub name: SharedString,
    pub detail: Option<SharedString>,
    pub avatar_url: Option<SharedString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccountStatus {
    #[default]
    Idle,
    SigningIn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountsEvent {
    SignInRequested,
    SwitchRequested(SharedString),
    SignOutRequested,
}

#[derive(Default)]
pub struct AccountsModel {
    accounts: Vec<AccountSummary>,
    active: Option<SharedString>,
    status: AccountStatus,
}

impl EventEmitter<AccountsEvent> for AccountsModel {}

impl AccountsModel {
    pub fn accounts(&self) -> &[AccountSummary] {
        &self.accounts
    }

    pub fn active(&self) -> Option<&AccountSummary> {
        let id = self.active.as_ref()?;
        self.accounts.iter().find(|account| &account.id == id)
    }

    pub fn status(&self) -> AccountStatus {
        self.status
    }

    pub fn set_accounts(
        &mut self,
        accounts: Vec<AccountSummary>,
        active: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.accounts = accounts;
        self.active = active;
        cx.notify();
    }

    pub fn set_status(&mut self, status: AccountStatus, cx: &mut Context<Self>) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    pub fn request_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = self.sign_in_event() {
            cx.emit(event);
        }
    }

    pub fn request_switch(&mut self, id: SharedString, cx: &mut Context<Self>) {
        if let Some(event) = self.switch_event(id) {
            cx.emit(event);
        }
    }

    pub fn request_sign_out(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = self.sign_out_event() {
            cx.emit(event);
        }
    }

    fn sign_in_event(&self) -> Option<AccountsEvent> {
        (self.status == AccountStatus::Idle).then_some(AccountsEvent::SignInRequested)
    }

    fn switch_event(&self, id: SharedString) -> Option<AccountsEvent> {
        let known = self.accounts.iter().any(|account| account.id == id);
        let already_active = self.active.as_ref() == Some(&id);
        (known && !already_active && self.status == AccountStatus::Idle)
            .then_some(AccountsEvent::SwitchRequested(id))
    }

    fn sign_out_event(&self) -> Option<AccountsEvent> {
        self.active().map(|_| AccountsEvent::SignOutRequested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str) -> AccountSummary {
        AccountSummary {
            id: id.to_owned().into(),
            name: id.to_owned().into(),
            detail: None,
            avatar_url: None,
        }
    }

    fn model(ids: &[&str], active: Option<&str>, status: AccountStatus) -> AccountsModel {
        AccountsModel {
            accounts: ids.iter().map(|id| account(id)).collect(),
            active: active.map(|id| id.to_owned().into()),
            status,
        }
    }

    fn id(raw: &str) -> SharedString {
        raw.to_owned().into()
    }

    #[test]
    fn a_fresh_model_is_signed_out_and_idle() {
        let model = AccountsModel::default();

        assert!(model.accounts().is_empty());
        assert!(model.active().is_none());
        assert_eq!(model.status(), AccountStatus::Idle);
    }

    #[test]
    fn the_active_account_is_looked_up_among_the_saved_ones() {
        let model = model(&["a", "b"], Some("b"), AccountStatus::Idle);

        assert_eq!(
            model.active().map(|account| account.id.clone()),
            Some(id("b"))
        );
    }

    #[test]
    fn an_active_id_with_no_matching_account_counts_as_signed_out() {
        let model = model(&["a"], Some("ghost"), AccountStatus::Idle);

        assert!(model.active().is_none());
    }

    #[test]
    fn signing_in_is_requested_only_while_idle() {
        let idle = model(&[], None, AccountStatus::Idle);
        let busy = model(&[], None, AccountStatus::SigningIn);

        assert_eq!(idle.sign_in_event(), Some(AccountsEvent::SignInRequested));
        assert_eq!(busy.sign_in_event(), None);
    }

    #[test]
    fn switching_to_another_saved_account_is_requested() {
        let model = model(&["a", "b"], Some("a"), AccountStatus::Idle);

        assert_eq!(
            model.switch_event(id("b")),
            Some(AccountsEvent::SwitchRequested(id("b")))
        );
    }

    #[test]
    fn switching_to_the_active_or_an_unknown_account_is_ignored() {
        let model = model(&["a", "b"], Some("a"), AccountStatus::Idle);

        assert_eq!(model.switch_event(id("a")), None);
        assert_eq!(model.switch_event(id("ghost")), None);
    }

    #[test]
    fn switching_is_ignored_while_a_sign_in_is_running() {
        let model = model(&["a", "b"], Some("a"), AccountStatus::SigningIn);

        assert_eq!(model.switch_event(id("b")), None);
    }

    #[test]
    fn signing_out_needs_an_active_account() {
        let signed_in = model(&["a"], Some("a"), AccountStatus::Idle);
        let signed_out = model(&["a"], None, AccountStatus::Idle);

        assert_eq!(
            signed_in.sign_out_event(),
            Some(AccountsEvent::SignOutRequested)
        );
        assert_eq!(signed_out.sign_out_event(), None);
    }
}
