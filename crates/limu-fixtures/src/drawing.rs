//! The fixtures of drawings, as "flowchart-render": what Pele drew of each case, read back out
//! of its SVG as one line per thing drawn, in the order drawn. The oracle keeps the lines of
//! the first cases in full, and of the rest only a digest and the lines a suite keeps.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::{dir, load, try_load};

/// How many cases of a fixture of drawings hold their lines in full. Past them the oracle
/// writes a digest of each case's lines in their place, as FULL in pele-oracle's run.mjs
/// does. The two are the same number.
pub const FULL_CASES: usize = 300;

/// The digest the oracle writes of the lines of a drawing: the first 16 bytes of the SHA-256
/// of the lines, each in UTF-8 followed by a line feed, in lowercase hexadecimal.
pub fn digest(lines: &[String]) -> String {
    let mut h = Sha256::new();
    for line in lines {
        h.update(line.as_bytes());
        h.update(b"\n");
    }
    h.finalize()[..16].iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// What Pele drew of one case of a fixture of drawings: the lines the oracle read out of
/// Pele's SVG, or, past the fixture's first [`FULL_CASES`] cases, a digest of them, with the
/// lines the suite keeps in full by their index. A test flattens it into the type of its
/// cases.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Drawing {
    /// The lines, where the fixture holds them.
    #[serde(default)]
    pub lines: Vec<String>,
    /// The digest of the lines, where the fixture holds that in their place.
    #[serde(default)]
    pub digest: String,
    /// The lines kept in full beside a digest, by their index.
    #[serde(default, deserialize_with = "indices")]
    pub kept: HashMap<usize, String>,
}

/// Reads a map whose keys are indices written as strings, as JSON writes the keys of an
/// object. serde_json reads such keys as numbers itself, but not through the buffer a
/// flattened field is read from.
fn indices<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HashMap<usize, String>, D::Error> {
    HashMap::<String, String>::deserialize(d)?
        .into_iter()
        .map(|(k, v)| {
            k.parse()
                .map(|k| (k, v))
                .map_err(|_| serde::de::Error::custom(format!("no index {k:?}")))
        })
        .collect()
}

impl Drawing {
    /// Line i of what Pele drew where the fixture holds it, and "" where it does not.
    pub fn line(&self, i: usize) -> &str {
        if self.digest.is_empty() {
            return self.lines.get(i).map_or("", String::as_str);
        }
        self.kept.get(&i).map_or("", String::as_str)
    }

    /// Whether got, the lines limu-rs wrote, are the lines of the digest, read with read:
    /// the lines kept in full read as got holds them, and the lines in their places and the
    /// rest of got together those of the digest.
    fn matches(&self, got: &[String], read: &dyn Fn(&str) -> String) -> bool {
        let mut lines = got.to_vec();
        for (&k, line) in &self.kept {
            if k >= got.len() || got[k] != read(line) {
                return false;
            }
            lines[k] = line.clone();
        }
        digest(&lines) == self.digest
    }

    /// The first of the lines kept in full that got does not hold as read reads it, or none
    /// where it holds them all.
    fn first_kept(&self, got: &[String], read: &dyn Fn(&str) -> String) -> Option<usize> {
        self.kept
            .iter()
            .filter(|&(&k, line)| k >= got.len() || got[k] != read(line))
            .map(|(&k, _)| k)
            .min()
    }
}

/// The cases of a file of full lines.
#[derive(Deserialize)]
struct Full {
    cases: Vec<Drawing>,
}

/// A fixture of drawings, as [`load_drawings`] reads it, for comparing what limu-rs draws of
/// its cases with what Pele drew.
pub struct Drawings {
    name: String,
    full: OnceCell<Result<Vec<Drawing>, String>>,
}

/// Reads the fixture of drawings of a name, as "flowchart-render", as a T, as [`load`] does,
/// with what compares the lines of its cases.
pub fn load_drawings<T: DeserializeOwned>(name: &str) -> (T, Drawings) {
    let v = load(name);
    let d = Drawings {
        name: name.to_string(),
        full: OnceCell::new(),
    };
    (v, d)
}

impl Drawings {
    /// The name of the file pele-oracle's `./scripts/oracle.sh --full` writes the lines of
    /// every case of the fixture to, beside it, which a release leaves out.
    fn full_name(&self) -> String {
        format!("{}.full", self.name)
    }

    fn full_path(&self) -> PathBuf {
        dir().join(format!("{}.json.gz", self.full_name()))
    }

