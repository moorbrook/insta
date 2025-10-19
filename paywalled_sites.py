"""
List of known paywalled sites
Source: bypass-paywalls-firefox-clean extension
Note: This list is for identification purposes only, not for bypassing paywalls
"""

PAYWALLED_DOMAINS = [
    # USA National News
    "bbc.com",
    "reuters.com",
    "nytimes.com",
    "washingtonpost.com",

    # Business Publications
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

    # Tech/Science
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

    # UK/Ireland Publications
    "ft.com",
    "economist.com",
    "telegraph.co.uk",
    "thetimes.com",
    "spectator.co.uk",
    "independent.co.uk",
    "theguardian.com",
    "bbc.co.uk",

    # European Media
    "lemonde.fr",
    "lefigaro.fr",
    "spiegel.de",
    "zeit.de",
    "corriere.it",
    "elpais.com",
    "lavanguardia.com",

    # Australian/NZ
    "nzherald.co.nz",
    "theaustralian.com.au",
    "smh.com.au",
    "theage.com.au",

    # Asia-Pacific
    "scmp.com",
    "nikkei.com",
    "japantimes.co.jp",
    "thehindu.com",
    "indianexpress.com",

    # Latin America
    "lanacion.com.ar",
    "clarin.com",
    "folha.uol.com.br",
    "elmercurio.com",
    "eltiempo.com",
]


def is_paywalled(url: str) -> bool:
    """
    Check if a URL is from a known paywalled site.

    Args:
        url: The URL to check

    Returns:
        True if the domain is in the paywalled sites list
    """
    from urllib.parse import urlparse

    try:
        domain = urlparse(url).netloc.lower()
        # Remove www. prefix if present
        if domain.startswith('www.'):
            domain = domain[4:]

        return domain in PAYWALLED_DOMAINS
    except Exception:
        return False


def get_paywalled_domain(url: str) -> str | None:
    """
    Get the paywalled domain from a URL if it matches.

    Args:
        url: The URL to check

    Returns:
        The paywalled domain name or None if not paywalled
    """
    from urllib.parse import urlparse

    try:
        domain = urlparse(url).netloc.lower()
        # Remove www. prefix if present
        if domain.startswith('www.'):
            domain = domain[4:]

        if domain in PAYWALLED_DOMAINS:
            return domain
        return None
    except Exception:
        return None
