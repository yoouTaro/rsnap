//! Render the README and site images the same way the app does:
//! `cargo run -p rsnap-core --example demo -- in.png out.png [backdrop] [marks]`
//! where marks is `rgrip` (the split-view tour), `rjot`, or `light` (one arrow).

use rsnap_core::doc::{Annotation, BACKDROPS, COLORS, Doc, Pt, Shape, Style};
use rsnap_core::render;
use rsnap_core::text::Fonts;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = image::open(&args[1]).expect("input image").to_rgba8();
    let backdrop = args.get(3).and_then(|n| BACKDROPS.iter().find(|(name, _)| name == n)).map(|b| b.1).unwrap_or(BACKDROPS[1].1);
    let (w, h) = (src.width() as f32, src.height() as f32);
    let p = |x: f32, y: f32| Pt::new(x * w, y * h);
    let a = |shape, c: usize| Annotation { shape, color: COLORS[c] };
    let annotations = match args.get(4).map(String::as_str).unwrap_or("rjot") {
        "rgrip" => vec![
            a(Shape::Highlight { a: p(0.188, 0.217), b: p(0.594, 0.243) }, 2),
            a(Shape::Highlight { a: p(0.188, 0.364), b: p(0.594, 0.391) }, 2),
            a(Shape::Rect { a: p(0.665, 0.175), b: p(0.77, 0.255) }, 4),
            a(Shape::Arrow { from: p(0.56, 0.6), to: p(0.69, 0.27) }, 0),
            a(Shape::Step { at: p(0.19, 0.205), n: 1 }, 0),
            a(Shape::Step { at: p(0.775, 0.165), n: 2 }, 0),
            a(Shape::Text { at: p(0.35, 0.61), text: "Selected rows light up on the map".into() }, 0),
            a(Shape::Redact { a: p(0.075, 0.976), b: p(0.27, 0.997) }, 5),
        ],
        "light" => vec![
            a(Shape::Arrow { from: p(0.25, 0.62), to: p(0.5, 0.42) }, 0),
            a(Shape::Step { at: p(0.9, 0.2), n: 1 }, 0),
        ],
        _ => vec![
            a(Shape::Highlight { a: p(0.03, 0.195), b: p(0.97, 0.29) }, 2),
            a(Shape::Rect { a: p(0.02, 0.335), b: p(0.98, 0.47) }, 4),
            a(Shape::Arrow { from: p(0.62, 0.66), to: p(0.86, 0.48) }, 0),
            a(Shape::Step { at: p(0.94, 0.2), n: 1 }, 0),
            a(Shape::Text { at: p(0.2, 0.67), text: "Pinned, with its page".into() }, 0),
        ],
    };
    let doc = Doc { annotations, crop: None, style: Style { backdrop, padding: 0.09, radius: 0.025, shadow: true } };
    let out = render::render(&src, &doc, Fonts::system().as_ref());
    std::fs::write(&args[2], render::to_png(&out).expect("png")).expect("write");
}