    /// The cases of the file of full lines, read once: none where there is no such file.
    fn full_cases(&self) -> &Result<Vec<Drawing>, String> {
        self.full.get_or_init(|| {
            if !self.full_path().exists() {
                return Err(String::new());
            }
            try_load::<Full>(&self.full_name()).map(|f| f.cases)
        })
    }

    /// How got, the lines limu-rs wrote of case i of the fixture, differ from d, what Pele
    /// drew of it, or none where they do not. A test that reads some of Pele's lines through
    /// limu-rs's own code, as the classes it moves its tags last in, gives read, which is
    /// applied to each of Pele's lines before they are compared. The suite keeps in full each
    /// line read changes, as the line cannot be found again from what limu-rs wrote.
    ///
    /// Where the fixture holds the lines, it names the first that differs. Where it holds
    /// only a digest, it takes the lines from the file `./scripts/oracle.sh --full` writes
    /// beside the fixture, where they are those of the digest, and otherwise says how to
    /// write that file.
    pub fn diff(
        &self,
        i: usize,
        d: &Drawing,
        got: &[String],
        read: Option<&dyn Fn(&str) -> String>,
    ) -> Option<String> {
        let same = |line: &str| line.to_string();
        let read = read.unwrap_or(&same);
        if d.digest.is_empty() {
            return line_diff(got, &read_all(&d.lines, read), "");
        }
        if d.matches(got, read) {
            return None;
        }
        let write = format!(
            "pele-oracle's ./scripts/oracle.sh --full <pele> {}, with LIMU_FIXTURES set to its output,",
            self.name
        );
        let full = self.full_path();
        let full = full.display();
        let cases = match self.full_cases() {
            Err(e) if e.is_empty() => {
                let mut msg = format!(
                    "differs from what Pele drew, of which {}.json.gz holds only a digest; {write} writes every line to {full}, to show the first that differs",
                    self.name
                );
                if let Some(k) = d.first_kept(got, read) {
                    let _ = write!(
                        msg,
                        ". Of the lines it holds, this one differs: line {k}\n got {}\nPele {}",
                        at(got, k),
                        read(&d.kept[&k])
                    );
                }
                return Some(msg);
            }
            Err(e) => {
                return Some(format!(
                    "differs from what Pele drew, of which {}.json.gz holds only a digest, and {full} cannot be read: {e}",
                    self.name
                ));
            }
            Ok(cases) => cases,
        };
        if i >= cases.len() || digest(&cases[i].lines) != d.digest {
            return Some(format!(
                "differs from what Pele drew, of which {}.json.gz holds only a digest, and {full} does not hold the lines of that digest: it is stale, and {write} writes it again",
                self.name
            ));
        }
        let note = format!("\n(Pele's lines from {full})");
        if let Some(msg) = line_diff(got, &read_all(&cases[i].lines, read), &note) {
            return Some(msg);
        }
        Some(format!(
            "differs from the digest of what Pele drew in {}.json.gz, but not from its lines in {full}: read changes a line the suite does not keep in full",
            self.name
        ))
    }
}

/// lines, each read with read.
fn read_all(lines: &[String], read: &dyn Fn(&str) -> String) -> Vec<String> {
    lines.iter().map(|l| read(l)).collect()
}

/// Names the first line where got and want differ, with both, and note after them, or none
/// where they are the same.
fn line_diff(got: &[String], want: &[String], note: &str) -> Option<String> {
    let i = first_diff(got, want)?;
    Some(format!(
        "line {i}\n got {}\nPele {}{note}",
        at(got, i),
        at(want, i)
    ))
}

/// The index of the first line where a and b differ, or none where they are the same.
pub fn first_diff(a: &[String], b: &[String]) -> Option<usize> {
    (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i))
}

