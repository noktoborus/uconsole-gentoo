//! The language of the tools of the user: `en` or `ru`, by the locale of
//! the session; any other is `en`.

/// The languages there are.
const LANGUAGES: [&str; 2] = ["en", "ru"];

/// The language of a locale such as `ru_RU.UTF-8` or `ru-RU`.
pub fn of_locale(locale: &str) -> &'static str {
    let code = locale
        .split(['_', '-', '.', '@'])
        .next()
        .unwrap_or_default();
    LANGUAGES
        .into_iter()
        .find(|language| *language == code)
        .unwrap_or("en")
}

/// Sets the language of the texts by the locale of the session.
pub fn init() {
    let locale = sys_locale::get_locale().unwrap_or_default();
    rust_i18n::set_locale(of_locale(&locale));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        assert_eq!(of_locale("ru_RU.UTF-8"), "ru");
        assert_eq!(of_locale("ru-RU"), "ru");
        assert_eq!(of_locale("en_US"), "en");
        assert_eq!(of_locale("de_DE"), "en");
        assert_eq!(of_locale(""), "en");
    }

    #[test]
    fn every_key_has_both_languages() {
        let text = include_str!("../locales/app.yml");
        let mut key: Option<&str> = None;
        let mut seen: Vec<&str> = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if !line.starts_with(' ') {
                if let Some(key) = key {
                    assert_eq!(seen, LANGUAGES, "{key}");
                }
                key = line.strip_suffix(':').filter(|key| !key.starts_with('_'));
                seen.clear();
            } else if let Some((language, _)) = line.trim().split_once(':') {
                seen.push(language);
            }
        }
        if let Some(key) = key {
            assert_eq!(seen, LANGUAGES, "{key}");
        }
    }
}
