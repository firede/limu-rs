//! The functions the page calls. The page writes a diagram's source into the room
//! [`input`] makes, calls [`show`], and reads the JSON it wrote from [`output`] and
//! [`output_len`]. A panic writes what it says to the output before the module traps, so the
//! page can show it, and the page then starts the module anew.

#![allow(unsafe_code)]

use std::cell::RefCell;

use limu_scene::style::Theme;

use crate::{Request, View};

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Sets the output to what a panic says, for the page to read once the module traps.
#[unsafe(no_mangle)]
pub extern "C" fn start() {
    std::panic::set_hook(Box::new(|info| {
        let said = serde_json::json!({ "panic": info.to_string() }).to_string();
        OUTPUT.with(|o| {
            if let Ok(mut o) = o.try_borrow_mut() {
                *o = said;
            }
        });
    }));
}

/// Makes room for a source of a number of bytes of UTF-8, and gives where it starts, for the
/// page to write it there.
#[unsafe(no_mangle)]
pub extern "C" fn input(len: usize) -> *mut u8 {
    INPUT.with_borrow_mut(|b| {
        b.clear();
        b.resize(len, 0);
        b.as_mut_ptr()
    })
}

/// Shows the source written into the input as the page asks: in a view of 0 for SVG, 1 for
/// text and 2 for the scene, a theme of 0 for Pele's, 1 for Kohu and 2 for Kohu's dark one,
/// in ASCII alone where ascii is not 0, laid out for a number of columns, with now as today.
#[unsafe(no_mangle)]
pub extern "C" fn show(view: u32, theme: u32, ascii: u32, columns: f64, now: f64) {
    let src = INPUT.with_borrow(|b| String::from_utf8_lossy(b).into_owned());
    let r = Request {
        view: match view {
            1 => View::Text,
            2 => View::Scene,
            _ => View::Svg,
        },
        theme: match theme {
            1 => Theme::KOHU,
            2 => Theme::KOHU_DARK,
            _ => Theme::PELE,
        },
        ascii: ascii != 0,
        columns,
        now,
    };
    let out = crate::show(&src, &r);
    OUTPUT.set(out);
}

/// Writes the samples' sources into the output.
#[unsafe(no_mangle)]
pub extern "C" fn samples() {
    OUTPUT.set(crate::samples());
}

/// Where the output starts.
#[unsafe(no_mangle)]
pub extern "C" fn output() -> *const u8 {
    OUTPUT.with_borrow(|o| o.as_ptr())
}

/// How many bytes of UTF-8 the output is.
#[unsafe(no_mangle)]
pub extern "C" fn output_len() -> usize {
    OUTPUT.with_borrow(|o| o.len())
}
