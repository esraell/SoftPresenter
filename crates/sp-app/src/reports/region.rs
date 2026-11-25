//! Where the computer says it is, for one thing only: whether anonymous reports start on or
//! off. In the European Union, the rest of the European Economic Area and the United Kingdom
//! they start **off**, and the setup invites the person to turn them on; everywhere else they
//! start on, and the setup shows the key that turns them off. A choice the person has made
//! always wins over either.
//!
//! It is the computer's *region*, never its language: a computer in Germany set to English is
//! in Germany. Each system is asked in its own way: a Mac's current locale (the global
//! `AppleLocale` default, read with `defaults`, as the languages are in `system_language`),
//! Windows' own name for the user's location (`GetUserDefaultGeoName`), elsewhere the territory
//! of `LC_ALL`, `LC_MESSAGES` or `LANG`, the first one set. A region that can't be read counts
//! as one where reports start on. The region is never sent, and is not kept.

/// The countries where reports start off: the European Union's 27, the three more of the
/// European Economic Area, and the United Kingdom, by their two-letter codes.
const OFF_BY_DEFAULT: [&str; 31] = [
    // The European Union.
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV", "LT", "LU", "MT",
    "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE", // The rest of the European Economic Area.
    "IS", "LI", "NO", // The United Kingdom.
    "GB",
];

/// Whether reports start on in this country (`DE`, `us`); an unknown one counts as on. `UK` and
/// `EL` are taken as the United Kingdom's and Greece's other names.
pub fn starts_on(country: Option<&str>) -> bool {
    let Some(country) = country else { return true };
    let code = country.trim().to_ascii_uppercase();
    let code = match code.as_str() {
        "UK" => "GB",
        "EL" => "GR",
        other => other,
    };
    !OFF_BY_DEFAULT.contains(&code)
}

/// A country's two-letter code as it is written, in capitals; nothing for anything else (a
/// region that is no country, such as `001` for the world or `419` for Latin America, or the
/// codes that stand for "unknown").
fn country_code(text: &str) -> Option<String> {
    let text = text.trim();
    let code = text.to_ascii_uppercase();
    (text.len() == 2 && text.bytes().all(|b| b.is_ascii_alphabetic()) && !matches!(code.as_str(), "XX" | "ZZ"))
        .then_some(code)
}

#[cfg(any(test, not(windows)))]
/// The territory in a locale's name: `DE` in `de_DE.UTF-8`, `en_DE@currency=EUR`, `en-DE`,
/// `sr_Latn_RS`. Nothing where it names none (`C`, `POSIX`, `ru`, `es_419`).
pub fn territory(locale: &str) -> Option<String> {
    let name = locale.trim().trim_matches('"').split(['.', '@']).next().unwrap_or("");
    // The first part is the language; the territory is the first two-letter part after it.
    name.split(['_', '-']).skip(1).find_map(country_code)
}

