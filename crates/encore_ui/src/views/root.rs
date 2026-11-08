use crate::accounts::AccountsModel;
use crate::components::account::switcher::AccountSwitcher;
use crate::components::layout::{header::Header, sidebar::AppSidebar};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Window,
    base::{h_flex, v_flex},
};

pub struct DesktopRoot {
    sidebar_collapsed: bool,
    accounts: Entity<AccountsModel>,
    account_switcher: Entity<AccountSwitcher>,
}

impl DesktopRoot {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let accounts = cx.new(|_| AccountsModel::default());
        let account_switcher = cx.new(|cx| AccountSwitcher::new(accounts.clone(), cx));
        Self {
            sidebar_collapsed: false,
            accounts,
            account_switcher,
        }
    }

    pub fn accounts(&self) -> &Entity<AccountsModel> {
        &self.accounts
    }
}

impl Render for DesktopRoot {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(Header::new(
                self.sidebar_collapsed,
                cx.listener(|this, _, _, cx| {
                    this.sidebar_collapsed = !this.sidebar_collapsed;
                    cx.notify();
                }),
            ))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(AppSidebar::new(
                        self.sidebar_collapsed,
                        self.account_switcher.clone(),
                    )),
            )
    }
}
