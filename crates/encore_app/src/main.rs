use anyhow::Context;
use gpui_kit::{component::TitleBar, *};

fn main() {
    application()
        .with_assets(encore_ui::EncoreAssets)
        .run(|cx| {
            if let Err(err) = start(cx) {
                eprintln!("encore: {err:#}");
                cx.quit();
            }
        });
}

fn start(cx: &mut App) -> anyhow::Result<()> {
    init(cx);
    let options = WindowOptions {
        #[cfg(target_os = "linux")]
        window_background: WindowBackgroundAppearance::Transparent,
        #[cfg(target_os = "linux")]
        window_decorations: Some(WindowDecorations::Client),
        ..TitleBar::window_options()
    };

    open_window(options, cx, |window, cx| {
        cx.new(|cx| encore_ui::DesktopRoot::new(window, cx))
    })
    .context("failed to open window")?;
    Ok(())
}
