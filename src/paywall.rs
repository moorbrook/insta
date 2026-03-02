use url::Url;

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

fn extract_domain(url_str: &str) -> Option<String> {
    let parsed = Url::parse(url_str).ok()?;
    let host = parsed.host_str()?;
    let domain = host.strip_prefix("www.").unwrap_or(host);
    Some(domain.to_lowercase())
}

pub fn is_paywalled(url_str: &str) -> bool {
    extract_domain(url_str)
        .is_some_and(|d| PAYWALLED_DOMAINS.contains(&d.as_str()))
}

pub fn get_paywalled_domain(url_str: &str) -> Option<String> {
    let domain = extract_domain(url_str)?;
    if PAYWALLED_DOMAINS.contains(&domain.as_str()) {
        Some(domain)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
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
            Some("wsj.com".to_string())
        );
        assert_eq!(get_paywalled_domain("https://example.com"), None);
    }
}
