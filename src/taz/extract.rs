use super::{
    models::{ArticleSummary, DiscoveryReport, DiscoverySourceKind},
    normalize::{clean_whitespace, strip_markup, trim_chars},
};
use crate::identity::article_key_from_url;
use anyhow::{Result, bail};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::{collections::HashSet, sync::LazyLock};

const BASE_URL: &str = "https://taz.de";

mod parsed {
    use super::*;

    pub static LINKS: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse(super::selectors::LINKS).unwrap());
    pub static ARTICLE: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse(super::selectors::ARTICLE).unwrap());
    pub static BODY_ELEMENTS: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse(super::selectors::BODY_ELEMENTS).unwrap());
    pub static BODY_FALLBACK: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse(super::selectors::BODY_FALLBACK).unwrap());
    pub static HEADLINE: LazyLock<Vec<Selector>> = LazyLock::new(|| {
        super::selectors::HEADLINE
            .iter()
            .filter_map(|s| Selector::parse(s).ok())
            .collect()
    });
}

mod selectors {
    pub const TITLE: &[&str] = &["h1", "meta[property=\"og:title\"]", "title"];
    pub const SUBTITLE: &[&str] = &["meta[name=\"description\"]"];
    pub const AUTHOR: &[&str] = &["meta[property=\"article:author\"]"];
    pub const AUTHOR_FALLBACK: &[&str] = &["[rel=\"author\"]", ".author", "[class*=\"author\"]"];
    pub const SECTION: &[&str] = &["meta[property=\"article:section\"]"];
    pub const DATE_TIME: &[&str] = &["time[datetime]"];
    pub const DATE_META: &[&str] = &[
        "meta[property=\"article:published_time\"]",
        "meta[name=\"date\"]",
    ];
    pub const ARTICLE: &str = "article";
    pub const BODY_ELEMENTS: &str = "p, h2, h3, li";
    pub const BODY_FALLBACK: &str = "main p";
    pub const LINKS: &str = "a[href]";
    pub const HEADLINE: &[&str] = &[".headline", "h1", "h2", "h3"];
    pub const DONATE_MARKERS: &[&str] = &[
        "Gemeinsam für freie Presse",
        "Jetzt unterstützen",
        "Diesen Artikel teilen",
        "Feedback",
        "Leser*innenkommentar",
        "Fehlerhinweis",
        "Mehr zum Thema",
    ];
    pub const PAYWALL: &[&str] = &[
        ".paywall",
        ".paywall-overlay",
        "[data-paywall]",
        ".tzi-paywall",
        ".article-paywall",
        ".hide-paywall",
    ];
    pub const PAYWALL_TEXT_MARKERS: &[&str] = &[
        "Lesen Sie diesen Artikel mit taz-zahl-ich",
        "Dieser Artikel ist für Abonnent",
        "Für diesen Artikel müssen Sie",
        "nur für Abonnent",
        "jetzt weiterlesen mit taz-zahl-ich",
        "um diesen artikel vollständig zu lesen",
        "mit einem abo weiterlesen",
    ];
    pub const SUPPORT_OVERLAY_TEXT_MARKERS: &[&str] = &[
        "ohne paywall",
        "freier zugang zu unabhängiger presse",
        "damit das so bleibt, brauchen wir ihre unterstützung",
        "schon ein kleiner beitrag hilft",
        "fördern sie jetzt den taz-journalismus",
        "gerade nicht",
        "schon dabei!",
    ];
}

pub(super) fn title_selectors() -> &'static [&'static str] {
    selectors::TITLE
}

pub(super) fn subtitle_selectors() -> &'static [&'static str] {
    selectors::SUBTITLE
}

pub(super) fn author_selectors() -> &'static [&'static str] {
    selectors::AUTHOR
}

pub(super) fn author_fallback_selectors() -> &'static [&'static str] {
    selectors::AUTHOR_FALLBACK
}

