use gpui_kit::{
    App, ClickEvent, Context, Entity, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Subscription, Window, base::v_flex,
};

use crate::components::sidebar::playlist_item::SidebarPlaylistItem;
use crate::playlists::{PlaylistSummary, PlaylistsModel};

pub struct PlaylistsPanel {
    model: Entity<PlaylistsModel>,
    _observe: Subscription,
}

impl PlaylistsPanel {
    pub fn new(model: Entity<PlaylistsModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&model, |_, _, cx| cx.notify());
        Self {
            model,
            _observe: observe,
        }
    }
}

impl Render for PlaylistsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.model.clone();
        let state = model.read(cx);
        let playlists = state.playlists().to_vec();
        let active = state.active().cloned();

        v_flex().w_full().min_w_0().gap_1().children(
            playlists
                .into_iter()
                .map(|playlist| render_row(playlist, active.as_ref(), &model)),
        )
    }
}

fn render_row(
    playlist: PlaylistSummary,
    active: Option<&SharedString>,
    model: &Entity<PlaylistsModel>,
) -> impl IntoElement {
    let id = playlist.id.clone();
    let model = model.clone();
    let item = SidebarPlaylistItem::new(
        SharedString::from(format!("sidebar-playlist-{id}")),
        playlist.title,
    )
    .active(active == Some(&playlist.id));

    let subtitle = playlist.owner.or_else(|| {
        playlist
            .track_count
            .map(|count| SharedString::from(format!("{count} songs")))
    });
    let item = match playlist.thumbnail_url {
        Some(url) => item.thumbnail(url),
        None => item,
    };
    let item = match subtitle {
        Some(subtitle) => item.subtitle(subtitle),
        None => item,
    };

    item.on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        model.update(cx, |model, cx| model.select(id.clone(), cx));
    })
}
