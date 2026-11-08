mod accounts;
mod paths;
mod playlists;

use std::sync::Arc;

use anyhow::Context;
use gpui_kit::{component::TitleBar, *};

fn main() {
    let app = application().with_assets(encore_ui::EncoreAssets);
    let app = match reqwest_client::ReqwestClient::user_agent(concat!(
        "Encore/",
        env!("CARGO_PKG_VERSION")
    )) {
        Ok(client) => app.with_http_client(Arc::new(client)),
        Err(err) => {
            eprintln!("encore: remote images are disabled: {err:#}");
            app
        }
    };

    app.run(|cx| {
        if let Err(err) = start(cx) {
            eprintln!("encore: {err:#}");
            cx.quit();
        }
    });
}

fn start(cx: &mut App) -> anyhow::Result<()> {
    init(cx);
    let data_dir = paths::data_dir()?;
    let accounts_wiring = accounts::build(&data_dir)?;
    let playlists_wiring = playlists::build(&data_dir)?;
    let options = WindowOptions {
        #[cfg(target_os = "linux")]
        window_background: WindowBackgroundAppearance::Transparent,
        #[cfg(target_os = "linux")]
        window_decorations: Some(WindowDecorations::Client),
        ..TitleBar::window_options()
    };

    open_window(options, cx, move |window, cx| {
        let root = cx.new(|cx| encore_ui::DesktopRoot::new(window, cx));
        let accounts_model = root.read(cx).accounts().clone();
        let playlists_model = root.read(cx).playlists().clone();
        accounts::attach(accounts_wiring, &accounts_model, window, cx);
        playlists::attach(
            playlists_wiring,
            &accounts_model,
            &playlists_model,
            window,
            cx,
        );
        root
    })
    .context("failed to open window")?;
    Ok(())
}