pub(super) fn section_selectors() -> &'static [&'static str] {
    selectors::SECTION
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_articles_from_document(
    article_url_re: &Regex,
    document: &Html,
    fallback_section: Option<&str>,
    source_url: &str,
    source_kind: DiscoverySourceKind,
    limit: usize,
    seen: &mut HashSet<String>,
    articles: &mut Vec<ArticleSummary>,
    report: &mut DiscoveryReport,
) {
    let selector = parsed::LINKS.clone();

    for link in document.select(&selector) {
        let Some(raw_href) = link.value().attr("href") else {
            continue;
        };

        let article_url = absolute_url(raw_href);
        if !article_url_re.is_match(&article_url) {
            continue;
        }
        if !seen.insert(article_url.clone()) {
            report.deduped_articles += 1;
            continue;
        }

        let title = extract_browse_title(link);
        if !looks_like_article_title(&title) {
            continue;
        }

        let teaser = extract_teaser(link);
        let section = fallback_section
            .map(str::to_owned)
            .unwrap_or_else(|| infer_section_from_url(&article_url));

        articles.push(ArticleSummary {
            article_key: article_key_from_url(&article_url),
            url: article_url,
            title,
            teaser,
            section,
            source_kind,
            source_label: source_label(source_url),
        });
        report.record_article(source_kind);

        if articles.len() >= limit {
            break;
        }
    }
}

pub(super) fn extract_topic_urls(
    article_url_re: &Regex,
    topic_url_re: &Regex,
    document: &Html,
) -> Vec<String> {
    let selector = parsed::LINKS.clone();
    let mut urls = Vec::new();
    let mut seen = HashSet::new();

    for link in document.select(&selector) {
        let Some(raw_href) = link.value().attr("href") else {
            continue;
        };

        let url = absolute_url(raw_href);
        if !topic_url_re.is_match(&url) {
            continue;
        }
        if article_url_re.is_match(&url) || !seen.insert(url.clone()) {
            continue;
        }

        urls.push(url);
    }

    urls
}

pub(super) fn extract_teaser(link: ElementRef<'_>) -> String {
    let title = extract_browse_title(link);
    let mut parent = link.parent();
    for _ in 0..3 {
        let Some(node) = parent else {
            break;
        };

        if let Some(value) = ElementRef::wrap(node) {
            let text = strip_markup(&clean_whitespace(&collect_text(value)));
            if text.len() > title.len() && text.len() > 20 {
                return trim_chars(&text, 220);
            }
        }

        parent = node.parent();
    }

    String::new()
}

pub(super) fn extract_browse_title(link: ElementRef<'_>) -> String {
    let mut parent = link.parent();

    for _ in 0..2 {
        let Some(node) = parent else {
            break;
        };

        if let Some(element) = ElementRef::wrap(node) {
            for selector in parsed::HEADLINE.iter() {
                if let Some(candidate) = element
                    .select(selector)
                    .map(collect_text)
                    .map(|text| clean_whitespace(&text))
                    .find(|text| looks_like_article_title(text))
                {
                    return candidate;
                }
            }
        }

        parent = node.parent();
    }

    clean_whitespace(&collect_text(link))
}

pub(super) fn strip_search_suffix(url: &str) -> String {
    if let Some(pos) = url.find("&s=") {
        let before = &url[..pos];
        if url.ends_with('/') {
            format!("{before}/")
        } else {
            before.to_owned()
        }
    } else {
        url.to_owned()
    }
}

pub(super) fn detect_paywall(document: &Html, html: &str) -> bool {
    let html_lower = html.to_lowercase();

    let has_support_overlay_text = selectors::SUPPORT_OVERLAY_TEXT_MARKERS
        .iter()
        .any(|marker| html_lower.contains(&marker.to_lowercase()));
    let has_paywall_text = selectors::PAYWALL_TEXT_MARKERS
        .iter()
        .any(|marker| html_lower.contains(&marker.to_lowercase()));

    if has_support_overlay_text && !has_paywall_text {
        return false;
    }

    for selector_str in selectors::PAYWALL {
        let Ok(selector) = Selector::parse(selector_str) else {
            continue;
        };
        if document.select(&selector).next().is_some() {
            if has_support_overlay_text && !has_paywall_text {
                return false;
            }
            return true;
        }
    }

    has_paywall_text
}

const EXCLUDE_ANCESTOR_TAGS: &[&str] = &["figure", "figcaption", "aside"];
const EXCLUDE_ANCESTOR_CLASSES: &[&str] =
    &["webelement_bio", "webelement_info", "author-container"];
const INTERVIEW_MIN_LEN: usize = 6;
const INTERVIEW_PREFIXES: &[&str] = &["taz:", "taz :", "taz :"];

fn has_excluded_ancestor(node: &ElementRef<'_>) -> bool {
    let mut current = node.parent();
    while let Some(parent_ref) = current {
        if let Some(element) = parent_ref.value().as_element() {
            let tag = element.name();
            if EXCLUDE_ANCESTOR_TAGS.contains(&tag) {
                return true;
            }
            if let Some(classes) = element.attr("class")
                && EXCLUDE_ANCESTOR_CLASSES.iter().any(|c| classes.contains(c))
            {
                return true;
            }
        }
        current = parent_ref.parent();
    }
    false
}

fn is_interview_line(text: &str) -> bool {
    let lower = text.to_lowercase();
    if INTERVIEW_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    if let Some(colon_pos) = text[..text.len().min(40)].find(':') {
        let prefix = &text[..colon_pos];
        let words: Vec<&str> = prefix.split_whitespace().collect();
        if (1..=3).contains(&words.len())
            && words.iter().all(|w| {
                w.chars().next().is_some_and(|c| c.is_uppercase())
                    && w.chars().all(|c| c.is_alphabetic())
            })
        {
            return true;
        }
    }
    false
}

pub(super) fn extract_body(document: &Html) -> Result<String> {
    let article_selector = parsed::ARTICLE.clone();
    let paragraph_selector = parsed::BODY_ELEMENTS.clone();
    let donate_markers = selectors::DONATE_MARKERS;

    let mut best_blocks = Vec::new();

    for article in document.select(&article_selector) {
        let mut blocks = Vec::new();

        for node in article.select(&paragraph_selector) {
            if has_excluded_ancestor(&node) {
                continue;
            }

            let name = node.value().name();
            let text = clean_whitespace(&collect_text(node));
            if text.is_empty() || donate_markers.iter().any(|marker| text.contains(marker)) {
                continue;
            }

            match name {
                "h2" | "h3" if text.len() >= 4 => blocks.push(format!("## {text}")),
                "li" if text.len() >= 20 => blocks.push(format!("- {text}")),
                "p" => {
                    if text.len() >= 45
                        || (text.len() >= INTERVIEW_MIN_LEN && is_interview_line(&text))
                    {
                        blocks.push(text);
                    }
                }
                _ => {}
            }
        }

        if blocks.len() > best_blocks.len() {
            best_blocks = blocks;
        }
    }

    if best_blocks.is_empty() {
        let fallback_selector = parsed::BODY_FALLBACK.clone();
        for node in document.select(&fallback_selector) {
            let text = clean_whitespace(&collect_text(node));
            if text.len() >= 45 {
                best_blocks.push(text);
            }
        }
    }

    dedupe_lines(&mut best_blocks);

    if best_blocks.is_empty() {
        bail!("could not extract article body");
    }

    Ok(best_blocks.join("\n\n"))
}

fn dedupe_lines(lines: &mut Vec<String>) {
    let mut seen = HashSet::new();
    lines.retain(|line| {
        let key = trim_chars(line, 120).to_lowercase();
        seen.insert(key)
    });
}

static RE_SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"cp:\s*"Redaktion/([^/",]+)"#).expect("section regex"));
static RE_DATE_JSON_LD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""datePublished"\s*:\s*"([^"]+)""#).expect("json-ld date regex"));
static RE_DATE_TEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,2}\.\d{1,2}\.\d{4})\b").expect("text date regex"));
static RE_DATE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/!?(\d{4})/(\d{2})/(\d{2})/").expect("url date regex"));

pub(super) fn extract_section_from_html(html: &str) -> Option<String> {
    RE_SECTION
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| clean_whitespace(value.as_str()))
}

pub(super) fn extract_date(document: &Html, html: &str, url: &str) -> Option<String> {
    first_attr(document, selectors::DATE_TIME, "datetime")
        .or_else(|| first_attr(document, selectors::DATE_META, "content"))
        .or_else(|| extract_date_from_json_ld(html))
        .or_else(|| extract_date_from_text(html))
        .or_else(|| extract_date_from_url(url))
}

fn extract_date_from_json_ld(html: &str) -> Option<String> {
    RE_DATE_JSON_LD
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| clean_whitespace(value.as_str()))
}

fn extract_date_from_text(html: &str) -> Option<String> {
    RE_DATE_TEXT
        .captures(html)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_owned())
}

