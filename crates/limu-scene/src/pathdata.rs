//! Reading path data, as SVG's d attribute holds it.

use std::fmt;

use crate::geom::Point;
use crate::path::Path;

/// What is wrong with path data that does not follow SVG's grammar, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathError {
    /// What was expected or found.
    pub message: &'static str,
    /// Where, as an offset in bytes.
    pub at: usize,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "path data: {} at {}", self.message, self.at)
    }
}

impl std::error::Error for PathError {}

impl Path {
    /// Reads path data, as SVG's d attribute holds it: the commands M, L, H, V, C, S, Q, T, A
    /// and Z, each also in lower case for numbers relative to where the path is, with numbers
    /// repeated for a command given once. Diagrams use it for the outlines Pele draws from
    /// path data of its own, as the icons of architecture and tree view diagrams.
    pub fn parse(d: &str) -> Result<Path, PathError> {
        let mut r = Reader {
            s: d.as_bytes(),
            i: 0,
        };
        let mut p = Path::new();
        let mut cmd = 0u8;
        // The control point a smooth curve reflects, and the kind of curve that set it.
        let mut ctrl = Point::default();
        let mut ctrl_of = 0u8;
        r.space();
        while r.i < r.s.len() {
            let c = r.s[r.i];
            if args(c & !0x20).is_some() {
                cmd = c;
                r.i += 1;
            } else if cmd == 0 || cmd == b'Z' || cmd == b'z' {
                return Err(r.error("number without a command"));
            } else if cmd == b'M' {
                cmd = b'L'; // coordinates after a moveto's first are lines
            } else if cmd == b'm' {
                cmd = b'l';
            }
            let at = p.last();
            let (ox, oy) = if cmd >= b'a' {
                (at.x, at.y)
            } else {
                (0.0, 0.0)
            };
            let mut kind = cmd & !0x20;
            let mut n = [0.0; 7];
            for (k, v) in n.iter_mut().enumerate().take(args(kind).unwrap_or(0)) {
                r.comma(k > 0);
                *v = if kind == b'A' && (k == 3 || k == 4) {
                    r.flag()?
                } else {
                    r.number()?
                };
            }
            match kind {
                b'M' => {
                    p.move_to(ox + n[0], oy + n[1]);
                }
                b'L' => {
                    p.line_to(ox + n[0], oy + n[1]);
                }
                b'H' => {
                    p.line_to(ox + n[0], at.y);
                }
                b'V' => {
                    p.line_to(at.x, oy + n[0]);
                }
                b'C' => {
                    p.cube_to(
                        ox + n[0],
                        oy + n[1],
                        ox + n[2],
                        oy + n[3],
                        ox + n[4],
                        oy + n[5],
                    );
                    ctrl = Point::new(ox + n[2], oy + n[3]);
                }
                b'S' => {
                    let c1 = if ctrl_of == b'C' {
                        Point::new(2.0 * at.x - ctrl.x, 2.0 * at.y - ctrl.y)
                    } else {
                        at
                    };
                    p.cube_to(c1.x, c1.y, ox + n[0], oy + n[1], ox + n[2], oy + n[3]);
                    ctrl = Point::new(ox + n[0], oy + n[1]);
                    kind = b'C';
                }
                b'Q' => {
                    p.quad_to(ox + n[0], oy + n[1], ox + n[2], oy + n[3]);
                    ctrl = Point::new(ox + n[0], oy + n[1]);
                }
                b'T' => {
                    let c = if ctrl_of == b'Q' {
                        Point::new(2.0 * at.x - ctrl.x, 2.0 * at.y - ctrl.y)
                    } else {
                        at
                    };
                    p.quad_to(c.x, c.y, ox + n[0], oy + n[1]);
                    ctrl = c;
                    kind = b'Q';
                }
                b'A' => {
                    if p.is_empty() {
                        p.move_to(0.0, 0.0);
                    }
                    p.arc_to(
                        n[0],
                        n[1],
                        n[2],
                        n[3] != 0.0,
                        n[4] != 0.0,
                        ox + n[5],
                        oy + n[6],
                    );
                }
                _ => {
                    p.close();
                }
            }
            ctrl_of = kind;
            r.space();
            if r.i < r.s.len() && r.s[r.i] == b',' {
                if kind == b'Z' {
                    return Err(r.error("comma after a closepath"));
                }
                r.i += 1;
                r.space();
            }
        }
        Ok(p)
    }
}

