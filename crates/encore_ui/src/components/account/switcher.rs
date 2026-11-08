use gpui_kit::{
    AnyElement, Context, Entity, FontWeight, Hsla, IntoElement, ParentElement as _, Pixels, Render,
    SharedString, Styled as _, Subscription, Window,
    assets::IconName,
    base::{h_flex, v_flex},
    component::{
        ActiveTheme as _, Disableable as _, Icon, Side, Sizable as _,
        avatar::Avatar,
        button::{Button, ButtonVariants as _},
        menu::{DropdownMenu as _, PopupMenu, PopupMenuItem},
    },
    div,
    prelude::FluentBuilder as _,
    px,
};

use crate::{
    accounts::{AccountStatus, AccountSummary, AccountsModel},
    components::marquee::Marquee,
};

const TRIGGER_ID: &str = "account-switcher";
const TRIGGER_HEIGHT: f32 = 48.;
const HEADER_AVATAR: f32 = 32.;
const ROW_AVATAR: f32 = 28.;
const MENU_MIN_WIDTH: f32 = 232.;

pub struct AccountSwitcher {
    model: Entity<AccountsModel>,
    _observe: Subscription,
}

impl AccountSwitcher {
    pub fn new(model: Entity<AccountsModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            _observe: observe,
        }
    }
}

impl Render for AccountSwitcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let model = self.model.clone();
        let state = model.read(cx);
        let accounts = state.accounts().to_vec();
        let active = state.active().cloned();
        let signing_in = state.status() == AccountStatus::SigningIn;

        let trigger = match active {
            Some(active) => {
                signed_in(model, accounts, active, signing_in, muted).into_any_element()
            }
            None => signed_out(model, accounts, signing_in, muted),
        };
        div().w_full().min_w_0().child(trigger)
    }
}

fn signed_in(
    model: Entity<AccountsModel>,
    accounts: Vec<AccountSummary>,
    active: AccountSummary,
    signing_in: bool,
    muted: Hsla,
) -> impl IntoElement {
    let row = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(avatar(&active, px(HEADER_AVATAR)))
        .child(identity(
            "account-switcher-active",
            &active.name,
            active.detail.as_ref(),
            muted,
        ))
        .child(
            Icon::new(IconName::ChevronsUpDown)
                .size_4()
                .text_color(muted),
        );

    trigger().child(row).dropdown_menu(move |menu, _, _| {
        account_menu(menu, &accounts, Some(&active.id), signing_in, &model, muted)
    })
}

fn signed_out(
    model: Entity<AccountsModel>,
    accounts: Vec<AccountSummary>,
    signing_in: bool,
    muted: Hsla,
) -> AnyElement {
    let title = SharedString::from(match signing_in {
        true => "Signing in…",
        false => "Sign in",
    });
    let detail = SharedString::from(match accounts.len() {
        0 => "YouTube Music".to_owned(),
        1 => "1 saved account".to_owned(),
        count => format!("{count} saved accounts"),
    });
    let icon = match accounts.is_empty() {
        true => IconName::LogIn,
        false => IconName::ChevronsUpDown,
    };
    let row = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(Avatar::new().with_size(px(HEADER_AVATAR)))
        .child(identity(
            "account-switcher-signed-out",
            &title,
            Some(&detail),
            muted,
        ))
        .child(Icon::new(icon).size_4().text_color(muted));

    match accounts.is_empty() {
        true => trigger()
            .child(row)
            .disabled(signing_in)
            .on_click(move |_, _, cx| {
                model.update(cx, |model, cx| model.request_sign_in(cx));
            })
            .into_any_element(),
        false => trigger()
            .child(row)
            .dropdown_menu(move |menu, _, _| {
                account_menu(menu, &accounts, None, signing_in, &model, muted)
            })
            .into_any_element(),
    }
}

fn trigger() -> Button {
    Button::new(TRIGGER_ID)
        .cursor_pointer()
        .ghost()
        .w_full()
        .h(px(TRIGGER_HEIGHT))
        .px_2()
}

fn account_menu(
    menu: PopupMenu,
    accounts: &[AccountSummary],
    active: Option<&SharedString>,
    signing_in: bool,
    model: &Entity<AccountsModel>,
    muted: Hsla,
) -> PopupMenu {
    let menu = menu
        .min_w(px(MENU_MIN_WIDTH))
        .check_side(Side::Right)
        .label("Accounts");

    let menu = accounts.iter().fold(menu, |menu, account| {
        let id = account.id.clone();
        let row = account.clone();
        let model = model.clone();
        menu.item(
            PopupMenuItem::element(move |_, _| account_row(&row, muted))
                .checked(active == Some(&account.id))
                .on_click(move |_, _, cx| {
                    model.update(cx, |model, cx| model.request_switch(id.clone(), cx));
                }),
        )
    });

    let add = {
        let model = model.clone();
        PopupMenuItem::new("Add account")
            .icon(IconName::UserPlus)
            .disabled(signing_in)
            .on_click(move |_, _, cx| {
                model.update(cx, |model, cx| model.request_sign_in(cx));
            })
    };
    let menu = menu.separator().item(add);
    match active {
        Some(_) => {
            let model = model.clone();
            menu.item(
                PopupMenuItem::new("Sign out")
                    .icon(IconName::LogOut)
                    .on_click(move |_, _, cx| {
                        model.update(cx, |model, cx| model.request_sign_out(cx));
                    }),
            )
        }
        None => menu,
    }
}

fn account_row(account: &AccountSummary, muted: Hsla) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(avatar(account, px(ROW_AVATAR)))
        .child(identity(
            SharedString::from(format!("account-switcher-row-{}", account.id)),
            &account.name,
            account.detail.as_ref(),
            muted,
        ))
        .into_any_element()
}

fn identity(
    id: impl Into<SharedString>,
    name: &SharedString,
    detail: Option<&SharedString>,
    muted: Hsla,
) -> AnyElement {
    let id = id.into();
    v_flex()
        .flex_1()
        .min_w_0()
        .text_left()
        .child(Marquee::new(
            format!("{id}-name"),
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(name.clone()),
        ))
        .when_some(detail.cloned(), |this, detail| {
            this.child(Marquee::new(
                format!("{id}-detail"),
                div().text_xs().text_color(muted).child(detail),
            ))
        })
        .into_any_element()
}

fn avatar(account: &AccountSummary, size: Pixels) -> Avatar {
    let avatar = Avatar::new().name(account.name.clone()).with_size(size);
    match &account.avatar_url {
        Some(url) => avatar.src(url.clone()),
        None => avatar,
    }
}