fn extract_date_from_url(url: &str) -> Option<String> {
    let captures = RE_DATE_URL.captures(url)?;
    Some(format!(
        "{}-{}-{}",
        captures.get(1)?.as_str(),
        captures.get(2)?.as_str(),
        captures.get(3)?.as_str()
    ))
}

fn source_label(source_url: &str) -> String {
    if source_url == BASE_URL || source_url.trim_end_matches('/') == BASE_URL {
        return "Startseite".to_owned();
    }

    let path = source_url.trim_start_matches(BASE_URL).trim_matches('/');
    if path.is_empty() {
        "Startseite".to_owned()
    } else {
        path.to_owned()
    }
}

pub(super) fn first_text(document: &Html, selectors: &[&str]) -> Option<String> {
    for selector in selectors {
        let Ok(selector) = Selector::parse(selector) else {
            continue;
        };
        let value = document.select(&selector).find_map(|node| {
            let attr_content = node.value().attr("content").map(clean_whitespace);
            let text_content =
                Some(clean_whitespace(&collect_text(node))).filter(|value| !value.is_empty());
            attr_content.or(text_content)
        });
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            return Some(value);
        }
    }
    None
}

pub(super) fn first_attr(document: &Html, selectors: &[&str], attr: &str) -> Option<String> {
    for selector in selectors {
        let Ok(selector) = Selector::parse(selector) else {
            continue;
        };
        if let Some(value) = document
            .select(&selector)
            .find_map(|node| node.value().attr(attr))
            .map(clean_whitespace)
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }
    }
    None
}

