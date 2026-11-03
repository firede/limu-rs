//! A look that is theme_only draws a diagram that sets colors in its text as it draws the
//! same diagram without them.

use limu::layout::Options;
use limu::scene::style::Look;
use limu::scene::svg;

/// A diagram that sets colors in its text, the same diagram without the lines that set them,
/// and the classes its author named, which the SVG writer writes as the classes of elements.
struct Case {
    name: &'static str,
    src: &'static str,
    bare: &'static str,
    classes: &'static [&'static str],
}

const CASES: &[Case] = &[
    // classDef fills, a style line on a group that edges run inside and across, which lays
    // casings under them, and a style line on a node.
    Case {
        name: "flowchart",
        src: r##"flowchart LR
  subgraph S[Group]
    A --> B
  end
  B --> C
  D --> A
  classDef hot fill:#c00,stroke:#600,stroke-width:3px,color:#fff
  class C hot
  style S fill:#123456,stroke:#f80,stroke-dasharray:4 2
  style D fill:#ff0,stroke-linecap:round"##,
        bare: r##"flowchart LR
  subgraph S[Group]
    A --> B
  end
  B --> C
  D --> A"##,
        classes: &["hot"],
    },
    Case {
        name: "state",
        src: r##"stateDiagram-v2
  [*] --> Moving
  state Moving {
    Slow --> Fast
  }
  Fast --> Still
  classDef bad fill:#300,color:#fcc
  class Still bad
  style Moving fill:#222"##,
        bare: r##"stateDiagram-v2
  [*] --> Moving
  state Moving {
    Slow --> Fast
  }
  Fast --> Still"##,
        classes: &["bad"],
    },
    Case {
        name: "er",
        src: r##"erDiagram
  subgraph G
    A ||--o{ B : has
  end
  B ||--|| C : is
  C {
    int id
    string name
  }
  style G fill:#004
  style C fill:#fe0,stroke:#000"##,
        bare: r##"erDiagram
  subgraph G
    A ||--o{ B : has
  end
  B ||--|| C : is
  C {
    int id
    string name
  }"##,
        classes: &[],
    },
    Case {
        name: "block",
        src: r##"block-beta
  columns 3
  block:G
    a b
  end
  c
  a --> c
  style G fill:#333
  style c fill:#9cf"##,
        bare: r##"block-beta
  columns 3
  block:G
    a b
  end
  c
  a --> c"##,
        classes: &[],
    },
    Case {
        name: "agentflow",
        src: r##"agentflow-beta TB
  flow reviewer["Review Agent"]
    changes["Gather changes"]@{ shape: input }
    ok["Clean?"]@{ shape: decision }
    changes --> ok
  end
  done["Done"]
  ok --> done
  style reviewer fill:#003
  style done fill:#fd0"##,
        bare: r##"agentflow-beta TB
  flow reviewer["Review Agent"]
    changes["Gather changes"]@{ shape: input }
    ok["Clean?"]@{ shape: decision }
    changes --> ok
  end
  done["Done"]
  ok --> done"##,
        classes: &[],
    },
    Case {
        name: "usecase",
        src: r##"usecase-beta
  direction LR
  classDef system fill:#050
  classDef order fill:#fcf
  actor Customer("Customer")
  systemBoundary Shop["Order system"]:::system
    Checkout("Place order"):::order
  end
  Customer --> Checkout"##,
        bare: r##"usecase-beta
  direction LR
  actor Customer("Customer")
  systemBoundary Shop["Order system"]
    Checkout("Place order")
  end
  Customer --> Checkout"##,
        classes: &["system", "order"],
    },
    Case {
        name: "sequence",
        src: r##"sequenceDiagram
  box rgb(20, 40, 80) Team
  participant A
  participant B
  end
  A->>B: Hello
  rect rgb(0, 0, 0)
  B->>A: Hi
  end"##,
        bare: r##"sequenceDiagram
  box Team
  participant A
  participant B
  end
  A->>B: Hello
  rect
  B->>A: Hi
  end"##,
        classes: &[],
    },
    Case {
        name: "pie",
        src: r##"---
config:
  themeVariables:
    pie1: "#101010"
    pie2: transparent
    pie3: currentColor
---
pie title Pets
  "Dogs": 386
  "Cats": 85
  "Rats": 15"##,
        bare: r##"pie title Pets
  "Dogs": 386
  "Cats": 85
  "Rats": 15"##,
        classes: &[],
    },
    Case {
        name: "treemap",
        src: r##"---
config:
  themeVariables:
    cScale0: "#202020"
    cScale1: "#f0e000"
---
treemap-beta
"Section":::frame
  "a": 10:::dark
  "b": 20
"Other"
  "c": 30
classDef frame fill:#FFF9C4,stroke-width:2px
classDef dark fill:navy"##,
        bare: r##"treemap-beta
"Section"
  "a": 10
  "b": 20
"Other"
  "c": 30"##,
        classes: &["frame", "dark"],
    },
    Case {
        name: "mindmap",
        src: r##"---
config:
  themeVariables:
    cScale1: "#400000"
    cScale2: "#00ff00"
---
mindmap
  root((Root))
    A
      A1
    B"##,
        bare: r##"mindmap
  root((Root))
    A
      A1
    B"##,
        classes: &[],
    },
    Case {
        name: "gitGraph",
        src: r##"---
config:
  themeVariables:
    git0: "#000080"
    git1: "#ffff00"
---
gitGraph
  commit
  branch develop
  commit
  checkout main
  merge develop"##,
        bare: r##"gitGraph
  commit
  branch develop
  commit
  checkout main
  merge develop"##,
        classes: &[],
    },
    Case {
        name: "timeline",
        src: r##"---
config:
  themeVariables:
    cScale0: "#300030"
    cScale1: "#e0ffe0"
---
timeline
  title History
  section Early
    2002 : LinkedIn
  section Later
    2004 : Facebook : Google"##,
        bare: r##"timeline
  title History
  section Early
    2002 : LinkedIn
  section Later
    2004 : Facebook : Google"##,
        classes: &[],
    },
    Case {
        name: "radar",
        src: r##"---
config:
  themeVariables:
    cScale0: "#880000"
    cScale1: "rgba(0, 0, 255, 0.5)"
---
radar-beta
  axis A, B, C
  curve c1{1, 2, 3}
  curve c2{3, 2, 1}"##,
        bare: r##"radar-beta
  axis A, B, C
  curve c1{1, 2, 3}
  curve c2{3, 2, 1}"##,
        classes: &[],
    },
    Case {
        name: "venn",
        src: r##"---
config:
  themeVariables:
    venn1: "#aa0000"
    venn2: "#0000aa"
---
venn-beta
  set A
  set B
  union A,B"##,
        bare: r##"venn-beta
  set A
  set B
  union A,B"##,
        classes: &[],
    },
    Case {
        name: "xychart",
        src: r##"---
config:
  themeVariables:
    xyChart:
      plotColorPalette: "#ff0000, #00aa00"
---
xychart-beta
  x-axis [jan, feb, mar]
  bar [50, 60, 70]
  line [40, 80, 30]"##,
        bare: r##"xychart-beta
  x-axis [jan, feb, mar]
  bar [50, 60, 70]
  line [40, 80, 30]"##,
        classes: &[],
    },
];

/// An SVG document with the classes of elements named left out of their class attributes.
fn without_classes(doc: &str, names: &[&str]) -> String {
    const ATTR: &str = " class=\"";
    let mut out = String::with_capacity(doc.len());
    let mut rest = doc;
    while let Some(at) = rest.find(ATTR) {
        out.push_str(&rest[..at]);
        let value = &rest[at + ATTR.len()..];
        let end = value.find('"').expect("a class attribute that ends");
        let kept: Vec<&str> = value[..end]
            .split_whitespace()
            .filter(|c| !names.contains(c))
            .collect();
        if !kept.is_empty() {
            out.push_str(ATTR);
            out.push_str(&kept.join(" "));
            out.push('"');
        }
        rest = &value[end + 1..];
    }
    out.push_str(rest);
    out
}

fn draw(src: &str, name: &str, look: Look) -> String {
    let sc = limu::layout(src, &Options::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
    svg::render(
        &sc,
        &svg::Options {
            look,
            ..svg::Options::default()
        },
    )
}

// What the author's style lines, classes and palette set, the colors picked for what lies on
// the author's fills, and the casings, are all left out. A look that isn't theme_only draws
// the colors.
#[test]
fn theme_only_draws_as_without_the_authors_colors() {
    let only = Look {
        theme_only: true,
        ..Look::default()
    };
    for c in CASES {
        let got = without_classes(&draw(c.src, c.name, only.clone()), c.classes);
        let want = draw(c.bare, c.name, Look::default());
        assert!(
            got == want,
            "{} drawn theme_only:\n{got}\nwithout its colors:\n{want}",
            c.name
        );
        let drawn = without_classes(&draw(c.src, c.name, Look::default()), c.classes);
        assert!(drawn != got, "{} draws no color of its author's", c.name);
    }
}

// The flowchart above lays casings under the edges that cross its group's fill, which a
// theme_only look doesn't draw.
#[test]
fn theme_only_leaves_out_casings() {
    let sc = limu::layout(CASES[0].src, &Options::default()).expect("a flowchart");
    assert!(
        sc.items
            .iter()
            .any(|it| it.role.as_str().ends_with(".casing")),
        "no casing laid out"
    );
    let look = Look {
        theme_only: true,
        ..Look::default()
    };
    assert!(!look.draws("flowchart.casing") && look.draws("flowchart.edge"));
    assert!(Look::default().draws("flowchart.casing"));
}
