//! Dates checked against what V8, running Pele, makes of the same dates in each zone of the
//! oracle's jsdate fixture.

use limu_fixtures::{Num, load};
use serde::Deserialize;

use super::{Zone, utc, utc_seconds};
use crate::testing::Failures;
use crate::testing::tzif::zone;

/// A number as the fixture writes it, null for NaN.
#[derive(Clone, Copy, Deserialize)]
#[serde(from = "Option<f64>")]
struct N(f64);

impl From<Option<f64>> for N {
    fn from(v: Option<f64>) -> N {
        N(v.unwrap_or(f64::NAN))
    }
}

/// The argument of a setter: one number, or several.
#[derive(Deserialize)]
#[serde(untagged)]
enum Args {
    One(N),
    Many(Vec<N>),
}

#[derive(Deserialize)]
struct Zones {
    zones: Vec<Cases>,
}

#[derive(Deserialize)]
struct Cases {
    zone: String,
    parse: Vec<(String, N)>,
    fields: Vec<(N, Vec<N>)>,
    built: Vec<(Vec<N>, N, N)>,
    set: Vec<(N, String, Args, N)>,
}

fn same(a: f64, b: f64) -> bool {
    Num::same(a, b)
}

#[test]
fn dates_match_v8() {
    let f: Zones = load("jsdate");
    let mut failures = Failures::new();
    for zc in &f.zones {
        let tz = zone(&zc.zone);
        let z = Zone(&tz);
        let name = &zc.zone;
        for (s, want) in &zc.parse {
            let got = z.parse(s);
            failures.check(same(got, want.0), || {
                format!("{name}: parse({s:?}) = {got}, V8 {}", want.0)
            });
        }
        for (t, want) in &zc.fields {
            let t = t.0;
            let got = [
                z.full_year(t),
                z.month(t),
                z.date(t),
                z.day(t),
                z.hours(t),
                z.minutes(t),
                z.seconds(t),
                z.milliseconds(t),
                z.timezone_offset(t),
                utc_seconds(t),
            ];
            let ok = got.len() == want.len() && got.iter().zip(want).all(|(&a, b)| same(a, b.0));
            failures.check(ok, || {
                let want: Vec<f64> = want.iter().map(|n| n.0).collect();
                format!("{name}: fields of {t} = {got:?}, V8 {want:?}")
            });
        }
        for (a, local, at_utc) in &zc.built {
            let a: Vec<f64> = a.iter().map(|n| n.0).collect();
            let got = z.local(a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
            failures.check(same(got, local.0), || {
                format!("{name}: new Date({a:?}) = {got}, V8 {}", local.0)
            });
            let got = utc(a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
            failures.check(same(got, at_utc.0), || {
                format!("{name}: Date.UTC({a:?}) = {got}, V8 {}", at_utc.0)
            });
        }
        for (t, op, args, want) in &zc.set {
            let t = t.0;
            let a: Vec<f64> = match args {
                Args::One(n) => vec![n.0],
                Args::Many(v) => v.iter().map(|n| n.0).collect(),
            };
            let got = match op.as_str() {
                "date" => z.set_date(t, a[0]),
                "month" => z.set_month(t, a[0]),
                "monthDate" => z.set_month_date(t, a[0], a[1]),
                "fullYear" => z.set_full_year(t, a[0]),
                "hours" => z.set_hours(t, a[0], a[1], a[2], a[3]),
                _ => f64::NAN,
            };
            failures.check(same(got, want.0), || {
                format!("{name}: set {op}({t}, {a:?}) = {got}, V8 {}", want.0)
            });
        }
        assert!(
            zc.parse.len() >= 5000 && zc.fields.len() >= 3000 && zc.built.len() >= 3000,
            "{name}: too few cases"
        );
    }
    failures.done("jsdate", 5 * 11000);
}