pub(super) fn looks_like_article_title(title: &str) -> bool {
    title.len() >= 15
        && title.len() <= 220
        && !title.starts_with("Jetzt unterstützen")
        && !title.starts_with("Fehlerhinweis")
        && !title.starts_with("Diesen Artikel teilen")
}

pub(super) fn infer_section_from_url(url: &str) -> String {
    let path = url
        .trim_start_matches(BASE_URL)
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("taz");

    if path.starts_with('!') || path.is_empty() {
        "Startseite".to_owned()
    } else {
        path.replace('-', " ")
    }
}

pub(super) fn absolute_url(raw_href: &str) -> String {
    if raw_href.starts_with("http://") || raw_href.starts_with("https://") {
        return raw_href.to_owned();
    }

    if raw_href.starts_with('/') {
        return format!("{BASE_URL}{raw_href}");
    }

    format!("{BASE_URL}/{raw_href}")
}

fn collect_text(node: ElementRef<'_>) -> String {
    node.text().collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_full_article() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Klimawandel bedroht Küsten | taz.de</title>
    <meta property="og:title" content="Klimawandel bedroht Küsten">
    <meta name="description" content="Steigende Meeresspiegel gefährden Millionen Menschen an den Küsten weltweit.">
    <meta property="article:author" content="Maria Schmidt">
    <meta property="article:section" content="Umwelt">
    <meta property="article:published_time" content="2025-03-15T10:30:00+01:00">
</head>
<body>
<article>
    <h1>Klimawandel bedroht Küsten</h1>
    <p>Steigende Meeresspiegel gefährden Millionen Menschen an den Küsten weltweit. Wissenschaftler warnen vor den Folgen des Klimawandels.</p>
    <p>Die Temperaturen steigen seit Jahrzehnten kontinuierlich an, und die Auswirkungen werden immer deutlicher sichtbar in allen Regionen der Welt.</p>
    <h2>Forschungsergebnisse</h2>
    <p>Neue Studien zeigen, dass der Meeresspiegel bis zum Ende des Jahrhunderts um bis zu einem Meter steigen könnte, was dramatische Folgen hätte.</p>
    <p>Besonders betroffen sind Inselstaaten im Pazifik, die bereits heute mit den Auswirkungen kämpfen und internationale Hilfe benötigen dringend.</p>
    <p>Die internationale Gemeinschaft muss schnell handeln, um die schlimmsten Auswirkungen abzuwenden und betroffenen Regionen zu helfen bei der Anpassung.</p>
    <p>Experten fordern eine drastische Reduktion der Treibhausgasemissionen und massive Investitionen in erneuerbare Energien als einzigen Ausweg aus der Krise.</p>
</article>
</body>
</html>"#
    }

    fn fixture_minimal_article() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head><title>Kurzmeldung | taz.de</title></head>
