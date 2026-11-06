//! Translation helpers. The message ids are the ones the Python version
//! used, so the existing translations keep working.

pub use gettextrs::gettext;

/// Translates the form of a message that belongs to `count` and fills that
/// number in for `{}`.
pub fn ngettext_f(singular: &str, plural: &str, count: u32) -> String {
    gettextrs::ngettext(singular, plural, count).replacen("{}", &count.to_string(), 1)
}

/// Translates `msgid` and fills in its positional placeholders: `{}` takes
/// the first argument, `{0}`, `{1}`, … the argument with that index.
pub fn gettext_f(msgid: &str, args: &[&str]) -> String {
    let mut text = gettext(msgid);
    if let Some(first) = args.first() {
        text = text.replacen("{}", first, 1);
    }
    for (index, arg) in args.iter().enumerate() {
        text = text.replace(&format!("{{{index}}}"), arg);
    }
    text
}

/// The name of a language in the language of the user, for its three-letter
/// code; the code itself for one that is not known here.
///
/// The names are translated by iso-codes, which every desktop ships.
pub fn language_name(code: &str) -> String {
    let english = match code {
        "eng" => "English",
        "nld" => "Dutch",
        "deu" => "German",
        "fra" => "French",
        "spa" => "Spanish",
        "ita" => "Italian",
        "por" => "Portuguese",
        "swe" => "Swedish",
        "dan" => "Danish",
        "nor" => "Norwegian",
        "fin" => "Finnish",
        "pol" => "Polish",
        "rus" => "Russian",
        "tur" => "Turkish",
        "jpn" => "Japanese",
        "kor" => "Korean",
        "zho" => "Chinese",
        "ces" => "Czech",
        "ell" => "Modern Greek (1453-)",
        "hun" => "Hungarian",
        "ron" => "Romanian",
        "ukr" => "Ukrainian",
        "ara" => "Arabic",
        "heb" => "Hebrew",
        "hin" => "Hindi",
        other => return other.to_owned(),
    };
    gettextrs::dgettext("iso_639-3", english)
}
