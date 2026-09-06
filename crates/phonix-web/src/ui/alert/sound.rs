//! The noise an alert makes, for the people who switched sounds on.
//!
//! One tone per outcome, taken from the same [`Tone`] the alert already draws
//! itself from, so a chime can never disagree with the colour beside it.
//! Silence is the default and the preference is [`Sounds`], per person.

use crate::components::page::Tone;

/// Served from `public/phonix-assets/sounds/`.
const SUCCESS: &str = "/phonix-assets/sounds/successeffulysubmitted.mp3";
const WARNING: &str = "/phonix-assets/sounds/warning.mp3";
const ERROR: &str = "/phonix-assets/sounds/error.mp3";

/// Loud enough to be heard beside a keyboard, quiet enough for an open office.
const VOLUME: f64 = 0.4;

/// The file this tone plays, or `None` where an outcome is neither good nor bad.
pub const fn chime(tone: Tone) -> Option<&'static str> {
    match tone {
        Tone::Success => Some(SUCCESS),
        Tone::Warning => Some(WARNING),
        Tone::Danger => Some(ERROR),
        Tone::Neutral | Tone::Brand => None,
    }
}

/// Play the tone for an alert, if this person asked for sound and this tone has
/// one.
#[cfg(feature = "hydrate")]
pub fn play(tone: Tone) {
    if !enabled() {
        return;
    }

    let Some(src) = chime(tone) else {
        return;
    };

    let Ok(audio) = web_sys::HtmlAudioElement::new_with_src(src) else {
        return;
    };

    audio.set_volume(VOLUME);

    // A browser that has seen no gesture yet rejects the promise. That is a
    // normal outcome, not a fault, so it is awaited and dropped rather than
    // left to surface as an unhandled rejection in the console.
    if let Ok(started) = audio.play() {
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(started).await;
        });
    }
}

#[cfg(not(feature = "hydrate"))]
pub fn play(_tone: Tone) {
    let _ = VOLUME;
}

/// Whether the viewer has sound on.
///
/// Untracked: posting an alert must not make its caller reactive over a
/// preference, and the preference cannot change between the post and the play.
#[cfg(feature = "hydrate")]
fn enabled() -> bool {
    crate::theme::Theme::get().sounds_untracked().is_on()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Sounds;

    #[test]
    fn only_the_three_outcomes_that_mean_something_have_a_tone() {
        assert_eq!(chime(Tone::Success), Some(SUCCESS));
        assert_eq!(chime(Tone::Warning), Some(WARNING));
        assert_eq!(chime(Tone::Danger), Some(ERROR));
        assert_eq!(chime(Tone::Neutral), None);
        assert_eq!(chime(Tone::Brand), None);
    }

    #[test]
    fn every_tone_points_at_the_asset_directory() {
        for tone in [Tone::Success, Tone::Warning, Tone::Danger] {
            let src = chime(tone).unwrap();
            assert!(src.starts_with("/phonix-assets/sounds/"), "{tone:?}");
            assert!(src.ends_with(".mp3"), "{tone:?}");
        }
    }

    #[test]
    fn nothing_plays_until_somebody_turns_it_on() {
        assert!(!Sounds::default().is_on());
        assert!(Sounds::default().toggled().is_on());
    }
}