/// Line i of lines, or "(none)" past their end.
pub fn at(lines: &[String], i: usize) -> &str {
    lines.get(i).map_or("(none)", String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digests are those pele-oracle's run.mjs writes, as Node computes them.
    #[test]
    fn digests() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        for (lines, want) in [
            (s(&[]), "e3b0c44298fc1c149afbf4c8996fb924"),
            (
                s(&["size 1,2", "t 3,4 é 😀", ""]),
                "834aae1f3304de4651cb95866c13e7a6",
            ),
            // A lone surrogate, which JSON escapes and both Node and serde_json read as
            // U+FFFD.
            (s(&["\u{fffd}"]), "8d75cfafa290dea108e554948eae67ba"),
        ] {
            assert_eq!(digest(&lines), want, "{lines:?}");
        }
    }

    #[test]
    fn diffs() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // read moves a tag last, as the tests of flowcharts read Pele's lines, and the suite
        // keeps the line it moves one in.
        let lines = s(&["size 10,10", "node a [clickable x]", "edge a-b M0,0"]);
        let read = |line: &str| line.replacen("[clickable x]", "[x clickable]", 1);
        let got = s(&["size 10,10", "node a [x clickable]", "edge a-b M0,0"]);
        let other = s(&["size 10,10", "node a [x clickable]", "edge a-b M1,1"]);
        let full = Drawing {
            lines: lines.clone(),
            ..Drawing::default()
        };
        let digested = Drawing {
            digest: digest(&lines),
            kept: HashMap::from([(1, lines[1].clone())]),
            ..Drawing::default()
        };
        assert_eq!(digested.line(1), lines[1]);
        assert_eq!(digested.line(2), "");
        // A name with no file of full lines beside it.
        let f = Drawings {
            name: "no-such-suite".into(),
            full: OnceCell::new(),
        };
        let mut more = got.clone();
        more.push("x".into());
        for (name, d, got, want) in [
            ("lines, the same", &full, &got, ""),
            (
                "lines, differing",
                &full,
                &other,
                "line 2\n got edge a-b M1,1\nPele edge a-b M0,0",
            ),
            ("digest, the same", &digested, &got, ""),
            ("digest, one line more", &digested, &more, "only a digest"),
            (
                "digest, a kept line differing",
                &digested,
                &s(&["size 10,10", "node a", "edge a-b M0,0"]),
                "to show the first that differs. Of the lines it holds, this one differs: line 1\n got node a\nPele node a [x clickable]",
            ),
            (
                "digest, differing",
                &digested,
                &other,
                "./scripts/oracle.sh --full <pele> no-such-suite",
            ),
        ] {
            let msg = f.diff(1, d, got, Some(&read)).unwrap_or_default();
            assert!(
                if want.is_empty() {
                    msg.is_empty()
                } else {
                    msg.contains(want)
                },
                "{name}: {msg:?}, not {want:?}"
            );
        }

        // The full file holds the lines of the digest, and shows which differs.
        let with = |cases: Vec<Drawing>| Drawings {
            name: "x".into(),
            full: OnceCell::from(Ok(cases)),
        };
        let f = with(vec![full.clone(), full.clone()]);
        let msg = f.diff(1, &digested, &other, Some(&read)).unwrap();
        assert!(
            msg.starts_with("line 2\n got edge a-b M1,1\nPele edge a-b M0,0\n(Pele's lines from "),
            "{msg:?}"
        );
        // read changes a line the suite does not keep: the lines of the full file match.
        let changes = |line: &str| read(&line.replace("M0,0", "M1,1"));
        let msg = f.diff(1, &digested, &other, Some(&changes)).unwrap();
        assert!(msg.contains("read changes a line"), "{msg:?}");

        // A full file that holds other lines is stale.
        let short = Drawing {
            lines: lines[..2].to_vec(),
            ..Drawing::default()
        };
        let f = with(vec![full.clone(), short]);
        let msg = f.diff(1, &digested, &other, Some(&read)).unwrap();
        assert!(msg.contains("it is stale, and "), "{msg:?}");
    }

    /// The cases of a fixture of drawings read with a Drawing flattened into each, as the
    /// tests of the diagrams read them: the first cases with their lines, the rest with a
    /// digest.
    #[test]
    fn flowchart_render_reads() {
        #[derive(Deserialize)]
        struct Case {
            #[allow(dead_code)]
            src: String,
            #[serde(flatten)]
            drawing: Drawing,
        }
        #[derive(Deserialize)]
        struct Render {
            cases: Vec<Case>,
        }
        let (r, _): (Render, Drawings) = load_drawings("flowchart-render");
        assert!(r.cases.len() > FULL_CASES, "only {} cases", r.cases.len());
        for (i, c) in r.cases.iter().enumerate() {
            let d = &c.drawing;
            assert_eq!(d.digest.is_empty(), i < FULL_CASES, "case {i}");
            assert_eq!(d.lines.is_empty(), i >= FULL_CASES, "case {i}");
        }
        let kept = r.cases.iter().find(|c| !c.drawing.kept.is_empty()).unwrap();
        let (&k, line) = kept.drawing.kept.iter().next().unwrap();
        assert_eq!(kept.drawing.line(k), line);
    }
}