<body>
<article>
    <h1>Kurzmeldung</h1>
    <p>Ein kurzer Absatz.</p>
</article>
</body>
</html>"#
    }

    fn fixture_json_ld_date() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head><title>Test</title>
<script type="application/ld+json">{"@type":"NewsArticle","datePublished":"2025-06-20T08:00:00Z"}</script>
</head>
<body>
<article>
    <p>Placeholder text that is long enough to pass the minimum length check for paragraph extraction in the body parser.</p>
</article>
</body>
</html>"#
    }

    fn fixture_representative_taz_article() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Protest gegen neue Verkehrspolitik | taz.de</title>
    <meta property="og:title" content="Protest gegen neue Verkehrspolitik">
    <meta name="description" content="Ein Bündnis fordert mehr Platz für Busse, Fahrräder und Fußwege in der Innenstadt.">
</head>
<body>
<nav>
    <a href="/Politik/">Politik</a>
    <a href="/Kultur/">Kultur</a>
</nav>
<main>
    <article>
        <header>
            <p class="kicker">Stadtentwicklung</p>
            <h1>Protest gegen neue Verkehrspolitik</h1>
        </header>
        <p>Ein Bündnis aus Anwohnerinnen und Gewerbetreibenden fordert mehr Platz für Busse, Fahrräder und Fußwege in der Innenstadt.</p>
        <aside class="related">
            <p>Mehr zum Thema Verkehr und Klima in der Stadt finden Sie in unserem Dossier.</p>
        </aside>
        <figure>
            <img src="/bild.jpg" alt="Demonstration vor dem Rathaus">
            <figcaption>
                <p>Foto: Archivbild mit Demonstrierenden vor dem Rathaus und Straßenbahnen.</p>
            </figcaption>
        </figure>
        <h2>Viele Fragen offen</h2>
        <p>Im Rathaus wird darüber gestritten, ob die Pläne schnell genug umgesetzt werden können und wer die Bauarbeiten bezahlt.</p>
        <ul>
            <li>Die Initiative will den Umbau der Hauptstraße noch in diesem Jahr beginnen lassen.</li>
        </ul>
        <p>Diesen Artikel teilen</p>
    </article>
</main>
</body>
</html>"#
    }

    fn fixture_date_in_url() -> &'static str {
        "https://taz.de/Artikel/!2025/04/10/some-slug/"
    }

    fn fixture_section_in_cp() -> &'static str {
        r#"<html><head></head><body><script>cp: "Redaktion/Politik", page: "artikel"</script></body></html>"#
    }

    fn fixture_search_listing_page() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<body>
<main>
    <article>
        <h2><a href="/Politik/Testartikel-zur-Suche/!1001001/">Suchtreffer zur Verkehrspolitik in der Stadt</a></h2>
        <p>Ein synthetischer Teaser beschreibt Busse, Wege und einen Ausschuss.</p>
    </article>
    <article>
        <h2><a href="/Politik/Testartikel-zur-Suche/!1001001/">Suchtreffer zur Verkehrspolitik in der Stadt</a></h2>
        <p>Der doppelte Treffer darf nicht zweimal in der Liste landen.</p>
    </article>
    <article>
        <h2><a href="/Kontakt/">Kontakt</a></h2>
        <p>Navigation und Servicelinks sind keine Artikel.</p>
    </article>
