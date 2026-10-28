//! HTML's entities and Mermaid's codes for characters, as Pele's src/text/entities.ts decodes
//! them.

use std::collections::HashMap;
use std::sync::OnceLock;

const NAMED: &str = "amp:38,lt:60,gt:62,quot:34,apos:39,nbsp:160,shy:173,iexcl:161,cent:162,\
    pound:163,curren:164,yen:165,sect:167,copy:169,laquo:171,not:172,reg:174,deg:176,plusmn:177,\
    sup2:178,sup3:179,micro:181,para:182,middot:183,sup1:185,raquo:187,frac14:188,frac12:189,\
    frac34:190,iquest:191,times:215,divide:247,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,\
    sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,\
    permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,euro:8364,trade:8482,larr:8592,\
    uarr:8593,rarr:8594,darr:8595,harr:8596,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,\
    forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,\
    prod:8719,sum:8721,minus:8722,radic:8730,infin:8734,and:8743,or:8744,cap:8745,cup:8746,\
    int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,\
    sub:8834,sup:8835,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,loz:9674,\
    spades:9824,clubs:9827,hearts:9829,diams:9830,check:10003,cross:10007,AMP:38,LT:60,GT:62,\
    QUOT:34,lsqb:91,rsqb:93,lcub:123,rcub:125,verbar:124,VerticalLine:124,midast:42,\
    DiacriticalGrave:96,Hat:94,UnderBar:95,equals:61,colon:58,semi:59,num:35,percnt:37,lpar:40,\
    rpar:41,lbrack:91,rbrack:93,lbrace:123,rbrace:125,vert:124,sol:47,bsol:92,quest:63,excl:33,\
    ast:42,plus:43,comma:44,period:46,dollar:36,commat:64,grave:96,hat:94,lowbar:95,tilde:126,\
    Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,\
    Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,\
    Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,\
    epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,\
    omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,\
    omega:969";

/// The character an entity names, as Pele's lookup finds it.
fn lookup(name: &str) -> Option<char> {
    static NAMES: OnceLock<HashMap<&'static str, char>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            NAMED
                .split(',')
                .filter_map(|pair| {
                    let (name, code) = pair.split_once(':')?;
                    Some((name, char::from_u32(code.parse().ok()?)?))
                })
                .collect()
        })
        .get(name)
        .copied()
}

/// The character n is the code of, as Pele's codePoint gives it, or none where n is not one.
fn code_point(n: f64) -> Option<char> {
    if n > 0.0 && n <= 1114111.0 {
        char::from_u32(n as u32)
    } else {
        None
    }
}

const OPEN: &str = "ﬂ°";
const CLOSE: &str = "¶ß";

/// Turns Mermaid's placeholders, which run from `ﬂ°` to `¶ß` with a second `°` marking a
/// numeric code, back into entities, so that each code is decoded once, as Pele's
/// restorePlaceholders does, and Mermaid.
fn restore_placeholders(text: &str) -> String {
    let mut out = String::new();
    let mut last = 0;
    let mut from = text.find(OPEN);
    while let Some(at) = from {
        let body = at + OPEN.len();
        let Some(stop) = text[body..].find('¶').map(|n| body + n) else {
            break;
        };
        let after = stop + '¶'.len_utf8();
        if text[after..].starts_with('ß') {
            out.push_str(&text[last..at]);
            match text[body..stop].strip_prefix('°') {
                Some(code) => out.push_str(&format!("&#{code};")),
                None => out.push_str(&format!("&{};", &text[body..stop])),
            }
            last = after + 'ß'.len_utf8();
        }
        from = text[after..].find(OPEN).map(|n| after + n);
    }
    if last == 0 {
        return text.to_string();
    }
    out.push_str(&text[last..]);
    out
}

/// Turns Mermaid's `#35;` and `#quot;` codes, as Mermaid's encodeEntities hides them, and
/// HTML's entities into characters, as Pele's decodeEntities does.
pub(crate) fn decode_entities(text: &str) -> String {
    let restored;
    let text = if text.contains(CLOSE) {
        restored = restore_placeholders(text);
        restored.as_str()
    } else {
        text
    };
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while let Some(n) = text[i..].find('&') {
        let at = i + n;
        match entity_at(text, at) {
            Some((end, c)) => {
                out.push_str(&text[last..at]);
                match c {
                    Some(c) => out.push(c),
                    None => out.push_str(&text[at..end]),
                }
                last = end;
                i = end;
            }
            None => i = at + 1,
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Matches `/&(?:#(\d+)|#[xX]([0-9a-fA-F]+)|(\w+));/` at byte i of s, and gives where the
/// match ends and the character it stands for, if it stands for one.
fn entity_at(s: &str, i: usize) -> Option<(usize, Option<char>)> {
    let b = s.as_bytes();
    let run = |from: usize, keep: fn(&u8) -> bool| {
        from + b[from.min(b.len())..]
            .iter()
            .take_while(|c| keep(c))
            .count()
    };
    let j = i + 1;
    if b.get(j) == Some(&b'#') {
        let k = run(j + 1, u8::is_ascii_digit);
        if k > j + 1 && b.get(k) == Some(&b';') {
            // A number too long for a double is Infinity, which names no character.
            let n: f64 = s[j + 1..k].parse().unwrap_or(f64::INFINITY);
            return Some((k + 1, code_point(n)));
        }
        if matches!(b.get(j + 1), Some(b'x' | b'X')) {
            let k = run(j + 2, u8::is_ascii_hexdigit);
            if k > j + 2 && b.get(k) == Some(&b';') {
                let n = crate::js::number::parse_int(&s[j + 2..k], 16);
                return Some((k + 1, code_point(n)));
            }
        }
        return None;
    }
    let k = run(j, |c| c.is_ascii_alphanumeric() || *c == b'_');
    if k == j || b.get(k) != Some(&b';') {
        return None;
    }
    Some((k + 1, lookup(&s[j..k])))
}
