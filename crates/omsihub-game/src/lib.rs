//! Omsi-Hub in the game program (`openomsi`), beside openOMSI's own code. Since the desktop
//! launcher (`launcher/`, a Tauri window) is the only launcher, the game draws no launcher of
//! its own any more; what is left of Omsi-Hub's game-drawn launcher is here, without a window:
//!
//! * in the game: the duty in the navigator (`nav_duty`), the bus stop signs on its maps
//!   (`stop_signs`) and the loading screen with openOMSI's mark (`splash`, `intro`);
//! * for the desktop launcher's editors, through the launcher service (`openomsi
//!   --launcher-service`, the game's `launcher::service`, which hands what it does not know to
//!   `service::serve`): the line editor kept here while its page draws it (`lineeditor`), the
//!   livery studio drawn here and shown on its page (`livery`, `studio`), a text on a bus's own
//!   destination display (`displayfont`) and the bus photos of the bus step and the company
//!   (`busphoto`, the game's showroom).
//!
//! What only the game program has (its renderer's showroom, the city map read the way the
//! navigator reads it, machine translation, the interface's language) is asked through
//! `host::Host`, which the program installs first. The toolkit the studio and the loading
//! screen are drawn with is omsi-ui's (`kit`, `theme`, `accent`).

pub mod busclass;
mod busoptions;
pub mod busphoto;
pub mod displayfont;
pub mod host;
pub mod intro;
pub mod lineeditor;
mod lineroute;
mod words;
pub mod livery;
pub mod nav_duty;
pub mod service;
pub mod showroom;
pub mod spawn;
pub mod splash;
pub mod stop_signs;
mod editor_host;

pub use omsi_ui::{accent, kit as ui, theme};

// The tests read the game's translation tables as the game does (the modules here ask
// `omsi_ui::tr`, whose lookup the program installs): that every text is translated.
#[cfg(test)]
rust_i18n::i18n!("../omsi-app/locales");

/// The tests' interface in `code` (the settings' ENG / DEU ...): the game's tables, read here
/// (`i18n!` above). The program sets its own (`host::Host::ui_language`).
#[cfg(test)]
pub(crate) fn ui_language(code: &str) {
    omsi_ui::i18n::set_lookup(|lang, text| crate::_rust_i18n_try_translate(lang, text).map(|t| t.into_owned()));
    omsi_ui::i18n::set_language(omsi_launcher_lib::language_iso(code));
}

/// The interface's machine translation (the program's `mt`): texts it must leave alone.
pub(crate) mod mt {
    pub(crate) fn protect<'a>(texts: impl IntoIterator<Item = &'a str>) {
        let mut it = texts.into_iter();
        crate::host::host().mt_protect(&mut it);
    }
}

#[cfg(test)]
mod tests {
    use omsi_ui::accent::{hex, parse_hex, DEFAULT, PRESETS};

    /// The accent the settings file keeps (`omsi-launcher-core`'s `accent_text`) is read back as
    /// it was chosen (`omsi_ui::accent`).
    #[test]
    fn the_accent_setting_is_written_as_it_is_read() {
        for (_, c) in PRESETS {
            assert_eq!(parse_hex(&omsi_launcher_lib::accent_text(&hex(c))), Some(c));
        }
        assert_eq!(omsi_launcher_lib::accent_text("nonsense"), hex(DEFAULT));
    }
}
