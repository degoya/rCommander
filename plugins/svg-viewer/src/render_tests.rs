use super::*;
use crate::settings::{Background, Size};
use crate::test_support::{VecSink, data_url, gzip, nested, painted, pixel, png, use_bomb};

const SVG_NS: &str =
    r#"xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink""#;
const RED: [u8; 4] = [255, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

fn small() -> Settings {
    Settings {
        size: Size::Small,
        background: Background::Transparent,
    }
}

/// A `width` by `height` document around `body`.
fn svg(width: u32, height: u32, body: &str) -> Vec<u8> {
    format!(r#"<svg {SVG_NS} width="{width}" height="{height}">{body}</svg>"#).into_bytes()
}

fn draw(svg: &[u8], settings: Settings) -> Pixmap {
    render(svg, settings).expect("the picture renders")
}

fn code_of(result: Result<Pixmap, Failure>) -> Option<String> {
    result.err().and_then(|failure| failure.code)
}

#[test]
fn a_path_fills_its_shape_and_nothing_else() {
    // 100 user units on 512 pixels: 5.12 pixels a unit.
    let picture = draw(
        &svg(100, 100, r#"<path d="M0 0 L100 0 L0 100 Z" fill="red"/>"#),
        small(),
    );
    assert_eq!((picture.width(), picture.height()), (512, 512));
    assert_eq!(pixel(&picture, 51, 51), RED);
    assert_eq!(pixel(&picture, 460, 460), CLEAR);
}

#[test]
fn a_gradient_runs_from_its_first_colour_to_its_last() {
    let body = r#"<defs><linearGradient id="g" x1="0" x2="1" y1="0" y2="0">
        <stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/>
        </linearGradient></defs><rect width="100" height="50" fill="url(#g)"/>"#;
    let picture = draw(&svg(100, 50, body), small());
    let [r, _, b, a] = pixel(&picture, 2, 128);
    assert!(r > 240 && b < 20 && a == 255, "left {r} {b} {a}");
    let [r, _, b, a] = pixel(&picture, 509, 128);
    assert!(r < 20 && b > 240 && a == 255, "right {r} {b} {a}");
    let [r, _, b, _] = pixel(&picture, 256, 128);
    assert!(r > 100 && r < 160 && b > 100 && b < 160, "middle {r} {b}");
}

#[test]
fn a_transform_moves_and_scales_its_shape() {
    // 50 units wide, halved and moved to 50: drawn from 50 to 75.
    let body =
        r#"<rect width="50" height="100" fill="red" transform="translate(50 0) scale(0.5 1)"/>"#;
    let picture = draw(&svg(100, 100, body), small());
    assert_eq!(pixel(&picture, 100, 256), CLEAR);
    assert_eq!(pixel(&picture, 320, 256), RED);
    assert_eq!(pixel(&picture, 435, 256), CLEAR);
}

#[test]
fn a_view_box_without_a_size_gives_the_size_and_the_aspect_ratio() {
    let document = format!(
        r#"<svg {SVG_NS} viewBox="0 0 200 100"><rect x="100" width="100" height="100" fill="red"/></svg>"#
    );
    let picture = draw(document.as_bytes(), Settings::default());
    assert_eq!((picture.width(), picture.height()), (1024, 512));
    assert_eq!(pixel(&picture, 100, 256), CLEAR);
    assert_eq!(pixel(&picture, 900, 256), RED);
}

#[test]
fn text_is_set_in_the_bundled_font_whatever_the_family() {
    let text = |family: &str| {
        let body = format!(
            r#"<text x="5" y="80" font-size="80" font-family="{family}" fill="black">Hg</text>"#
        );
        draw(&svg(200, 100, &body), small())
    };
    let unknown = text("No Such Family");
    assert!(painted(&unknown) > 2000, "{}", painted(&unknown));
    for family in ["serif", "sans-serif", "monospace", "Arial", "Lato"] {
        assert_eq!(text(family).data(), unknown.data(), "{family}");
    }
}

#[test]
fn a_data_png_is_drawn() {
    let href = data_url("image/png", &png(2, 2, RED));
    let body = format!(r#"<image href="{href}" width="100" height="100"/>"#);
    let picture = draw(&svg(100, 100, &body), small());
    assert_eq!(pixel(&picture, 256, 256), RED);
}

#[test]
fn each_size_is_the_longest_side() {
    let document = svg(100, 50, "");
    for (size, expected) in [
        (Size::Small, (512, 256)),
        (Size::Medium, (1024, 512)),
        (Size::Large, (2048, 1024)),
    ] {
        let picture = draw(
            &document,
            Settings {
                size,
                background: Background::Transparent,
            },
        );
        assert_eq!((picture.width(), picture.height()), expected);
    }
}

#[test]
fn each_background_fills_what_the_picture_leaves_empty() {
    let document = svg(10, 10, r#"<rect width="5" height="10" fill="red"/>"#);
    for (background, expected) in [
        (Background::Transparent, CLEAR),
        (Background::White, [255, 255, 255, 255]),
        (Background::Black, [0, 0, 0, 255]),
    ] {
        let picture = draw(
            &document,
            Settings {
                size: Size::Small,
                background,
            },
        );
        assert_eq!(pixel(&picture, 500, 256), expected, "{background:?}");
        assert_eq!(pixel(&picture, 10, 256), RED, "{background:?}");
    }
}

#[test]
fn the_raster_is_straight_rgba_in_writes_of_at_most_a_mebibyte() {
    let document = svg(
        100,
        100,
        r#"<rect width="100" height="100" fill="red" fill-opacity="0.5"/>"#,
    );
    let picture = draw(&document, Settings::default());
    let mut sink = VecSink::default();
    write_rgba(&picture, &mut sink).expect("written");
    assert_eq!(sink.0.len(), 1024 * 1024 * 4);
    let [r, g, b, a] = [sink.0[0], sink.0[1], sink.0[2], sink.0[3]];
    assert!(r >= 250 && g == 0 && b == 0, "{r} {g} {b}");
    assert!((126..=129).contains(&a), "{a}");
}

#[test]
fn an_external_href_is_not_loaded() {
    // A real picture on disk, which usvg's own resolver would read.
    let path = std::env::temp_dir().join(format!("rc-svg-viewer-{}.png", std::process::id()));
    std::fs::write(&path, png(2, 2, RED)).expect("the test picture is written");
    let file = path.to_string_lossy().into_owned();
    let hrefs = [
        file.clone(),
        format!("file://{file}"),
        "http://127.0.0.1:9/red.png".to_owned(),
        "https://example.invalid/red.png".to_owned(),
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ];
    for href in hrefs {
        for attribute in ["href", "xlink:href"] {
            let body = format!(r#"<image {attribute}="{href}" width="100" height="100"/>"#);
            let picture = draw(&svg(100, 100, &body), small());
            assert_eq!(painted(&picture), 0, "{attribute}={href}");
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_gzip_stream_inside_a_data_url_is_not_inflated() {
    let inner = svg(10, 10, r#"<rect width="10" height="10" fill="red"/>"#);
    let image = |href: String| {
        let body = format!(r#"<image href="{href}" width="100" height="100"/>"#);
        draw(&svg(100, 100, &body), small())
    };
    // The same picture plain is drawn, so the gzip one is refused, not merely unreadable.
    assert!(painted(&image(data_url("image/svg+xml", &inner))) > 0);
    assert_eq!(painted(&image(data_url("image/svg+xml", &gzip(&inner)))), 0);
}

#[test]
fn an_svg_in_a_data_url_nested_too_deep_is_not_parsed() {
    let image = |inner: String| {
        let href = data_url("image/svg+xml", inner.as_bytes());
        let body = format!(r#"<image href="{href}" width="100" height="100"/>"#);
        draw(&svg(100, 100, &body), small())
    };
    assert!(painted(&image(nested(8))) > 0);
    assert_eq!(painted(&image(nested(100_000))), 0);
}

#[test]
fn an_svg_in_a_data_url_draws_no_image_of_its_own() {
    let image = |inner: &[u8]| {
        let href = data_url("image/svg+xml", inner);
        let body = format!(r#"<image href="{href}" width="100" height="100"/>"#);
        draw(&svg(100, 100, &body), small())
    };
    let picture = data_url("image/png", &png(2, 2, RED));
    let with_picture = svg(
        10,
        10,
        &format!(r#"<image href="{picture}" width="10" height="10"/>"#),
    );
    let with_rect = svg(10, 10, r#"<rect width="10" height="10" fill="red"/>"#);
    // The nested SVG is drawn, its own `data:` picture is not.
    assert!(painted(&image(&with_rect)) > 0);
    assert_eq!(painted(&image(&with_picture)), 0);
}

#[test]
fn embedded_svgs_together_beyond_the_budget_end_with_too_complex() {
    // One embedded SVG of 68 020 elements, drawn through 16 `<use>`: usvg reads it 16 times,
    // 1 088 320 elements in all - beyond the budget, though the document itself counts 35.
    let inner = data_url("image/svg+xml", use_bomb(4, 10).as_bytes());
    let uses = r##"<use href="#i"/>"##.repeat(16);
    let body =
        format!(r#"<defs><image id="i" href="{inner}" width="10" height="10"/></defs>{uses}"#);
    let outer = svg(10, 10, &body);
    assert_eq!(
        code_of(render(&outer, small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
    // An embedded use bomb alone is refused before usvg builds it.
    let bomb = data_url("image/svg+xml", use_bomb(10, 10).as_bytes());
    let body = format!(r#"<image href="{bomb}" width="10" height="10"/>"#);
    assert_eq!(
        code_of(render(&svg(10, 10, &body), small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
}

#[test]
fn a_few_markers_and_a_pattern_are_drawn() {
    let body = r##"<defs>
            <marker id="arrow" markerWidth="4" markerHeight="4" refX="2" refY="2">
                <circle cx="2" cy="2" r="2" fill="red"/>
            </marker>
            <pattern id="dots" width="4" height="4" patternUnits="userSpaceOnUse">
                <rect width="2" height="2" fill="red"/>
            </pattern>
        </defs>
        <rect width="50" height="50" fill="url(#dots)"/>
        <polyline points="60,10 80,30 60,50" fill="none" stroke="red"
            marker-start="url(#arrow)" marker-mid="url(#arrow)" marker-end="url(#arrow)"/>"##;
    assert!(painted(&draw(&svg(100, 100, body), small())) > 0);
}

#[test]
fn a_marker_on_every_vertex_beyond_the_budget_ends_with_too_complex() {
    let points = (0..1100)
        .map(|x| format!("{x},0"))
        .collect::<Vec<_>>()
        .join(" ");
    let body = format!(
        r##"<marker id="m">{}</marker><polyline points="{points}" marker-mid="url(#m)"/>"##,
        "<rect/>".repeat(1000)
    );
    assert_eq!(
        code_of(render(&svg(10, 10, &body), small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
}

#[test]
fn a_pattern_tile_beyond_the_pixel_budget_ends_with_too_complex() {
    // A 100 x 100 tile in a 10 x 10 picture drawn 512 pixels wide: 5 120 x 5 120 pixels.
    let body = r##"<pattern id="p" width="100" height="100" patternUnits="userSpaceOnUse">
            <rect width="1" height="1"/>
        </pattern>
        <rect width="10" height="10" fill="url(#p)"/>"##;
    assert_eq!(
        code_of(render(&svg(10, 10, body), small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
}

#[test]
fn an_arc_of_a_huge_radius_ends_with_too_complex_before_it_is_cut_up() {
    // Half an ellipse of radius 10^30: kurbo would cut it into some 75 000 curves, and svgtypes
    // take them out of its buffer from the front, one move of the rest per curve.
    let bomb = r#"<path d="M0 0 A1e30 1e30 0 0 1 2e30 0" fill="red"/>"#;
    assert_eq!(
        code_of(render(&svg(10, 10, bomb), small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
    // Embedded as a `data:` picture, it is not read, and the picture is refused.
    let inner = data_url("image/svg+xml", &svg(10, 10, bomb));
    let body = format!(r#"<image href="{inner}" width="10" height="10"/>"#);
    assert_eq!(
        code_of(render(&svg(10, 10, &body), small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
}

fn large() -> Settings {
    Settings {
        size: Size::Large,
        background: Background::Transparent,
    }
}

/// `depth` groups with `attribute`, one inside the next, around two rectangles covering a 10 x 10
/// picture, after `defs`.
fn nested_groups(defs: &str, attribute: &str, depth: usize) -> Vec<u8> {
    let rects = r#"<rect width="10" height="10" fill="red"/><rect width="5" height="5"/>"#;
    let open = format!("<g {attribute}>").repeat(depth);
    let close = "</g>".repeat(depth);
    svg(10, 10, &format!("{defs}{open}{rects}{close}"))
}

#[test]
fn nested_canvases_beyond_the_pixel_budget_end_with_too_complex() {
    // Drawn 2048 pixels wide, each group takes a canvas of the whole picture (4.2 M pixels), a
    // mask or a clip path one more of that size, a filter one per primitive and input.
    let mask = r#"<mask id="m"><rect width="10" height="10" fill="white"/></mask>"#;
    // usvg cuts a clipped group's canvas to the clip path: this one covers the picture.
    let clip = r#"<clipPath id="c"><rect width="10" height="10"/></clipPath>"#;
    let filter = r#"<filter id="f"><feGaussianBlur stdDeviation="1"/><feOffset dx="1"/></filter>"#;
    for (defs, attribute, depth) in [
        ("", r#"opacity="0.9""#, 20),
        (mask, r#"mask="url(#m)""#, 8),
        (clip, r#"clip-path="url(#c)""#, 10),
        (filter, r#"filter="url(#f)""#, 3),
    ] {
        assert_eq!(
            code_of(render(&nested_groups(defs, attribute, depth), large())).as_deref(),
            Some(code::TOO_COMPLEX),
            "{attribute} x {depth}"
        );
        // One of them is a picture (drawn small: the test build draws slowly).
        assert!(
            painted(&draw(&nested_groups(defs, attribute, 1), small())) > 0,
            "{attribute}"
        );
    }
}

#[test]
fn an_ordinary_picture_with_arcs_groups_a_filter_and_a_mask_is_drawn() {
    let body = r##"<defs>
            <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
                <feGaussianBlur in="SourceAlpha" stdDeviation="2"/>
                <feOffset dx="2" dy="2" result="blur"/>
                <feFlood flood-color="black" flood-opacity="0.5"/>
                <feComposite in2="blur" operator="in"/>
                <feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge>
            </filter>
            <mask id="fade">
                <rect width="100" height="100" fill="white"/>
                <circle cx="50" cy="50" r="20" fill="black"/>
            </mask>
        </defs>
        <g filter="url(#shadow)"><rect x="5" y="5" width="90" height="90" rx="10" fill="red"/></g>
        <g opacity="0.5">
            <circle cx="30" cy="30" r="20" fill="blue"/>
            <ellipse cx="70" cy="70" rx="20" ry="10" fill="green"/>
        </g>
        <g mask="url(#fade)">
            <path d="M10 50 A40 40 0 1 1 90 50 A40 40 0 1 1 10 50 Z" fill="yellow"/>
        </g>"##;
    let picture = draw(&svg(100, 100, body), Settings::default());
    assert!(painted(&picture) > 0);
    // Cheap: a few dozen curves, intermediate canvases of half the budget at most
    // (the shadow over the whole picture takes the most: a dozen canvases of 1291 x 1291).
    let text = String::from_utf8(svg(100, 100, body)).expect("UTF-8");
    let document = roxmltree::Document::parse(&text).expect("parses");
    let cost = budget::expanded(&document, Cost::LIMIT);
    assert!(cost.curves < 100, "{cost:?}");
    let tree = usvg::Tree::from_xmltree(&document, &options(Arc::new(Remaining::new(Cost::LIMIT))))
        .expect("builds");
    let pixels = canvas_pixels(
        &tree,
        (1024, 1024),
        Transform::from_scale(10.24, 10.24),
        Pixels::LIMIT,
    );
    assert!(pixels.layers < Pixels::LIMIT.layers / 2, "{pixels:?}");
}

#[test]
fn a_use_bomb_ends_with_too_complex() {
    assert_eq!(
        code_of(render(use_bomb(10, 10).as_bytes(), small())).as_deref(),
        Some(code::TOO_COMPLEX)
    );
    // A hundred copies are a picture.
    assert!(painted(&draw(use_bomb(2, 10).as_bytes(), small())) > 0);
}

#[test]
fn what_is_no_svg_is_not_svg() {
    for document in [
        b"<svg".to_vec(),
        b"\xff\xfe<\x00s\x00".to_vec(),
        b"plain text".to_vec(),
        Vec::new(),
    ] {
        assert_eq!(
            code_of(render(&document, small())).as_deref(),
            Some(code::NOT_SVG),
            "{document:?}"
        );
    }
}

#[test]
fn a_byte_order_mark_is_allowed() {
    let mut document = "\u{feff}".as_bytes().to_vec();
    document.extend(svg(10, 10, r#"<rect width="10" height="10" fill="red"/>"#));
    assert_eq!(pixel(&draw(&document, small()), 256, 256), RED);
}

#[test]
fn fit_keeps_the_aspect_ratio_and_one_pixel_at_least() {
    assert_eq!(fit(100.0, 50.0, 1024), Some((1024, 512)));
    assert_eq!(fit(50.0, 100.0, 512), Some((256, 512)));
    assert_eq!(fit(16.0, 16.0, 2048), Some((2048, 2048)));
    assert_eq!(fit(1.0, 100_000.0, 512), Some((1, 512)));
    assert_eq!(fit(0.0, 10.0, 512), None);
    assert_eq!(fit(f32::NAN, 10.0, 512), None);
    assert_eq!(fit(f32::INFINITY, 10.0, 512), None);
}