</main>
</body>
</html>"#
    }

    #[test]
    fn strip_search_suffix_removes_query_param() {
        assert_eq!(
            strip_search_suffix("https://taz.de/Title/!123456&s=Query/"),
            "https://taz.de/Title/!123456/"
        );
    }

    #[test]
    fn strip_search_suffix_no_trailing_slash() {
        assert_eq!(
            strip_search_suffix("https://taz.de/Title/!123456&s=Query"),
            "https://taz.de/Title/!123456"
        );
    }

    #[test]
    fn strip_search_suffix_no_match() {
        let url = "https://taz.de/Title/!123456/";
        assert_eq!(strip_search_suffix(url), url);
    }

    #[test]
    fn extract_body_finds_paragraphs_in_article_tag() {
        let doc = Html::parse_document(fixture_full_article());
        let body = extract_body(&doc).unwrap();
        assert!(
            body.contains("Temperaturen steigen"),
            "should contain body paragraph"
        );
        assert!(
            body.contains("## Forschungsergebnisse"),
            "should preserve h2 as markdown heading"
        );
        assert!(!body.contains("<p>"), "should not contain raw HTML tags");
    }

    #[test]
    fn representative_taz_article_shape_extracts_content_without_boilerplate() {
        let doc = Html::parse_document(fixture_representative_taz_article());

        let title = first_text(&doc, selectors::TITLE);
        let body = extract_body(&doc).unwrap();

        let expected_body = concat!(
            "Ein Bündnis aus Anwohnerinnen und Gewerbetreibenden fordert mehr Platz für Busse, ",
            "Fahrräder und Fußwege in der Innenstadt.\n\n",
            "## Viele Fragen offen\n\n",
            "Im Rathaus wird darüber gestritten, ob die Pläne schnell genug umgesetzt werden ",
            "können und wer die Bauarbeiten bezahlt.\n\n",
            "- Die Initiative will den Umbau der Hauptstraße noch in diesem Jahr beginnen lassen."
        );
        assert_eq!(title.as_deref(), Some("Protest gegen neue Verkehrspolitik"));
        assert_eq!(body, expected_body);
        assert_eq!(body.split_whitespace().count(), 53);
        assert!(!body.contains("Politik"));
        assert!(!body.contains("Dossier"));
        assert!(!body.contains("Foto:"));
        assert!(!body.contains("Diesen Artikel teilen"));
    }

    #[test]
    fn extract_body_rejects_too_short_article() {
        let doc = Html::parse_document(fixture_minimal_article());
        let result = extract_body(&doc);
        assert!(
            result.is_err(),
            "should fail for article with no substantial paragraphs"
        );
    }

    #[test]
    fn extract_body_deduplicates_identical_paragraphs() {
        let html = r#"<html><body><article>
            <p>Dies ist ein langer Absatz der mindestens fünfundvierzig Zeichen haben muss um gezählt zu werden.</p>
            <p>Dies ist ein langer Absatz der mindestens fünfundvierzig Zeichen haben muss um gezählt zu werden.</p>
            <p>Ein zweiter unterschiedlicher Absatz der ebenfalls lang genug sein muss für die Extraktion.</p>
        </article></body></html>"#;
        let doc = Html::parse_document(html);
        let body = extract_body(&doc).unwrap();
        let count = body.matches("mindestens fünfundvierzig").count();
        assert_eq!(count, 1, "duplicate paragraph should be removed");
    }

    #[test]
    fn first_text_extracts_h1() {
        let doc = Html::parse_document(fixture_full_article());
        let title = first_text(&doc, selectors::TITLE);
        assert_eq!(title.as_deref(), Some("Klimawandel bedroht Küsten"));
    }

    #[test]
    fn first_text_falls_back_to_og_title() {
        let html = r#"<html><head><meta property="og:title" content="OG Titel"></head><body></body></html>"#;
        let doc = Html::parse_document(html);
        let title = first_text(&doc, selectors::TITLE);
        assert_eq!(title.as_deref(), Some("OG Titel"));
    }

    #[test]
    fn first_text_returns_none_when_no_match() {
        let doc = Html::parse_document("<html><body></body></html>");
        assert!(first_text(&doc, selectors::TITLE).is_none());
    }

    #[test]
    fn first_attr_extracts_meta_content() {
        let doc = Html::parse_document(fixture_full_article());
        let author = first_attr(&doc, selectors::AUTHOR, "content");
        assert_eq!(author.as_deref(), Some("Maria Schmidt"));
    }

    #[test]
    fn first_attr_extracts_section() {
        let doc = Html::parse_document(fixture_full_article());
        let section = first_attr(&doc, selectors::SECTION, "content");
        assert_eq!(section.as_deref(), Some("Umwelt"));
    }

    #[test]
    fn extract_date_from_meta_tag() {
        let doc = Html::parse_document(fixture_full_article());
        let date = extract_date(&doc, fixture_full_article(), "https://taz.de/test/");
        assert_eq!(date.as_deref(), Some("2025-03-15T10:30:00+01:00"));
    }

    #[test]
    fn extract_date_from_json_ld_fixture() {
        let html = fixture_json_ld_date();
        let result = extract_date_from_json_ld(html);
        assert_eq!(result.as_deref(), Some("2025-06-20T08:00:00Z"));
    }

    #[test]
    fn extract_date_from_url_fixture() {
        let result = extract_date_from_url(fixture_date_in_url());
        assert_eq!(result.as_deref(), Some("2025-04-10"));
    }

    #[test]
    fn extract_date_from_german_text() {
        let html = "<html><body>Veröffentlicht am 15.03.2025 um 10 Uhr</body></html>";
        let result = extract_date_from_text(html);
        assert_eq!(result.as_deref(), Some("15.03.2025"));
    }

    #[test]
    fn extract_section_from_cp_variable() {
        let result = extract_section_from_html(fixture_section_in_cp());
        assert_eq!(result.as_deref(), Some("Politik"));
    }

    #[test]
    fn extract_section_returns_none_without_cp() {
        assert!(extract_section_from_html("<html><body>no cp here</body></html>").is_none());
    }

    #[test]
    fn dedupe_lines_removes_exact_duplicates() {
        let mut lines = vec!["aaa".to_owned(), "bbb".to_owned(), "aaa".to_owned()];
        dedupe_lines(&mut lines);
        assert_eq!(lines, vec!["aaa", "bbb"]);
    }

    #[test]
    fn looks_like_article_title_accepts_normal_title() {
        assert!(looks_like_article_title("Klimawandel bedroht Küsten"));
    }

    #[test]
    fn looks_like_article_title_rejects_short_text() {
        assert!(!looks_like_article_title("Mehr"));
    }

    #[test]
    fn source_label_startseite_for_base_url() {
        assert_eq!(source_label("https://taz.de"), "Startseite");
        assert_eq!(source_label("https://taz.de/"), "Startseite");
    }

    #[test]
    fn source_label_returns_path_for_section() {
        assert_eq!(source_label("https://taz.de/Politik/"), "Politik");
    }

    #[test]
    fn search_listing_collects_articles_and_dedupes_urls() {
        let article_url_re = Regex::new(r"^https://taz\.de/(?:[^/]+/)*(?:%21|!)\d+/??$").unwrap();
        let document = Html::parse_document(fixture_search_listing_page());
        let mut seen = HashSet::new();
        let mut articles = Vec::new();
        let mut report = DiscoveryReport {
            source_pages_visited: 0,
            section_pages_visited: 0,
            subsection_pages_visited: 0,
            topic_pages_visited: 0,
            section_articles: 0,
            subsection_articles: 0,
            topic_articles: 0,
            deduped_articles: 0,
        };

        collect_articles_from_document(
            &article_url_re,
            &document,
            Some("Suche"),
            "https://taz.de/!s=verkehr/",
            DiscoverySourceKind::Search,
            10,
            &mut seen,
            &mut articles,
            &mut report,
        );

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].article_key, "1001001");
        assert_eq!(
            articles[0].url,
            "https://taz.de/Politik/Testartikel-zur-Suche/!1001001/"
        );
        assert_eq!(
            articles[0].title,
            "Suchtreffer zur Verkehrspolitik in der Stadt"
        );
        assert!(articles[0].teaser.contains("synthetischer Teaser"));
        assert_eq!(articles[0].section, "Suche");
        assert_eq!(articles[0].source_kind, DiscoverySourceKind::Search);
        assert_eq!(articles[0].source_label, "!s=verkehr");
        assert_eq!(report.deduped_articles, 1);
    }

    #[test]
    fn infer_section_from_url_extracts_first_segment() {
        let result = infer_section_from_url("https://taz.de/Politik/!1234567/");
        assert_eq!(result, "Politik");
    }

    #[test]
    fn is_interview_line_detects_taz_prefix() {
        assert!(is_interview_line("taz: Wie waren Sie als Kind?"));
    }

    #[test]
    fn is_interview_line_detects_speaker_name() {
        assert!(is_interview_line("Reynolds: Ja, das stimmt."));
        assert!(is_interview_line("Jason Reynolds: Ich sehe mich selbst."));
    }

    #[test]
    fn is_interview_line_rejects_normal_text() {
        assert!(!is_interview_line(
            "Dies ist ein normaler Absatz ohne Sprecherkennzeichnung."
        ));
    }

    #[test]
    fn is_interview_line_rejects_non_name_prefix() {
        assert!(!is_interview_line(
            "das problem: es gibt keine lösung für dieses thema."
        ));
        assert!(!is_interview_line(
            "2025: Ein Jahr der Veränderungen in der Politik."
        ));
    }

    #[test]
    fn extract_body_keeps_short_interview_questions() {
        let html = r#"<html><body><article>
            <p><strong>taz: Wie waren Sie als Kind?</strong></p>
            <p>Reynolds: Ich war empfindsam, und zwar auf eine beständige und geerdete Art und Weise, die mich überall hinführte.</p>
            <p><strong>taz: Schon vom ersten Buch an?</strong></p>
            <p>Reynolds: Ja, weil es so lange gedauert hatte, überhaupt dahin zu kommen und das war wirklich bemerkenswert!</p>
        </article></body></html>"#;
        let doc = Html::parse_document(html);
        let body = extract_body(&doc).unwrap();
        assert!(
            body.contains("taz: Wie waren Sie als Kind?"),
            "should keep short taz: question"
        );
        assert!(
            body.contains("taz: Schon vom ersten Buch an?"),
            "should keep another short question"
        );
    }

    #[test]
    fn extract_body_excludes_figure_captions() {
        let html = r#"<html><body><article>
            <figure>
                <img src="photo.jpg" alt="Portrait von Jason Reynolds">
                <figcaption>
                    <p>In seiner Kindheit gab es keine Bücher, sagt Jason Reynolds.</p>
                    <span>Foto: Anna Tiessen</span>
                </figcaption>
            </figure>
            <p>Dies ist der eigentliche Artikeltext, der lang genug sein muss, um die Mindestlänge zu überschreiten.</p>
        </article></body></html>"#;
        let doc = Html::parse_document(html);
        let body = extract_body(&doc).unwrap();
        assert!(
            !body.contains("Foto: Anna Tiessen"),
            "should not contain photo credit"
        );
        assert!(
            !body.contains("keine Bücher, sagt"),
            "should not contain figcaption text"
        );
        assert!(
            body.contains("eigentliche Artikeltext"),
            "should contain actual body text"
        );
    }

    #[test]
    fn extract_body_excludes_bio_sidebar() {
        let html = r#"<html><body><article>
            <p>Haupttext des Artikels der lang genug sein muss, um die Mindestlänge von fünfundvierzig Zeichen zu erreichen.</p>
            <div class="webelement_bio webelement-pos-7">
                <p><strong>Der Autor</strong></p>
                <p>Jason Reynolds, 42, wuchs in einem Vorort von Washington D.C. auf und studierte Literaturwissenschaften.</p>
            </div>
            <p>Noch ein Absatz im Haupttext der ebenfalls lang genug ist, um mitgenommen zu werden und nicht gefiltert.</p>
        </article></body></html>"#;
        let doc = Html::parse_document(html);
        let body = extract_body(&doc).unwrap();
        assert!(
            !body.contains("Der Autor"),
            "should not contain bio heading"
        );
        assert!(
            !body.contains("Literaturwissenschaften"),
            "should not contain bio text"
        );
        assert!(
            body.contains("Haupttext des Artikels"),
            "should contain body text"
        );
    }
}