/// How many numbers a command in upper case takes, or none for no command.
fn args(c: u8) -> Option<usize> {
    match c {
        b'Z' => Some(0),
        b'H' | b'V' => Some(1),
        b'M' | b'L' | b'T' => Some(2),
        b'S' | b'Q' => Some(4),
        b'C' => Some(6),
        b'A' => Some(7),
        _ => None,
    }
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn error(&self, message: &'static str) -> PathError {
        PathError {
            message,
            at: self.i,
        }
    }

    fn space(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
        {
            self.i += 1;
        }
    }

    /// Skips the space, and between numbers a comma, that may come before a number.
    fn comma(&mut self, between: bool) {
        self.space();
        if between && self.i < self.s.len() && self.s[self.i] == b',' {
            self.i += 1;
            self.space();
        }
    }

    /// Reads a number as SVG writes one: a sign, digits with a point among or before them,
    /// and an exponent. It ends where the next character can't continue it, so "1-2" is two
    /// numbers and ".5.5" is two too.
    fn number(&mut self) -> Result<f64, PathError> {
        let (from, s) = (self.i, self.s);
        let mut i = self.i;
        let digits = |i: &mut usize| {
            let start = *i;
            while *i < s.len() && s[*i].is_ascii_digit() {
                *i += 1;
            }
            *i - start
        };
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let mut n = digits(&mut i);
        if i < s.len() && s[i] == b'.' {
            i += 1;
            n += digits(&mut i);
        }
        if n == 0 {
            return Err(self.error("number expected"));
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let j = i;
            i += 1;
            if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
                i += 1;
            }
            if digits(&mut i) == 0 {
                i = j; // an e that no digits follow ends the number before it
            }
        }
        // The bytes are ASCII, and in the grammar Rust's parse reads.
        let text = std::str::from_utf8(&s[from..i]).unwrap_or_default();
        let v = text
            .parse::<f64>()
            .map_err(|_| self.error("number expected"))?;
        if !v.is_finite() {
            return Err(self.error("number out of range"));
        }
        self.i = i;
        Ok(v)
    }

    /// Reads an arc's flag, a single 0 or 1 that needs nothing to part it from what follows.
    fn flag(&mut self) -> Result<f64, PathError> {
        match self.s.get(self.i) {
            Some(&c @ (b'0' | b'1')) => {
                self.i += 1;
                Ok(f64::from(c - b'0'))
            }
            _ => Err(self.error("flag expected")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(f: impl FnOnce(&mut Path) -> &mut Path) -> Path {
        let mut p = Path::new();
        f(&mut p);
        p
    }

    #[test]
    fn parse() {
        for (d, want) in [
            (
                "M3 4h18v7H3z",
                path(|p| {
                    p.move_to(3.0, 4.0)
                        .line_to(21.0, 4.0)
                        .line_to(21.0, 11.0)
                        .line_to(3.0, 11.0)
                        .close()
                }),
            ),
            // Numbers repeated after a moveto are lines. A sign or a second point parts
            // numbers.
            (
                "m1,2 3-4 .5.5",
                path(|p| p.move_to(1.0, 2.0).line_to(4.0, -2.0).line_to(4.5, -1.5)),
            ),
            (
                "M7 7.5h.01",
                path(|p| p.move_to(7.0, 7.5).line_to(7.01, 7.5)),
            ),
            // After a closepath, relative numbers start from the figure's start.
            (
                "M1 1L5 1 5 5Zl1 0",
                path(|p| {
                    p.move_to(1.0, 1.0)
                        .line_to(5.0, 1.0)
                        .line_to(5.0, 5.0)
                        .close()
                        .line_to(2.0, 1.0)
                }),
            ),
            (
                "M0 0C1 0 2 1 2 2S3 4 4 4",
                path(|p| {
                    p.move_to(0.0, 0.0)
                        .cube_to(1.0, 0.0, 2.0, 1.0, 2.0, 2.0)
                        .cube_to(2.0, 3.0, 3.0, 4.0, 4.0, 4.0)
                }),
            ),
            (
                "M0 0Q1 1 2 0T4 0",
                path(|p| {
                    p.move_to(0.0, 0.0)
                        .quad_to(1.0, 1.0, 2.0, 0.0)
                        .quad_to(3.0, -1.0, 4.0, 0.0)
                }),
            ),
            (
                "M0 0S1 1 2 2",
                path(|p| p.move_to(0.0, 0.0).cube_to(0.0, 0.0, 1.0, 1.0, 2.0, 2.0)),
            ),
            ("M1e1 2E-1", path(|p| p.move_to(10.0, 0.2))),
            // Flags need nothing between them.
            (
                "M10 0a10 10 0 0110-10",
                path(|p| {
                    p.move_to(10.0, 0.0)
                        .arc_to(10.0, 10.0, 0.0, false, true, 20.0, -10.0)
                }),
            ),
            (
                "M4 6a8 3 0 1 0 16 0",
                path(|p| {
                    p.move_to(4.0, 6.0)
                        .arc_to(8.0, 3.0, 0.0, true, false, 20.0, 6.0)
                }),
            ),
        ] {
            assert_eq!(Path::parse(d), Ok(want), "{d:?}");
        }
        for d in [
            "1 2",
            "M1",
            "M1 2z3",
            "M1 2 x",
            "M0 0A1 1 0 2 0 1 1",
            "M1 2z,",
            "M1e999 0",
        ] {
            assert!(Path::parse(d).is_err(), "{d:?}: no error");
        }
    }
}
