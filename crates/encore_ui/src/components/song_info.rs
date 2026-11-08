use gpui_kit::{
    App, FontWeight, IntoElement, ObjectFit, ParentElement as _, RenderOnce, SharedString,
    Styled as _, StyledImage as _, Window,
    assets::IconName,
    base::v_flex,
    component::{ActiveTheme as _, Icon},
    div, img,
    prelude::FluentBuilder as _,
    px,
};

const POSTER_SIZE: f32 = 220.;

#[derive(IntoElement)]
pub struct SongInfoCard {
    title: SharedString,
    artist: Option<SharedString>,
    album: Option<SharedString>,
    poster_url: Option<SharedString>,
    credits: Vec<SharedString>,
}

impl SongInfoCard {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            artist: None,
            album: None,
            poster_url: None,
            credits: Vec::new(),
        }
    }

    pub fn artist(mut self, artist: impl Into<SharedString>) -> Self {
        self.artist = Some(artist.into());
        self
    }

    pub fn album(mut self, album: impl Into<SharedString>) -> Self {
        self.album = Some(album.into());
        self
    }

    pub fn poster(mut self, url: impl Into<SharedString>) -> Self {
        self.poster_url = Some(url.into());
        self
    }

    pub fn credits(mut self, credits: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.credits = credits.into_iter().map(Into::into).collect();
        self
    }
}

impl RenderOnce for SongInfoCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            title,
            artist,
            album,
            poster_url,
            credits,
        } = self;

        let theme = cx.theme();
        let foreground = theme.foreground;
        let muted = theme.muted_foreground;
        let track_bg = theme.muted;
        let border = theme.border;

        let poster = div()
            .size(px(POSTER_SIZE))
            .rounded(px(8.))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .bg(track_bg)
            .map(|this| match poster_url {
                Some(url) => {
                    this.child(img(url).size(px(POSTER_SIZE)).object_fit(ObjectFit::Cover))
                }
                None => this.child(Icon::new(IconName::Music).size_8().text_color(muted)),
            });

        v_flex()
            .size_full()
            .items_center()
            .gap_4()
            .child(poster)
            .child(
                v_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(foreground)
                            .child(title),
                    )
                    .when_some(artist, |this, artist| {
                        this.child(div().text_sm().text_color(muted).child(artist))
                    })
                    .when_some(album, |this, album| {
                        this.child(div().text_sm().text_color(muted).child(album))
                    }),
            )
            .when(!credits.is_empty(), |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .gap_2()
                        .pt_4()
                        .border_t_1()
                        .border_color(border)
                        .children(
                            credits
                                .into_iter()
                                .map(|line| div().text_sm().text_color(muted).child(line)),
                        ),
                )
            })
    }
}
