//! URLs checked against what Pele makes of the same cases, in the oracle's url fixture.

use limu_fixtures::load;
use serde::Deserialize;

use super::{BLANK, Schemes, decode, image_url, internal_link, link_url, safe_url, sanitize_url};

#[derive(Deserialize)]
struct Fixture {
    url: Vec<Case>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Case {
    input: String,
    sanitized: String,
    safe: String,
    link: Option<String>,
    image: Option<String>,
    mail: Option<String>,
    internal: bool,
    internal_mail: bool,
}

/// The schemes the oracle passes as linkSchemes, for the cases of mail and internalMail.
const MAIL: Schemes = Schemes {
    links: true,
    images: true,
    link_schemes: &["mailto", "obsidian"],
    image_schemes: super::IMAGE_SCHEMES,
};

#[test]
fn urls_match_pele() {
    let f: Fixture = load("url");
    assert!(f.url.len() >= 1000, "only {} cases", f.url.len());
    let o = Schemes::default();
    let mut failed = 0;
    for c in &f.url {
        let sanitized = sanitize_url(&c.input);
        let safe = safe_url(&c.input);
        let link = link_url(&sanitized, &o);
        let image = image_url(&sanitized, &o);
        let mail = link_url(&sanitized, &MAIL);
        let internal = internal_link(&c.input, &o);
        let internal_mail = internal_link(&c.input, &MAIL);
        let ok = sanitized == c.sanitized
            && safe == c.safe
            && link == c.link
            && image == c.image
            && mail == c.mail
            && internal == c.internal
            && internal_mail == c.internal_mail;
        if !ok {
            failed += 1;
            if failed <= 10 {
                eprintln!(
                    "{:?}\n got sanitized {sanitized:?}, safe {safe:?}, link {link:?}, image {image:?}, mail {mail:?}, internal {internal}, internal mail {internal_mail}\nPele {c:?}",
                    c.input
                );
            }
        }
    }
    assert_eq!(failed, 0, "{failed} of {} differ", f.url.len());
}

#[test]
fn links_and_images_may_be_turned_off() {
    let off = Schemes {
        links: false,
        images: false,
        ..Schemes::default()
    };
    assert_eq!(link_url("https://example.com/", &off), None);
    assert_eq!(image_url("https://example.com/", &off), None);
    assert!(!internal_link("Note", &off));
}

#[test]
fn internal_links_are_checked_as_the_browser_reads_them() {
    let o = Schemes::default();
    for name in [
        "Note",
        "Folder\\note",
        "Note%23heading",
        "👩‍💻",
        "Note#heading",
        "  Note  ",
    ] {
        assert!(internal_link(name, &o), "{name:?}");
    }
    let about = Schemes {
        link_schemes: &["about"],
        ..Schemes::default()
    };
    for name in [
        "javascript:alert(1)",
        "java\u{fffe}script:alert(1)",
        "\\\\nevil.example",
        "\t/\\nevil.example",
        "/\t/evil.example",
    ] {
        assert!(!internal_link(name, &about), "{name:?}");
    }
}

#[test]
fn decode_throws_as_decode_uri_component_does() {
    assert_eq!(decode("a%EF%BF%BDb"), "a\u{fffd}b");
    assert_eq!(decode("a%ED%A0%80b"), "a%ED%A0%80b");
    assert_eq!(decode("a%C0%80b"), "a%C0%80b");
    assert_eq!(decode("a%2"), "a%2");
    assert_eq!(sanitize_url("javascript%3Aalert(1)%EF%BF%BD"), BLANK);
}