#[cfg(any(test, not(any(target_os = "macos", windows))))]
/// The territory in the locale variables, the first set of `LC_ALL`, `LC_MESSAGES` and `LANG`
/// (POSIX's order).
pub fn from_locale_variables(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Option<String> {
    let value = [lc_all, lc_messages, lang].into_iter().flatten().map(str::trim).find(|value| !value.is_empty())?;
    territory(value)
}

#[cfg(any(test, target_os = "macos"))]
/// The country in what `defaults read -g AppleLocale` prints: `en_DE`, or with more after an
/// `@` (`en_DE@rg=dezzzz`).
pub fn from_apple_locale(printed: &str) -> Option<String> {
    territory(printed.lines().next().unwrap_or(""))
}

#[cfg(any(test, windows))]
/// The country in what Windows calls the user's location: `DE`. `XX` (not set) and a region
/// that is no country (`001`) give nothing.
pub fn from_geo_name(name: &str) -> Option<String> {
    country_code(name)
}

/// The country the computer says it is in, or nothing when it doesn't say. On a Mac this runs a
/// small program, so it is asked on the reporter's own thread, once.
pub fn read() -> Option<String> {
    // Testing aid: SOFTPRESENTER_REGION=DE stands in for the computer's region (a country's
    // two-letter code), so a test copy starts as a computer there does.
    if let Ok(stand_in) = std::env::var("SOFTPRESENTER_REGION")
        && !stand_in.trim().is_empty()
    {
        return country_code(&stand_in);
    }
    #[cfg(target_os = "macos")]
    {
        super::sample::quiet_command("defaults", &["read", "-g", "AppleLocale"])
            .and_then(|printed| from_apple_locale(&printed))
    }
    #[cfg(windows)]
    {
        sp_winapi::user_geo_name().and_then(|name| from_geo_name(&name))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let var = |name: &str| std::env::var(name).ok();
        from_locale_variables(var("LC_ALL").as_deref(), var("LC_MESSAGES").as_deref(), var("LANG").as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_start_off_in_the_eu_the_eea_and_the_uk_and_on_everywhere_else() {
        let eu = [
            "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV", "LT", "LU",
            "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
        ];
        assert_eq!(eu.len(), 27, "the European Union has 27 members");
        for country in eu.into_iter().chain(["IS", "LI", "NO"]).chain(["GB", "UK", "gb", "de", "EL"]) {
            assert!(!starts_on(Some(country)), "{country} starts off");
        }
        assert_eq!(OFF_BY_DEFAULT.len(), 27 + 3 + 1);
        // Everywhere else, and where the computer doesn't say: on.
        for country in ["US", "CA", "UA", "RU", "CH", "MX", "BR", "AU", "TR", "RS", "MD", "KZ", "ZZ", ""] {
            assert!(starts_on(Some(country)), "{country} starts on");
        }
        assert!(starts_on(None));
    }

    #[test]
    fn the_region_is_the_territory_never_the_language() {
        // A computer in Germany set to English is in Germany; one set to German in the United
        // States is not.
        assert_eq!(territory("en_DE").as_deref(), Some("DE"));
        assert_eq!(territory("de_US").as_deref(), Some("US"));
        assert_eq!(territory("de_DE.UTF-8").as_deref(), Some("DE"));
        assert_eq!(territory("ca_ES.UTF-8@valencia").as_deref(), Some("ES"));
        assert_eq!(territory("sr_Latn_RS").as_deref(), Some("RS"));
        assert_eq!(territory("en-GB").as_deref(), Some("GB"));
        assert_eq!(territory("en_US_POSIX").as_deref(), Some("US"));
        for none in ["C", "POSIX", "C.UTF-8", "ru", "de", "es_419", "en_001", ""] {
            assert_eq!(territory(none), None, "{none}");
        }
    }

    #[test]
    fn a_mac_says_its_region_in_its_locale() {
        assert_eq!(from_apple_locale("en_DE\n").as_deref(), Some("DE"));
        assert_eq!(from_apple_locale("ru_UA\n").as_deref(), Some("UA"));
        assert_eq!(from_apple_locale("en_US@rg=dezzzz\n").as_deref(), Some("US"));
        assert_eq!(from_apple_locale("\"en_GB@calendar=gregorian\"\n").as_deref(), Some("GB"));
        assert_eq!(from_apple_locale("uk\n"), None);
        assert_eq!(from_apple_locale(""), None);
    }

    #[test]
    fn windows_names_the_users_location() {
        assert_eq!(from_geo_name("DE").as_deref(), Some("DE"));
        assert_eq!(from_geo_name("us").as_deref(), Some("US"));
        // Not set, or a region that is no country.
        for none in ["XX", "001", "150", "", "DEU"] {
            assert_eq!(from_geo_name(none), None, "{none}");
        }
    }

    #[test]
    fn linux_says_it_in_the_first_locale_variable_set() {
        assert_eq!(from_locale_variables(Some("fr_FR.UTF-8"), Some("en_US.UTF-8"), None).as_deref(), Some("FR"));
        assert_eq!(from_locale_variables(None, Some(""), Some("de_AT.UTF-8")).as_deref(), Some("AT"));
        assert_eq!(from_locale_variables(Some(" "), None, Some("en_US.UTF-8")).as_deref(), Some("US"));
        // The first one set decides, even when it names no territory.
        assert_eq!(from_locale_variables(Some("C"), None, Some("de_DE.UTF-8")), None);
        assert_eq!(from_locale_variables(None, None, None), None);
    }
}
