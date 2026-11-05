//! Translation helpers. The message ids are the ones the Python version
//! used, so the existing translations keep working.

pub use gettextrs::gettext;

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
