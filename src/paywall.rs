use crate::extractors::host_is_domain_or_subdomain;

const PAYWALLED_DOMAINS: &[&str] = &[
    // USA National News
    "bbc.com",
    "reuters.com",
    "nytimes.com",
    "washingtonpost.com",
    // Business Publications
    "adweek.com",
    "americanaffairsjournal.org",
    "barrons.com",
    "benzinga.com",
    "bloomberg.com",
    "businessinsider.com",
    "citywire.com",
    "cnbc.com",
    "entrepreneur.com",
    "fastcompany.com",
    "forbes.com",
    "fortune.com",
    "hbr.org",
    "inc.com",
    "marketwatch.com",
    "sloanreview.mit.edu",
    "mnimarkets.com",
    "qz.com",
    "spglobal.com",
    "stocknews.com",
    "bizjournals.com",
    "businessoffashion.com",
    "wsj.com",
    "voguebusiness.com",
    // Tech/Science
    "brill.com",
    "thebulletin.org",
    "cen.acs.org",
    "discovermagazine.com",
    "historytoday.com",
    "insidehighered.com",
    "interestingengineering.com",
    "medscape.com",
    "technologyreview.com",
    "nationalgeographic.com",
    "nature.com",
    "nautil.us",
    "newscientist.com",
    "popsci.com",
    "science.org",
    "sciencenews.org",
    "scientificamerican.com",
    "statnews.com",
    "the-scientist.com",
    "timeshighereducation.com",
    // UK/Ireland
    "ft.com",
    "economist.com",
    "telegraph.co.uk",
    "thetimes.com",
    "spectator.co.uk",
    "independent.co.uk",
    "theguardian.com",
    "bbc.co.uk",
    // European
    "lemonde.fr",
    "lefigaro.fr",
    "spiegel.de",
    "zeit.de",
    "corriere.it",
    "elpais.com",
    "lavanguardia.com",
    // Australian/NZ
    "nzherald.co.nz",
    "theaustralian.com.au",
    "smh.com.au",
    "theage.com.au",
    // Asia-Pacific
    "scmp.com",
    "nikkei.com",
    "japantimes.co.jp",
    "thehindu.com",
    "indianexpress.com",
    // Latin America
    "lanacion.com.ar",
    "clarin.com",
    "folha.uol.com.br",
    "elmercurio.com",
    "eltiempo.com",
];

pub(crate) fn is_paywalled(url_str: &str) -> bool {
    get_paywalled_domain(url_str).is_some()
}

pub(crate) fn get_paywalled_domain(url_str: &str) -> Option<&'static str> {
    PAYWALLED_DOMAINS
        .iter()
        .copied()
        .find(|domain| host_is_domain_or_subdomain(url_str, domain))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stdout,
        clippy::print_stderr
    )]

    use super::*;

    #[test]
    fn test_paywalled() {
        assert!(is_paywalled("https://www.nytimes.com/2024/article"));
        assert!(is_paywalled("https://bloomberg.com/news"));
    }

    #[test]
    fn test_not_paywalled() {
        assert!(!is_paywalled("https://example.com"));
        assert!(!is_paywalled("https://github.com/repo"));
    }

    #[test]
    fn test_get_paywalled_domain() {
        assert_eq!(
            get_paywalled_domain("https://www.wsj.com/article"),
            Some("wsj.com")
        );
        assert_eq!(get_paywalled_domain("https://example.com"), None);
    }

    #[test]
    fn path_query_fragment_and_scheme_do_not_change_classification() {
        for url in [
            "https://nytimes.com",
            "http://nytimes.com/story",
            "https://nytimes.com/story?gift=1",
            "https://nytimes.com/story#comments",
            "HTTPS://WWW.NYTIMES.COM/story",
        ] {
            assert!(is_paywalled(url), "expected paywall classification: {url}");
        }
    }

    #[test]
    fn domain_substrings_and_typosquats_are_not_classified() {
        for url in [
            "https://notnytimes.com/story",
            "https://nytimes.com.evil.example/story",
            "https://example.com/?next=https://nytimes.com",
            "https://nytimes.com@example.com/story",
            "not a URL mentioning nytimes.com",
        ] {
            assert!(!is_paywalled(url), "false paywall classification: {url}");
        }
    }

    #[test]
    fn listed_subdomains_are_classified() {
        assert!(is_paywalled("https://cooking.nytimes.com/recipe"));
        assert_eq!(
            get_paywalled_domain("https://cooking.nytimes.com/recipe"),
            Some("nytimes.com")
        );
    }
}
