use libmpv2::Mpv;

use crate::PlayerError;

pub(crate) fn configure(mpv: &Mpv) -> Result<(), PlayerError> {
    mpv.set_property("vid", "no")?;
    mpv.set_property("ytdl", "no")?;
    mpv.set_property("demuxer", "lavf")?;
    mpv.set_property("gapless-audio", "yes")?;
    mpv.set_property("prefetch-playlist", "yes")?;
    mpv.set_property("cache", "yes")?;
    mpv.set_property("audio-client-name", "Encore")?;
    mpv.set_property("title", "Encore")?;
    mpv.set_property("volume-max", 150)?;
    mpv.set_property(
        "stream-lavf-o",
        "reconnect=1,reconnect_streamed=1,reconnect_on_network_error=1,\
         reconnect_delay_max=2,reconnect_max_retries=6,reconnect_delay_total_max=30,\
         short_seek_size=1",
    )?;
    Ok(())
}
